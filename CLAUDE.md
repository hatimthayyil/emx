# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## PROJECT PHILOSOPHY
- Nix builds Emacs and every Elisp package with twist; there is no other package manager
- `packages.el` lists the packages, `recipes/` adds sources missing from the archives, `lock/` pins every version
- Nix stays underneath: it builds and pins, and Elisp work stays live
- `emx sync` (the Rust CLI in `cli/`) builds Emacs and its packages into emx's own Nix profile, `$XDG_STATE_HOME/emx/profile`; home-manager installs emx once and never builds the package set
- The tools emx's modules call are `executablePackages` in `nix/emacs/default.nix`: they sit in emx's profile, on the `PATH` of Emacs and what it starts, and nothing outside Emacs sees them
- Emacs loads its Elisp from the emx repo (`programs.emx.src`, `EMX_SRC` for the CLI), so edits need no build
- The config repo's `emx` input needs bumping only for changes to `cli/`, `Cargo.toml`, `Cargo.lock`, `nix/cli.nix`, `nix/home-module.nix` or `nix/init-files.nix`, or for a new Emacs major version. Elisp edits and package upgrades never touch it

## BUILD/LINT/TEST COMMANDS
- Dev environment: `devenv shell` (Rust toolchain, treefmt, commit hooks)
- CLI tests: `cargo test` (or `devenv test`); `cargo test -- --ignored` also builds emx's Emacs with Nix into a temporary profile and boots it (extra `nix build` arguments in `EMX_TEST_NIX_ARGS`)
- Build Emacs into emx's profile: `emx sync` (extra `nix build` arguments after `--`). It shows what changes and asks before switching; without a terminal, such as from an agent, pass `--yes`. The sync that creates the profile starts `emacs.service` if the session starts it (`services.emacs.startWithUserSession`, passed as `EMX_DAEMON_STARTS_WITH_SESSION` by `nix/home-module.nix`, which names the unit in `EMX_DAEMON_UNIT` whenever `services.emacs` runs the daemon), and otherwise says why it didn't; a failed start keeps the switch and exits 1; later syncs reload a running Emacs into the new generation with `emx-reload` (`lisp/emx-reload.el`), and say when it needs a restart. A switch whose reload fails stays switched and exits 1
- Upgrade packages: `emx upgrade [PACKAGE...]` moves pins in `lock/` to upstream's newest, then syncs (`--yes` and extra Nix arguments after `--`, as for `emx sync`). With names it moves those git packages; with none it moves every package. ELPA packages move only together, so naming one is refused. A source Nix can't reach keeps its pin and Nix still exits 0, so emx then asks upstream itself (`git ls-remote` for git packages, each archive's `archive-contents` for ELPA, `cli/src/upstream.rs`): the rest still move and sync, then emx names each package that isn't at upstream's newest, or that it couldn't check, and exits 1. The new pins stay uncommitted: commit them, or undo them with `git restore lock`. A running Emacs is reloaded as after `emx sync`
- Roll back: `emx rollback` returns emx's profile to its previous generation and reloads a running Emacs into it; `lock/` stays, so the next sync builds its pins again
- Reload by hand: `M-x emx-reload` loads the profile's current generation and the config again. A restart is still needed for a different Emacs, a generation without `share/emx/manifest.json`, or to drop what was removed
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
- `modules/<concern>.el` is startup config: `init.el` loads it by path from `emx-modules`. It is not on `load-path` and has no `provide`, and its name must not be an Emacs library's, because loading it would fire that library's string-keyed `eval-after-load` forms
- `lisp/emx-<name>.el` holds emx's libraries, whose names share Emacs's global namespace

### Rust
- The CLI drives Nix through its command line (`cli/src/nix.rs`)
- Tests never touch the real state directory, daemon or Emacs server: point `XDG_STATE_HOME` and `XDG_RUNTIME_DIR` at temporary directories, unset `EMACS_SOCKET_NAME`, put a fake `systemctl` on `PATH`, and give the fake build a fake `bin/emacsclient`, since the CLI runs the `emacsclient` of the generation it switched to

### Module Design
- Keep Nix as non-intrusive as possible to Emacs workflow
- Provide sane defaults but allow easy customization
- Document clearly where Nix ends and Elisp begins

## LICENSING
- `REUSE.toml` gives every file its licence, the texts are in `LICENSES/`, and `nix flake check` runs `reuse lint`
- Everything is Apache-2.0, except the files `REUSE.toml` lists as GPL-3.0-or-later, which hold text copied from GPL sources. Root `LICENSE` is an unmodified copy of `LICENSES/Apache-2.0.txt`, for GitHub. The compiled `emx` is GPL-3.0-only because it links dix
- Copied GPL text, and text with no stated licence, can't be relabelled, only replaced: write the replacement from what the code does, not by rewording the copy. A file leaves the GPL list once its copied text is gone
- Elisp copied from elsewhere gets a notice in the file's header: its source, the source's copyright line and its licence. A file marked adapted also says when
- A new file outside the listed paths needs an entry in `REUSE.toml`
