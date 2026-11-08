# Checks that emx's Emacs is correct: `nix flake check`.
{ pkgs
, emacs
, homeManager
, homeModule
, src
}:
let
  # Stands in for `pass` in the boot check: logs each call and prints a key and
  # a second line. It warns on stderr first for the entry "noisy", exits 1 after
  # a line for "missing", and prints nothing for "empty".
  fakePass = pkgs.writeShellScriptBin "pass" ''
    echo "$*" >> "$EMX_PASS_LOG"
    case "$2" in
      noisy)
        echo "gpg: WARNING: a warning from gpg" >&2
        ;;
      missing)
        echo "partial output"
        echo "Error: $2 is not in the password store." >&2
        exit 1
        ;;
      empty)
        exit 0
        ;;
    esac
    echo "key for $2"
    echo "second line"
  '';

  inherit (import ../init-files.nix { inherit (pkgs) lib; }) elispString initFiles;

  # The init directory the boot and reload checks start Emacs from: the init
  # files nix/home-module.nix writes to ~/.config/emx, whose early-init first
  # records warnings, with lisp/emx-warnings.el's filter. Emacs loads
  # site-start before any early-init, so the checks start it with
  # --no-site-file and the early-init loads site-start after the recording
  # begins, just before emx's early-init as Emacs would. bootInitWith's
  # PRELUDE runs after site-start, and DAEMONOWNSSERVER writes the init files
  # as the module does when services.emacs runs the daemon.
  bootInitWith = { prelude ? "", daemonOwnsServer ? false }:
    pkgs.linkFarm "emx-boot-init" (pkgs.lib.mapAttrs pkgs.writeText (initFiles {
      src = "${src}";
      inherit daemonOwnsServer;
      prelude = ''
        (load ${elispString "${src}/lisp/emx-warnings.el"} nil t)
        (load ${elispString "${./record-warnings.el}"} nil t)
        (load "site-start" t t)
      '' + prelude;
    }));

  bootInit = bootInitWith { };

  # The init directory of the daemon check's Emacs processes, which load
  # daemon.el first.
  daemonCheckInit = daemonOwnsServer: bootInitWith {
    inherit daemonOwnsServer;
    prelude = ''
      (load ${elispString "${./daemon.el}"} nil t)
    '';
  };

  packageList = pkgs.writeText "emx-packages" (
    pkgs.lib.concatLines (builtins.attrNames emacs.packageInputs)
  );

  # The font families emx's configuration names, one per line: each
  # `…-family` in Fontaine's presets, read once emx's real init, booted
  # through bootInit's shims, has loaded Fontaine. Fails if that boot went
  # wrong.
  configFontFamilies = pkgs.runCommandLocal "emx-font-families" { } ''
    export HOME=$TMPDIR
    mkdir -p $HOME/Documents/org
    ${emacs}/bin/emacs --batch --no-site-file -u "" --init-directory=${bootInit} \
      --eval '(require (quote fontaine))' -l ${pkgs.writeText "font-families.el" ''
        (when-let* ((failures (emx-boot-check-boot-failures)))
          (error "emx's init didn't boot cleanly: %s" (string-join failures "; ")))
        (let (families)
          (dolist (preset fontaine-presets)
            (let ((properties (cdr preset)))
              (while properties
                (when (and (string-suffix-p "-family" (symbol-name (car properties)))
                           (stringp (cadr properties)))
                  (push (cadr properties) families))
                (setq properties (cddr properties)))))
          (dolist (family (delete-dups (nreverse families)))
            (princ (concat family "\n"))))
      ''} > $out
  '';

  # A home-manager configuration using the module, with services.emacs set to
  # `emacsService`.
  homeWith = emacsService: homeManager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
      homeModule
      {
        home = {
          username = "emx";
          homeDirectory = "/home/emx";
          stateVersion = "26.05";
        };
        programs.emx = {
          enable = true;
          src = "/home/emx/code/emx";
        };
        services.emacs = emacsService;
      }
    ];
  };

  home = homeWith { enable = true; };

  homeWithoutDaemon = homeWith { enable = false; };

  # The daemon is installed, but the session doesn't start it.
  homeWithManualDaemon = homeWith {
    enable = true;
    startWithUserSession = false;
  };

  # Whether the module refuses services.emacs.socketActivation. home-manager
  # throws for a failed assertion once the configuration is read.
  refusesSocketActivation = !(builtins.tryEval (homeWith {
    enable = true;
    socketActivation.enable = true;
  }).config.home.username).success;

  # The EMX menu entry of a home-manager configuration.
  menuEntry = h: "${h.config.home.path}/share/applications/emx.desktop";

  # The emx package a home-manager configuration installs.
  emxOf = h: h.config.services.emacs.package;
