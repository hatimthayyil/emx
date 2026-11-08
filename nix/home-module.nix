# Installs emx once: one `emx` package holding the CLI, `emacs` and
# `emacsclient` launchers that run Emacs from emx's profile, and Emacs's icons;
# an EMX menu entry, which opens a window on the daemon (starting it through
# libexec/emx/open-on-daemon when no server answers) or, without the daemon,
# runs plain `emacs`; and the init shims in ~/.config/emx that load the Elisp
# from the emx repo.
# `emx sync` builds Emacs, its packages and the tools its modules call into the
# profile, so home-manager never builds them and an update needs no switch.
{ self }:
{ config
, lib
, pkgs
, ...
}:
let
  cfg = config.programs.emx;

  system = pkgs.stdenv.hostPlatform.system;

  src = toString cfg.src;

  initDirectory = "${config.xdg.configHome}/emx";

  inherit (import ./init-files.nix { inherit lib; }) initFiles;

  # Runs NAME from emx's profile, which cli/src/profile.rs locates by the same
  # rule: $XDG_STATE_HOME when it is absolute, else ~/.local/state.
  launcher = name: args:
    lib.getExe (pkgs.writeShellApplication {
      inherit name;
      text = ''
        case "''${XDG_STATE_HOME:-}" in
          /*) state=$XDG_STATE_HOME ;;
          *) state=$HOME/.local/state ;;
        esac
        program=$state/emx/profile/bin/${name}
        if [ ! -x "$program" ]; then
          echo "${name}: emx's Emacs is not built yet; run 'emx sync'" >&2
          exit 1
        fi
        exec "$program" ${args} "$@"
      '';
    });

  # home-manager's unit for the Emacs daemon.
  daemonUnit = "emacs.service";

  # The EMX menu entry's fallback when no Emacs server answers. A daemon's
  # server appears only once its init finishes, so a plain Emacs started while
  # the daemon starts would take the server, and the daemon would exit when
  # its init ends. Starting the unit instead waits until the daemon is ready
  # (it is Type=notify), or does nothing if it runs. The @…@ paths are the
  # launchers of the package this is installed into.
  openOnDaemon = pkgs.writeShellApplication {
    name = "open-on-daemon";
    text = ''
      if systemctl --user start ${daemonUnit}; then
        exec @emacsclient@ --create-frame "$@"
      fi
      exec @emacs@ "$@"
    '';
  };

  emacs = self.packages.${system}.emacs.emacs;

  # Whether the session starts the daemon, as home-manager decides when it
  # gives emacs.service an [Install] section. The sync that creates emx's
  # profile starts it then (cli/src/daemon.rs), since the session's start
  # skipped while the profile was missing. emx is told the daemon's unit
  # whenever services.emacs runs the daemon, for its restart advice.
  sessionDaemon =
    config.services.emacs.enable && config.services.emacs.startWithUserSession != false;

  # The launchers and Emacs's application icons are copied in, so the closure
  # holds no separate launcher paths and no Emacs. `version` is Emacs's:
  # home-manager's Emacs service reads it to choose the socket and window class.
  emx = pkgs.runCommandLocal "emx"
    {
      inherit (emacs) version;
      nativeBuildInputs = [ pkgs.makeWrapper ];
      meta.mainProgram = "emx";
    } ''
    makeWrapper ${lib.getExe self.packages.${system}.emx} $out/bin/emx \
      --set-default EMX_SRC ${lib.escapeShellArg src} \
      ${lib.optionalString config.services.emacs.enable "--set EMX_DAEMON_UNIT ${daemonUnit}"} \
      ${lib.optionalString sessionDaemon "--set EMX_DAEMON_STARTS_WITH_SESSION 1"}
    install -m555 ${launcher "emacs" "--init-directory ${lib.escapeShellArg initDirectory}"} $out/bin/emacs
    install -m555 ${launcher "emacsclient" ""} $out/bin/emacsclient
    install -Dm555 ${lib.getExe openOnDaemon} $out/libexec/emx/open-on-daemon
    substituteInPlace $out/libexec/emx/open-on-daemon \
      --subst-var-by emacsclient $out/bin/emacsclient \
      --subst-var-by emacs $out/bin/emacs

    cd ${emacs}/share/icons
    find hicolor -path '*/apps/emacs.png' -o -path '*/apps/emacs.svg' |
      while read -r icon; do install -Dm444 "$icon" "$out/share/icons/$icon"; done
  '';
in
{
  options.programs.emx = {
    enable = lib.mkEnableOption "emx, an Emacs distribution";

    src = lib.mkOption {
      type = lib.types.pathWith {
        absolute = true;
        inStore = false;
      };
      example = "/home/alice/code/emx";
      description = ''
        The emx repo. Emacs loads its configuration from it, and
        {command}`emx sync` builds Emacs and its packages from it.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ emx ];

    # services.emacs.enable runs the daemon from emx's profile. The service
    # skips until the profile exists, and the `emx sync` creating it starts it
    # if the session would have.
    services.emacs.package = lib.mkDefault emx;
    systemd.user.services.emacs = lib.mkIf config.services.emacs.enable {
      Unit.ConditionPathExists = "${config.xdg.stateHome}/emx/profile/bin/emacs";
    };

    assertions = [{
      assertion = !(config.services.emacs.enable && config.services.emacs.socketActivation.enable);
      message = ''
        programs.emx: turn off services.emacs.socketActivation. emx finds a
        running Emacs by connecting to its server socket, so socket activation
        would start the daemon on every emx sync.
      '';
    }];

    # With the daemon, windows open on it, where the server socket `emx sync`
    # reloads through also lives. With no server answering, open-on-daemon
    # starts the daemon and opens the window there, and starts a plain Emacs
    # only if systemd can't. An empty --alternate-editor would start a second
    # daemon that systemd does not manage.
    xdg.desktopEntries.emx = {
      name = "EMX";
      genericName = "Text Editor";
      comment = "Edit text";
      exec =
        if config.services.emacs.enable
        then "${emx}/bin/emacsclient --create-frame --alternate-editor=${emx}/libexec/emx/open-on-daemon %F"
        else "${emx}/bin/emacs %F";
      terminal = false;
      type = "Application";
      icon = "emacs";
      categories = [ "Development" "TextEditor" ];
      mimeType = [ "text/plain" ];
      startupNotify = true;
      # The Wayland app_id pgtk Emacs gives its windows, a daemon's included,
      # so the task manager matches them to this entry. Upstream's
      # emacs.desktop names X11's class, Emacs.
      settings.StartupWMClass = "emacs";
    };

    xdg.configFile = lib.mapAttrs'
      (name: text: lib.nameValuePair "emx/${name}" { inherit text; })
      (initFiles {
        inherit src;
        daemonOwnsServer = config.services.emacs.enable;
      });
  };
}
