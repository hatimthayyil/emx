{
  description = "Emacs configuration";

  inputs = {
    emacs-overlay = {
      url = "github:nix-community/emacs-overlay";
      flake = false;
    };
    melpa = {
      url = "github:melpa/melpa";
      flake = false;
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    twist.url = "github:emacs-twist/twist.nix";
  };

  outputs =
    { nixpkgs, ... }@inputs:
    let
      system = "x86_64-linux";
      trial = import ./nix/trial {
        inherit inputs;
        pkgs = nixpkgs.legacyPackages.${system};
      };
    in
    {
      apps.${system} = trial.twistApps;
      homeManagerModules.default = import ./nix/home-module.nix;
      packages.${system} = {
        emacs-nixpkgs = trial.nixpkgs;
        emacs-twist = trial.twist;
      };
    };
}
