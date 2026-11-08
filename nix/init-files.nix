# The init files of an Emacs init directory that runs emx: early-init.el and
# init.el load emx's own from the emx repo. The home-manager module writes them
# to ~/.config/emx, and the boot and reload checks boot from them.
{ lib }:
rec {
  # S as an Elisp string literal.
  elispString = s: "\"" + lib.replaceStrings [ "\\" "\"" ] [ "\\\\" "\\\"" ] s + "\"";

  # Maps each file's name to its text, for the emx repo at SRC. PRELUDE is
  # Elisp that early-init.el runs first, each form on a line of its own.
  initFiles = { src, prelude ? "" }: {
    "early-init.el" = ''
      ;;; early-init.el --- Load emx's early-init.el from the emx repo  -*- lexical-binding: t -*-
      ${prelude}(defvar emx-source-dir (file-name-as-directory ${elispString src})
        "The emx repo that Emacs loads its configuration from.")
      (load (expand-file-name "early-init.el" emx-source-dir) nil t)
    '';
    "init.el" = ''
      ;;; init.el --- Load emx's init.el from the emx repo  -*- lexical-binding: t -*-
      (load (expand-file-name "init.el" emx-source-dir) nil t)
    '';
  };
}
