# Development environment for emx: the Rust toolchain for the emx CLI (cli/),
# formatters for Nix and Rust, and commit hooks.
{ pkgs, ... }:
{
  # dix (cli/) reads the Nix store database through SQLite; `emx sync` renders
  # build logs with nom.
  packages = [ pkgs.nix-output-monitor pkgs.pkg-config pkgs.sqlite ];

  languages.rust = {
    enable = true;
    channel = "stable";
  };

  treefmt = {
    enable = true;
    config = {
      programs = {
        nixpkgs-fmt.enable = true;
        rustfmt.enable = true;
      };
      # twist writes lock/ (nix run --impure .#lock).
      settings.global.excludes = [ "lock/*" ];
    };
  };

  git-hooks.hooks = {
    treefmt.enable = true;
    clippy = {
      enable = true;
      settings = {
        denyWarnings = true;
        extraArgs = "--all-targets";
        offline = false;
      };
    };
  };

  enterTest = ''
    cargo test
  '';
}
