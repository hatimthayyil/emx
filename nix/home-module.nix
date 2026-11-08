# Installs emx once: one `emx` package holding the CLI, `emacs` and
# `emacsclient` launchers that run Emacs from emx's profile, and Emacs's icons;
# and the init shims in ~/.config/emx that load the Elisp from the emx repo.
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

  emacs = self.packages.${system}.emacs.emacs;

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
      --set-default EMX_SRC ${lib.escapeShellArg src}
    install -m555 ${launcher "emacs" "--init-directory ${lib.escapeShellArg initDirectory}"} $out/bin/emacs
    install -m555 ${launcher "emacsclient" ""} $out/bin/emacsclient

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
    # skips until the profile exists, and the `emx sync` creating it starts it.
    services.emacs.package = lib.mkDefault emx;
    systemd.user.services.emacs = lib.mkIf config.services.emacs.enable {
      Unit.ConditionPathExists = "${config.xdg.stateHome}/emx/profile/bin/emacs";
    };

    xdg.desktopEntries.emx = {
      name = "EMX";
      genericName = "Text Editor";
      comment = "Edit text";
      exec = "${emx}/bin/emacs %F";
      terminal = false;
      type = "Application";
      icon = "emacs";
      categories = [ "Development" "TextEditor" ];
      mimeType = [ "text/plain" ];
      startupNotify = true;
    };

    xdg.configFile = lib.mapAttrs'
      (name: text: lib.nameValuePair "emx/${name}" { inherit text; })
      (initFiles { inherit src; });
  };
}
