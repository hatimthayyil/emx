# Checks that emx's Emacs is correct: `nix flake check`.
{ pkgs
, emacs
, homeManager
, homeModule
, src
}:
let
  # Stands in for `pass` in the boot check: logs each call, prints a key and a
  # second line, and fails for the entry "missing".
  fakePass = pkgs.writeShellScriptBin "pass" ''
    echo "$*" >> "$EMX_PASS_LOG"
    if [ "$2" = missing ]; then
      echo "Error: $2 is not in the password store." >&2
      exit 1
    fi
    echo "key for $2"
    echo "second line"
  '';

  inherit (import ../init-files.nix { inherit (pkgs) lib; }) elispString initFiles;

  # The init directory the boot and reload checks start Emacs from: the init
  # files nix/home-module.nix writes to ~/.config/emx, whose early-init first
  # records warnings, with lisp/emx-warnings.el's filter. Emacs loads
  # site-start before any early-init, so the checks start it with
  # --no-site-file and the early-init loads site-start after the recording
  # begins, just before emx's early-init as Emacs would.
  bootInit = pkgs.linkFarm "emx-boot-init" (pkgs.lib.mapAttrs pkgs.writeText (initFiles {
    src = "${src}";
    prelude = ''
      (load ${elispString "${src}/lisp/emx-warnings.el"} nil t)
      (load ${elispString "${./record-warnings.el}"} nil t)
      (load "site-start" t t)
    '';
  }));

  packageList = pkgs.writeText "emx-packages" (
    pkgs.lib.concatLines (builtins.attrNames emacs.packageInputs)
  );

  home = homeManager.lib.homeManagerConfiguration {
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
        services.emacs.enable = true;
      }
    ];
  };
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

  # Builds a home-manager configuration using the module, and fails if its
  # closure holds emx's package set or Emacs, which only `emx sync` builds, if
  # it lacks the desktop entry's icon, or if the daemon's service would fail
  # rather than skip before the first sync.
  home-module =
    pkgs.runCommandLocal "emx-home-module-check"
      {
        exportReferencesGraph = [
          "closure"
          home.activationPackage
        ];
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
        touch $out
      '';

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
