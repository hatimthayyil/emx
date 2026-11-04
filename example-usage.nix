# Example home-manager configuration using emx, with emx as a flake input.
{ inputs, ... }:

{
  imports = [ inputs.emx.homeManagerModules.default ];

  programs.emacs-emx = {
    enable = true;
    # Load the Elisp from this checkout instead of the store copy.
    localPath = "/path/to/emx";
  };
}
