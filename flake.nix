{
  description = "emx, an Emacs distribution";

  inputs = {
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
    in
    {
      checks.${system} = import ./nix/checks {
        inherit pkgs emacs;
        src = self;
      };

      homeManagerModules.default = import ./nix/home-module.nix { inherit self; };

      # `nix run --impure .#lock` regenerates lock/ from packages.el, and
      # `nix run --impure .#update` moves the ELPA archive packages to their
      # newest releases. Both read the archives live, so they need --impure;
      # as apps, `nix flake check` would evaluate them and fail.
      legacyPackages.${system} = emacs.makeApps { lockDirName = "lock"; };

      packages.${system} = {
        default = emacs;
        inherit emacs;
      };
    };
}
