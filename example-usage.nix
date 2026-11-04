# Example home-manager configuration using emx, with emx as a flake input.
# After the switch, run `emx sync` once to build Emacs into emx's profile.
{ config, inputs, ... }:

{
  imports = [ inputs.emx.homeManagerModules.default ];

  programs.emacs-emx = {
    enable = true;
    # Emacs loads its configuration from this checkout, and `emx sync` builds
    # Emacs and its packages from it.
    checkout = "${config.home.homeDirectory}/code/emx";
  };

  # Optional: run the Emacs daemon from emx's profile.
  services.emacs.enable = true;
}
