;;; early-init.el --- earliest birds               -*- lexical-binding: t -*-

;; Nix provides every package (packages.el); package.el must not activate any.
(setq package-enable-at-startup nil)
(setq package-archives nil)

;; Defer garbage collection further back in the startup process
(setq gc-cons-threshold most-positive-fixnum)

;; Support direct evaluation with fallbacks if not run through shim. This
;; ensures that the paths are in a clean Emacs env (eg: emacs -Q).
(unless (boundp 'emx-source-dir)
  (defvar emx-source-dir
    (file-name-directory
     (or load-file-name                   ; when loaded from shim
         buffer-file-name                 ; when buffer is evaluated
         (expand-file-name "early-init.el" user-emacs-directory))) ; fallback
    "Directory for EMX source files."))

(unless (boundp 'emx-config-dir)
  (defvar emx-config-dir
    (expand-file-name "~/.config/emx/")
    "Directory for EMX configuration files."))

(unless (boundp 'emx-data-dir)
  (defvar emx-data-dir
    (expand-file-name "~/.local/share/emx/")
    "Directory for EMX data files."))

(unless (boundp 'emx-cache-dir)
  (defvar emx-cache-dir
    (expand-file-name "~/.cache/emx/")
    "Directory for EMX cache files."))

;; Load the EMX early initialization module
(load (expand-file-name "lisp/emx-early-init.el" emx-source-dir))

;;; early-init.el ends here

;; Local Variables:
;; no-byte-compile: t
;; no-native-compile: t
;; no-update-autoloads: t
;; End:
