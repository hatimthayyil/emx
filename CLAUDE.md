# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## PROJECT PHILOSOPHY
- Nix builds Emacs and every Elisp package with twist; there is no other package manager
- `packages.el` lists the packages, `recipes/` adds sources missing from the archives, `lock/` pins every version
- Nix stays underneath: it builds and pins, and Elisp work stays live
- `emx sync` (the Rust CLI in `cli/`) builds Emacs and its packages into emx's own Nix profile, `$XDG_STATE_HOME/emx/profile`; home-manager installs emx once and never builds the package set
- Emacs loads its Elisp from the emx repo (`programs.emx.src`, `EMX_SRC` for the CLI), so edits need no build

## BUILD/LINT/TEST COMMANDS
- Dev environment: `devenv shell` (Rust toolchain, treefmt, commit hooks)
- CLI tests: `cargo test` (or `devenv test`); `cargo test -- --ignored` also builds emx's Emacs with Nix into a temporary profile and boots it (extra `nix build` arguments in `EMX_TEST_NIX_ARGS`)
- Build Emacs into emx's profile: `emx sync` (extra `nix build` arguments after `--`). It shows what changes and asks before switching; without a terminal, such as from an agent, pass `--yes`
- Build: `nix build .#emacs`, `nix build .#emx`
- Check (boots the real init and loads every package, checks the home-manager module): `nix flake check`
- Throttle local builds: `--max-jobs 2 --cores 4`
- Format: `treefmt` (nixpkgs-fmt, rustfmt)

## CODE STYLE GUIDELINES
### Nix
- Use explicit lib references (lib.mkOption, not mkOption)
- Prefer descriptive variable names in camelCase
- Sort attribute sets alphabetically
- Minimize use of with statements

### Emacs Lisp
- Follow Emacs Lisp conventions (2 spaces, kebab-case)
- Add a package to `packages.el`, then regenerate the lock with `nix run --impure .#lock`

### Rust
- The CLI drives Nix through its command line (`cli/src/nix.rs`)
- Tests never touch the real state directory: point `XDG_STATE_HOME` at a temporary directory

### Module Design
- Keep Nix as non-intrusive as possible to Emacs workflow
- Provide sane defaults but allow easy customization
- Document clearly where Nix ends and Elisp begins

## LICENSING
- `REUSE.toml` gives every file its licence, the texts are in `LICENSES/`, and `nix flake check` runs `reuse lint`
- Elisp that Emacs loads (`init.el`, `early-init.el`, `lisp/`, `modules/`) is GPL-3.0-or-later
- Everything else is CC0-1.0. The compiled `emx` is GPL-3.0-only because it links dix
- Elisp copied from elsewhere gets a notice in the file's header: its source, the source's copyright line and its licence. A file marked adapted also says when
- A new file outside the listed paths needs an entry in `REUSE.toml`
