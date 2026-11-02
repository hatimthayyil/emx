# Two builds of the same package set (./init.el) for comparison: twist, from
# upstream sources pinned in ./lock, and nixpkgs' package set through
# emacs-overlay's use-package parser.
{ pkgs, inputs }:
let
  emacsPackage = pkgs.emacs-pgtk;

  grammars = (pkgs.emacsPackagesFor emacsPackage).treesit-grammars.with-all-grammars;

  twistEnv =
    (inputs.twist.lib.makeEnv {
      inherit pkgs emacsPackage;
      initFiles = [ ./init.el ];
      lockDir = ./lock;
      registries = [
        {
          name = "custom";
          type = "melpa";
          path = ./recipes;
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
      inputOverrides = import ./twist-input-overrides.nix;
      exportManifest = true;
      persistMetadata = true;
      initialLibraries = builtins.filter (s: builtins.isString s && s != "") (
        builtins.split "\n" (builtins.readFile ./builtin-libraries.txt)
      );
      extraSiteStartElisp = ''
        (add-to-list 'treesit-extra-load-path "${grammars}/lib/")
      '';
    }).overrideScope
      (
        _: prev: {
          elispPackages = prev.elispPackages.overrideScope (
            import ./twist-overrides.nix { inherit pkgs emacsPackage; }
          );
        }
      );
in
{
  twist = twistEnv;

  twistApps = twistEnv.makeApps { lockDirName = "nix/trial/lock"; };

  nixpkgs = import "${inputs.emacs-overlay}/elisp.nix" { inherit pkgs; } {
    package = emacsPackage;
    config = ./init.el;
    extraEmacsPackages = epkgs: [ epkgs.treesit-grammars.with-all-grammars ];
    override = import ./nixpkgs-overrides.nix { inherit pkgs; };
  };
}
