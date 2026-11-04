# Installs emx: its Emacs, the tools its modules call, the `emx` launcher and
# the init shims in ~/.config/emx.
{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.emacs-emx;

  emacs = self.packages.${pkgs.stdenv.hostPlatform.system}.emacs;

  # The emx source in the Nix store, loaded unless the launcher runs in
  # development mode.
  src = "${self}";
in
{
  options.programs.emacs-emx = {
    enable = lib.mkEnableOption "Emacs EmX configuration";

    localPath = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Local path to your flake checkout (containing the "emacs" folder).
        If set, Emacs will load config from this directory at runtime,
        bypassing the immutable store copy.
      '';
    };

    defaultEditor = lib.mkOption rec {
      type = lib.types.bool;
      default = false;
      example = !default;
      description = ''
        Whether to configure {command}`emacsclient` as the default
        editor using the {env}`EDITOR` environment variable.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # Install the Emacs package and EMX launcher script
    home.packages = [ emacs ] ++ [
        pkgs.ripgrep
        pkgs.fd
        pkgs.yt-dlp
        pkgs.parinfer-rust-emacs

        pkgs.vale # syntax aware prose linter
        pkgs.vale-ls # LSP for vale
        pkgs.valeStyles.write-good

        pkgs.basedpyright # Python LSP
        pkgs.ruff # Python linter and formatter
        pkgs.beancount-language-server # TODO split off into a "personal"
      ] ++ [
      (pkgs.writeShellScriptBin "emx" ''
        # Process arguments to detect --dev flag
        args=()
        dev_mode=1
        
        for arg in "$@"; do
          if [ "$arg" = "--stable" ]; then
            dev_mode=0
          else
            args+=("$arg")
          fi
        done
        
        # Check if dev mode is requested but localPath isn't configured
        if [ $dev_mode -eq 1 ] && [ -z "${if cfg.localPath != null then cfg.localPath else ""}" ]; then
          echo "Error: Cannot use --dev flag because localPath is not configured"
          echo "Add this to your Home Manager configuration:"
          echo "  programs.emacs-emx.localPath = \"/path/to/emx/source\";"
          exit 1
        fi
        
        # Set environment variable based on mode
        if [ $dev_mode -eq 1 ]; then
          export EMX_DEV_MODE=1
          echo "Starting EMX in development mode (using localPath)"
        else
          unset EMX_DEV_MODE
        fi
        
        # Launch Emacs with proper init directory
        ${emacs}/bin/emacs --init-directory "$HOME/.config/emx" "''${args[@]}"
      '')
      ];

    home.sessionVariables = lib.mkIf cfg.defaultEditor {
      EDITOR = lib.getBin (
        pkgs.writeShellScript "editor" ''exec ${emacs}/bin/emacsclient "''${@:---create-frame}"''
      );
    };
    
    # Set up desktop entry for EMX
    xdg.desktopEntries.emx = {
      name = "EMX";
      genericName = "Text Editor";
      comment = "Edit text";
      exec = "emx %F";
      terminal = false;
      type = "Application";
      icon = "emacs";
      categories = [ "Development" "TextEditor" ];
      mimeType = [ "text/plain" ];
      startupNotify = true;
    };

    # Create XDG-compliant configuration directory with shims
    xdg.configFile = {
      # Early init shim
      "emx/early-init.el" = {
        text = ''
          ;; EMX early initialization shim -*- lexical-binding: t -*-

          ;; Determine source directory based on mode (development or stable)
          (defvar emx-source-dir
            (if (getenv "EMX_DEV_MODE")
                ;; Development mode - load from localPath
                (progn
                  (message "EMX: Development mode active (using localPath)")
                  "${cfg.localPath}")
              ;; Stable mode - load from Nix store
              "${src}")
            "Source directory of EMX configuration.")

          ;; Define config/data/cache directories for XDG compliance
          (defvar emx-config-dir (expand-file-name "~/.config/emx/")
            "Directory for EMX configuration files.")

          (defvar emx-data-dir (expand-file-name "~/.local/share/emx/")
            "Directory for EMX data files.")

          (defvar emx-cache-dir (expand-file-name "~/.cache/emx/")
            "Directory for EMX cache files.")

          ;; Load the actual early-init.el from source
          (load (expand-file-name "early-init.el" emx-source-dir))
        '';
      };

      # Main init.el shim
      "emx/init.el" = {
        text = ''
          ;; EMX initialization shim -*- lexical-binding: t -*-

          ;; Directory variables already set by early-init.el
          ;; Just ensure user-emacs-directory is set to our config dir
          (setq user-emacs-directory emx-config-dir)

          ;; Load the actual init.el from source
          (load (expand-file-name "init.el" emx-source-dir))
        '';
      };
    };
  };
}
