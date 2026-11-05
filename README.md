# emx

An Emacs distribution: Nix builds and pins Emacs and its packages, and the Elisp loads live from this repo.

## Licensing

emx is under the Apache License 2.0, except the files that `REUSE.toml` marks GPL-3.0-or-later, which hold text copied from GPL sources. A few lines of `init.el` are under ISC, as its header says. The licence texts are in `LICENSES/`.

The compiled `emx` CLI is GPL-3.0-only, because it links dix.
