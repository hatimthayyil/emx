# emx

An Emacs distribution: Nix builds and pins Emacs and its packages, and the Elisp loads live from this repo.

## Upgrading packages

```
emx upgrade              # every package, ELPA packages included
emx upgrade gptel magit  # only these git packages
emx rollback             # back to the previous build
```

`emx upgrade` moves the pins in `lock/`, shows what the build changes, and asks before switching. The new pins stay uncommitted: commit them to keep them, or run `git restore lock` to undo them. `emx rollback` moves only the profile, so `lock/` keeps its pins. After either command, `systemctl --user restart emacs` loads the new build.

When a package's source can't be reached and Nix has a cached copy, the package keeps its pin and everything else still moves. `emx upgrade` then names it and exits 1, so run it again later.

Package upgrades and Elisp edits never touch your home-manager or NixOS config. It needs its `emx` input updated only when `cli/`, `Cargo.toml`, `Cargo.lock`, `nix/cli.nix` or `nix/home-module.nix` change, or for a new Emacs major version.

## Licensing

emx is under the Apache License 2.0, except the files that `REUSE.toml` marks GPL-3.0-or-later, which hold text copied from GPL sources. A few lines of `init.el` are under ISC, as its header says. The licence texts are in `LICENSES/`.

The compiled `emx` CLI is GPL-3.0-only, because it links dix.
