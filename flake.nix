{
  description = "emx, an Emacs distribution";

  inputs = {
    # Only the home-manager module check uses it.
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    melpa = {
      url = "github:melpa/melpa";
      flake = false;
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    twist.url = "github:hatimbt/twist.nix";
  };

  outputs =
    { self, nixpkgs, ... }@inputs:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      emacs = import ./nix/emacs { inherit inputs pkgs; };
      emx = pkgs.callPackage ./nix/cli.nix { };
    in
    {
      checks.${system} = import ./nix/checks {
        inherit pkgs emacs;
        homeManager = inputs.home-manager;
        homeModule = self.homeManagerModules.default;
        src = self;
      };

      homeManagerModules.default = import ./nix/home-module.nix { inherit self; };

      # `nix run --impure .#lock` regenerates lock/ from packages.el, and
      # `nix run --impure .#update` moves the ELPA archive packages to their
      # newest releases. Both read the archives live, so they need --impure;
      # as apps, `nix flake check` would evaluate them and fail.
      legacyPackages.${system} = emacs.makeApps { lockDirName = "lock"; };

      # `emx sync` builds `emacs` into emx's profile; home-manager installs `emx`.
      packages.${system} = {
        default = emacs;
        inherit emacs emx;
      };
    };
}