in
{
  # Boots with emx's real init through bootInit's shims, then loads every
  # package in the set, including dependencies. Batch mode loads init files
  # only when -u names a user.
  # magit computes the git version when it loads.
  boot = pkgs.runCommandLocal "emx-boot-check"
    {
      EMX_PACKAGES = packageList;
      nativeBuildInputs = [ fakePass pkgs.git ];
    } ''
    export HOME=$TMPDIR
    export EMX_PASS_LOG=$TMPDIR/pass.log
    # A false value beats claude --ide, so claude-code-ide's Claude must still
    # get true.
    export CLAUDE_CODE_AUTO_CONNECT_IDE=false
    # org-directory (modules/organisation.el); org-node's :config fails
    # when it is missing.
    mkdir -p $HOME/Documents/org
    ${emacs}/bin/emacs --batch --no-site-file -u "" --init-directory=${bootInit} \
      -l ${./boot.el}
    touch $out
  '';

  # Boots like `boot`, then has emx-reload bring the running Emacs up to date:
  # with its own generation, with a made-up one that moves packages, tools,
  # native code and manuals, and with one running a different Emacs.
  reload = pkgs.runCommandLocal "emx-reload-check"
    {
      EMX_EMACS = "${emacs}";
      nativeBuildInputs = [ fakePass pkgs.git ];
    } ''
    export HOME=$TMPDIR
    export EMX_PASS_LOG=$TMPDIR/pass.log
    mkdir -p $HOME/Documents/org
    ${emacs}/bin/emacs --batch --no-site-file -u "" --init-directory=${bootInit} \
      -l ${./reload.el}
    touch $out
  '';

  # Starts a daemon as emacs.service does, through daemon.el, and fails if a
  # plain Emacs started while it initialises takes the server from it, if it
  # isn't set up as a standalone Emacs is, if a Fontaine preset gives a face a
  # font family emx's configuration doesn't name, or none to the default and
  # fixed-pitch faces, or if a plain Emacs started beside it, with no daemon
  # owning the server, starts a server or warns.
  daemon = pkgs.runCommandLocal "emx-daemon-check"
    {
      EMX_FONT_FAMILIES = configFontFamilies;
      nativeBuildInputs = [ fakePass pkgs.git ];
    } ''
    export HOME=$TMPDIR
    export EMX_PASS_LOG=$TMPDIR/pass.log
    mkdir -p $HOME/Documents/org
    # Each server's socket is $XDG_RUNTIME_DIR/emacs/server, whose path must
    # fit in 108 bytes.
    export XDG_RUNTIME_DIR=$TMPDIR/run
    mkdir -m 700 $XDG_RUNTIME_DIR
    unset EMACS_SOCKET_NAME
    export EMX_CHECK_DIR=$TMPDIR/check
    mkdir $EMX_CHECK_DIR
    emacs=${emacs}/bin/emacs
    emacsclient=${emacs}/bin/emacsclient

    EMX_CHECK_HOLD_DAEMON=1 $emacs --fg-daemon --no-site-file \
      --init-directory=${daemonCheckInit true} > daemon.log 2>&1 &
    daemon=$!
    $emacs --batch --no-site-file -u "" --init-directory=${daemonCheckInit true} \
      -f emx-daemon-check-plain-during-init &
    plain=$!
    tries=0
    until [ "$($emacsclient --eval '(daemonp)' 2>/dev/null)" = t ]; do
      if ! kill -0 $daemon 2>/dev/null; then
        echo "the daemon exited while a plain Emacs started during its init ran:"
        cat daemon.log
        exit 1
      fi
      tries=$((tries + 1))
      if [ $tries -gt 900 ]; then
        echo "the daemon didn't start within three minutes:"
        cat daemon.log
        exit 1
      fi
      sleep 0.2
    done
    $emacsclient --eval '(emx-daemon-check-setup)' > /dev/null
    touch $EMX_CHECK_DIR/daemon-checked
    failed=
    wait $plain || failed=1
    $emacs --batch --no-site-file -u "" --init-directory=${daemonCheckInit false} \
      -f emx-daemon-check-plain-beside || failed=1
    if [ -s $EMX_CHECK_DIR/setup-failures ]; then
      sed 's/^/The daemon: /' $EMX_CHECK_DIR/setup-failures
      failed=1
    fi
    $emacsclient --eval '(kill-emacs)' > /dev/null
    wait $daemon

    # A daemon whose init files don't mark it the server's owner, as when
    # emacs.service runs without the home module's daemon setting.
    $emacs --fg-daemon --no-site-file \
      --init-directory=${daemonCheckInit false} > own-server.log 2>&1 &
    daemon=$!
    tries=0
    until [ "$($emacsclient --eval '(daemonp)' 2>/dev/null)" = t ]; do
      if ! kill -0 $daemon 2>/dev/null; then
        echo "a daemon not marked the server's owner exited during its start:"
        cat own-server.log
        exit 1
      fi
      tries=$((tries + 1))
      if [ $tries -gt 900 ]; then
        echo "a daemon not marked the server's owner didn't start within three minutes:"
        cat own-server.log
        exit 1
      fi
      sleep 0.2
    done
    $emacsclient --eval '(emx-daemon-check-own-server)' > /dev/null
    if [ -s $EMX_CHECK_DIR/own-server-failures ]; then
      sed "s/^/A daemon not marked the server's owner: /" \
        $EMX_CHECK_DIR/own-server-failures
      failed=1
    fi
    $emacsclient --eval '(kill-emacs)' > /dev/null
    wait $daemon
    [ -z "$failed" ]
    touch $out
  '';

  # Builds a home-manager configuration using the module, and fails if its
  # closure holds emx's package set or Emacs, which only `emx sync` builds, if
  # it lacks the desktop entry's icon, if the daemon's service would fail
  # rather than skip before the first sync, if the EMX menu entry opens a
  # standalone Emacs while the daemon runs or starts, if the module allows
  # socket activation, if a plain Emacs isn't told to leave the server to the
  # daemon exactly when services.emacs runs it, if emx isn't told the daemon's
  # unit, if emx's first sync starts the daemon when the session wouldn't or
  # doesn't when it would, or if fontconfig can't find a font family emx's
  # configuration names.
  home-module =
    pkgs.runCommandLocal "emx-home-module-check"
      {
        exportReferencesGraph = [
          "closure"
          home.activationPackage
        ];
        nativeBuildInputs = [ pkgs.fontconfig ];
      }
      ''
        for path in ${builtins.unsafeDiscardStringContext emacs.outPath} \
                    ${builtins.unsafeDiscardStringContext emacs.emacs.outPath}; do
          if grep -qF "$path" closure; then
            echo "the home-manager module depends on $path"
            exit 1
          fi
        done
        if [ ! -e ${home.config.home.path}/share/icons/hicolor/scalable/apps/emacs.svg ]; then
          echo "the home-manager generation lacks Emacs's icon"
          exit 1
        fi
        if ! grep -qxF ConditionPathExists=${home.config.xdg.stateHome}/emx/profile/bin/emacs \
            ${home.activationPackage}/home-files/.config/systemd/user/emacs.service; then
          echo "emacs.service does not wait for emx's profile"
          exit 1
        fi
        if ! grep -qxF "Exec=${emxOf home}/bin/emacsclient --create-frame --alternate-editor=${emxOf home}/libexec/emx/open-on-daemon %F" \
            ${menuEntry home}; then
          echo "with the daemon, the EMX menu entry does not open a window on it"
          exit 1
        fi
        fallback=${emxOf home}/libexec/emx/open-on-daemon
        if ! grep -qxF "if systemctl --user start emacs.service; then" $fallback ||
            ! grep -qF "exec ${emxOf home}/bin/emacsclient --create-frame" $fallback ||
            ! grep -qF "exec ${emxOf home}/bin/emacs " $fallback; then
          echo "the EMX menu entry's fallback does not start the daemon before opening a window on it"
          exit 1
        fi
        if ! grep -qxF "Exec=${emxOf homeWithoutDaemon}/bin/emacs %F" ${menuEntry homeWithoutDaemon}; then
          echo "without the daemon, the EMX menu entry does not start Emacs"
          exit 1
        fi
        if ! grep -qxF StartupWMClass=emacs ${menuEntry home}; then
          echo "the EMX menu entry does not name Emacs's window class"
          exit 1
        fi
        ${pkgs.lib.optionalString (!refusesSocketActivation) ''
          echo "the module allows services.emacs.socketActivation, which would start the daemon on every sync"
          exit 1
        ''}
        if ! grep -qxF '(setq emx-daemon-owns-server t)' ${home.activationPackage}/home-files/.config/emx/early-init.el ||
            grep -qF emx-daemon-owns-server ${homeWithoutDaemon.activationPackage}/home-files/.config/emx/early-init.el; then
          echo "a plain Emacs is not told to leave the server to the daemon exactly when services.emacs runs it"
          exit 1
        fi
        for emx in ${emxOf home} ${emxOf homeWithManualDaemon}; do
          if ! grep -q 'EMX_DAEMON_UNIT.*emacs\.service' $emx/bin/emx; then
            echo "with the daemon, emx is not told its unit: $emx"
            exit 1
          fi
        done
        if grep -q EMX_DAEMON_UNIT ${emxOf homeWithoutDaemon}/bin/emx; then
          echo "without the daemon, emx is told of a daemon unit"
          exit 1
        fi
        if ! grep -q 'EMX_DAEMON_STARTS_WITH_SESSION.*1' ${emxOf home}/bin/emx; then
          echo "when the session starts the daemon, emx is not told to start it on the first sync"
          exit 1
        fi
        for emx in ${emxOf homeWithoutDaemon} ${emxOf homeWithManualDaemon}; do
          if grep -q EMX_DAEMON_STARTS_WITH_SESSION $emx/bin/emx; then
            echo "emx is told to start a daemon that the session does not start: $emx"
            exit 1
          fi
        done
        if [ ! -e ${home.activationPackage}/home-files/.config/fontconfig/conf.d/10-hm-fonts.conf ]; then
          echo "fontconfig does not look in the profile, so Emacs can't find emx's fonts"
          exit 1
        fi
        families=$(FONTCONFIG_FILE=${pkgs.makeFontsConf { fontDirectories = [ ]; }} XDG_CACHE_HOME=$TMPDIR \
          fc-scan --format '%{family}\n' ${home.config.home.path}/share/fonts | tr , '\n')
        if [ ! -s ${configFontFamilies} ]; then
          echo "found no font family in emx's configuration"
          exit 1
        fi
        while IFS= read -r family; do
          if ! grep -qxF "$family" <<<"$families"; then
            echo "the home-manager generation lacks the font family $family, which emx's configuration names"
            exit 1
          fi
        done < ${configFontFamilies}
        touch $out
      '';

  # Every package's pin satisfies the versions the packages that need it ask
  # for, so a named `emx upgrade`, which keeps the ELPA pins, can't leave a
  # moved package needing a newer ELPA release.
  dependencies = emacs.depsCheck;

  # initialLibraries must list the libraries of the Emacs being built.
  builtin-libraries = pkgs.runCommandLocal "emx-builtin-libraries-check" { } ''
    if ! diff -u ${../emacs/builtin-libraries.txt} ${emacs.builtinLibraryList}; then
      echo "nix/emacs/builtin-libraries.txt does not match Emacs ${emacs.emacs.version}. Regenerate it:"
      echo '  cp $(nix build --no-link --print-out-paths .#emacs.builtinLibraryList) nix/emacs/builtin-libraries.txt'
      exit 1
    fi
    touch $out
  '';

  # Every file carries a licence, in REUSE.toml or in its own header.
  licences = pkgs.runCommandLocal "emx-licences-check"
    {
      nativeBuildInputs = [ pkgs.reuse ];
    } ''
    reuse --root ${src} lint
    touch $out
  '';
}
