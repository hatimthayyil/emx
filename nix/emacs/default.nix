# emx's Emacs: emacs-pgtk with every package in ../../packages.el, built by
# twist from upstream sources pinned in ../../lock.
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
  '';
}).overrideScope
  (
    _: prev: {
      elispPackages = prev.elispPackages.overrideScope (import ./overrides.nix { inherit pkgs; });
    }
  )
