# Development environment for emx: the Rust toolchain for the emx CLI (cli/),
# formatters for Nix and Rust, and commit hooks.
{ ... }:
{
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
