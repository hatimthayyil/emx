# Example home-manager configuration using emx, with emx as a flake input.
# After the switch, run `emx sync` once to build Emacs into emx's profile.
{ config, inputs, ... }:

{
  imports = [ inputs.emx.homeModules.default ];

  programs.emx = {
    enable = true;
    # The emx repo: Emacs loads its configuration from it, and `emx sync` builds
    # Emacs and its packages from it.
    src = "${config.home.homeDirectory}/code/emx";
  };

  # Optional: run the Emacs daemon from emx's profile.
  services.emacs.enable = true;
}
