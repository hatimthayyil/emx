{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.emacs-emx;
  
  # Get the Emacs package set for the selected Emacs package
  emacsPackages = pkgs.emacsPackagesFor cfg.package;
  
  # Process tree-sitter grammar selections based on user-provided grammar names
  treesitGrammars =
    if builtins.elem "all" cfg.treesitGrammars then
      # Include all available grammars when the special "all" name is specified
      emacsPackages.treesit-grammars.with-all-grammars
    else if cfg.treesitGrammars != [] then
      # For specific grammar selections, map our simple names to package references
      emacsPackages.treesit-grammars.with-grammars (grammars:
        map (name:
          grammars."tree-sitter-${name}" or
            (throw "Unknown treesit grammar: ${name}")
        ) cfg.treesitGrammars
      )
    else null;

  # Define grammar packages for inclusion in the final Emacs package
  grammarPackages = if treesitGrammars != null then [ treesitGrammars ] else [];
  
  # Define core Emacs packages to be managed by Nix
  # These should be stable packages or those with C dependencies
  coreEmacsPackages = epkgs: with epkgs; [
    use-package

    # emx-navigation
    dirvish

    vterm

    # emx-multimedia
    yeetube
    mpv
    empv

    # emx-programming
    #lsp-bridge

    # emx-research
    pdf-tools
  ];

  # Create the final Emacs package with grammars and core packages included
  finalEmacsPackage = emacsPackages.emacsWithPackages (epkgs:
    grammarPackages ++ (coreEmacsPackages epkgs));

  # Config source location in Nix store (always points to the immutable copy)
  src = "${../.}";  # This is the path to the EMX source in the Nix store
in
{
  options.programs.emacs-emx = {
    enable = lib.mkEnableOption "Emacs EmX configuration";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.emacs-pgtk;
      description = "The default Emacs derivation to use.";
    };
    
    treesitGrammars = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [];
      example = [ "rust" "python" "nix" ];
      description = ''
        List of tree-sitter grammar languages to include.
        Use "all" to include all available grammars.
        Example: [ "rust" "python" "nix" ]
        For all grammars: [ "all" ]
      '';
    };
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
    home.packages = [ finalEmacsPackage ] ++ [
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
        ${finalEmacsPackage}/bin/emacs --init-directory "$HOME/.config/emx" "''${args[@]}"
      '')
      ];

    home.sessionVariables = lib.mkIf cfg.defaultEditor {
      EDITOR = lib.getBin (
        pkgs.writeShellScript "editor" ''exec ${lib.getBin cfg.package}/bin/emacsclient "''${@:---create-frame}"''
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
        text = let
          # Extract date from Emacs package path for elpaca-core-date
          # See: https://github.com/progfolio/elpaca/wiki/Warnings-and-Errors
          emacsBuildDate =
            let
              packagePath = toString cfg.package;
              dateMatch = builtins.match ".*-([0-9]{8}).*" packagePath;
            in
              if dateMatch != null
              then builtins.head dateMatch
              else null;
        in ''
          ;; EMX initialization shim -*- lexical-binding: t -*-

          ;; Directory variables already set by early-init.el
          ;; Just ensure user-emacs-directory is set to our config dir
          (setq user-emacs-directory emx-config-dir)

          ;; Set Elpaca core date based on Emacs build date from Nix
          ${if emacsBuildDate != null then "(setq elpaca-core-date '(${emacsBuildDate}))" else ";; No build date found in package name"}

          ;; Load the actual init.el from source
          (load (expand-file-name "init.el" emx-source-dir))
        '';
      };
    };
  };
}
