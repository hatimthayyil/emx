# emx's Emacs: emacs-pgtk with every package in ../../packages.el, built by
# twist from upstream sources pinned in ../../lock, and the tools its modules
# call.
{ pkgs, inputs }:
let
  emacsPackage = pkgs.emacs-pgtk;

  grammars = (pkgs.emacsPackagesFor emacsPackage).treesit-grammars.with-all-grammars;
in
(inputs.twist.lib.makeEnv {
  inherit pkgs emacsPackage;
  initFiles = [ ../../packages.el ];
  lockDir = ../../lock;
  registries = [
    {
      name = "custom";
      type = "melpa";
      path = ../../recipes;
    }
    {
      name = "melpa";
      type = "melpa";
      path = inputs.melpa + "/recipes";
    }
    {
      name = "gnu";
      type = "archive";
      url = "https://elpa.gnu.org/packages/";
    }
    {
      name = "nongnu";
      type = "archive";
      url = "https://elpa.nongnu.org/nongnu/";
    }
  ];
  inputOverrides = import ./input-overrides.nix;
  exportManifest = true;
  # Together with initialLibraries, this keeps evaluation free of
  # import-from-derivation: package headers are read from lock/metadata.json.
  persistMetadata = true;
  # Must match emacsPackage's version; the builtin-libraries check fails when
  # it does not.
  initialLibraries = builtins.filter (s: builtins.isString s && s != "") (
    builtins.split "\n" (builtins.readFile ./builtin-libraries.txt)
  );
  extraSiteStartElisp = ''
    (add-to-list 'treesit-extra-load-path "${grammars}/lib/")
    (defvar parinfer-rust-library "${pkgs.parinfer-rust-emacs}/lib/libparinfer_rust.so")
  '';
}).overrideScope
  (
    _: prev: {
      elispPackages = prev.elispPackages.overrideScope (import ./overrides.nix { inherit pkgs; });

      # Emacs's wrapper prefixes these to PATH, so Emacs and the processes it
      # starts find them and the user's shell does not.
      executablePackages = [
        pkgs.ripgrep
        pkgs.fd
        pkgs.yt-dlp
        # Runs the `claude` on PATH: nixpkgs would pin its own, unfree one.
        (pkgs.claude-agent-acp.override {
          claude-code = pkgs.writeShellScriptBin "claude" ''exec claude "$@"'';
        })

        (pkgs.vale.withStyles (styles: [ styles.write-good ])) # syntax aware prose linter
        pkgs.vale-ls # LSP for vale

        pkgs.basedpyright # Python LSP
        pkgs.ruff # Python linter and formatter
        pkgs.beancount-language-server # TODO split off into a "personal"
      ];
    }
  )
