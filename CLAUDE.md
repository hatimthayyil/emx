# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## PROJECT PHILOSOPHY
- Nix builds Emacs and every Elisp package with twist; there is no other package manager
- `packages.el` lists the packages, `recipes/` adds sources missing from the archives, `lock/` pins every version
- Nix stays underneath: it builds and pins, and Elisp work stays live

## BUILD/LINT/TEST COMMANDS
- Build: `nix build`
- Check (boots the real init and loads every package): `nix flake check`
- Throttle local builds: `--max-jobs 2 --cores 4`
- Dev environment: `nix develop`
- Lint Nix files: `nixpkgs-fmt <filename>.nix`

## CODE STYLE GUIDELINES
### Nix
- Use explicit lib references (lib.mkOption, not mkOption)
- Prefer descriptive variable names in camelCase
- Sort attribute sets alphabetically
- Minimize use of with statements

### Emacs Lisp
- Follow Emacs Lisp conventions (2 spaces, kebab-case)
- Add a package to `packages.el`, then regenerate the lock with `nix run --impure .#lock`

### Module Design
- Keep Nix as non-intrusive as possible to Emacs workflow
- Provide sane defaults but allow easy customization
- Document clearly where Nix ends and Elisp begins

## AI ASSISTANT WORKSPACE
- The `.ai-assistant/` directory serves as a dedicated workspace for AI assistance
- `.ai-assistant/decisions/` contains Decision Records (DRs) documenting design choices
- `.ai-assistant/journal/` tracks progress across work sessions
- `.ai-assistant/notes/` stores implementation strategies and project understanding
- `.ai-assistant/TODO.org` keeps track of pending and completed tasks
- This workspace maintains continuity between sessions and documents project evolution
- Reference these files when implementing features to understand context and rationale
