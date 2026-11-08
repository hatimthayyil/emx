# emx

An Emacs distribution: Nix builds and pins Emacs and its packages, and the Elisp loads live from this repo.

## Opening Emacs

With `services.emacs.enable`, the EMX menu entry opens a window on the Emacs daemon. While no daemon answers, including while one is still starting, the entry starts `emacs.service` and waits for it; only if systemd can't start it does the entry start a plain Emacs. The daemon then owns the server: a plain Emacs, however it starts, runs none, so one started while the daemon starts can't take the server from it. While the daemon is down, as after a failed start or `systemctl --user stop emacs`, no Emacs runs the server: `emacsclient` finds none, emx has no Emacs to reload, and after a failed start each EMX click opens another plain Emacs. Without `services.emacs.enable`, the entry starts a plain Emacs, and the first one running holds the server. The Emacs holding the server is the one emx reloads after a sync, upgrade or rollback. A plain Emacs without the server isn't reloaded.

The home-manager module also installs the fonts emx's configuration names, Iosevka Comfy Motion and Amiri Quran Colored, and turns on fontconfig so Emacs finds them. Set `programs.emx.fonts` to change the list.

## Upgrading packages

```
emx upgrade              # every package, ELPA packages included
emx upgrade gptel magit  # only these git packages
emx rollback             # back to the previous build
```

`emx upgrade` moves the pins in `lock/`, shows what the build changes, and asks before switching. The new pins stay uncommitted: commit them to keep them, or run `git restore lock` to undo them. `emx rollback` moves only the profile, so `lock/` keeps its pins. After either command, a running Emacs reloads into the new build in place. emx says when it needs a restart instead: for a different Emacs, or to drop packages and config that were removed. `M-x emx-reload` does the same reload by hand.

When a package's source can't be reached and Nix has a cached copy, the package keeps its pin and everything else still moves. `emx upgrade` then asks each upstream itself, names every package that isn't at upstream's newest or that it couldn't check, and exits 1, so run it again later. A package it keeps naming may have Nix following another branch than upstream's default, as Nix does when the repository has a branch named `HEAD`; a recipe in `recipes/` that names the branch (`:branch`) fixes it. It asks without your Git config or `.curlrc`, so a proxy set only there, such as Git's `http.proxy`, doesn't apply to these questions; one set in the environment (`https_proxy`, `all_proxy`) does.

Package upgrades and Elisp edits never touch your home-manager or NixOS config. It needs its `emx` input updated only when `cli/`, `Cargo.toml`, `Cargo.lock`, `nix/cli.nix`, `nix/home-module.nix` or `nix/init-files.nix` change, or for a new Emacs major version.

## Licensing

emx is under the Apache License 2.0, except the files that `REUSE.toml` marks GPL-3.0-or-later, which hold text copied from GPL sources. A few lines of `init.el` are under ISC, as its header says. The licence texts are in `LICENSES/`.

The compiled `emx` CLI is GPL-3.0-only, because it links dix.
