;;;; emx-programming.el --- EmX programming, software development, languages  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords: languages

;;; Commentary:
;;
;; Software development
;;
;; Emacs is probably the best available editor for writing Lisp programs. Emacs
;; doesn't fair well for multithreading tasks. In this configuration, we setup
;; tools that aid software development, and related tasks such as version
;; control. I do not aim to have Emacs be the only programming IDE that I use, I
;; am exploring Neovim/Helix for languages for which support is lacking in
;; Emacs.

;;; Code:

;;; Workspaces and project management

;; `project'
(use-package project
  :ensure nil
  :bind (:map project-prefix-map
              ("<return>" . project-dired)
              ("<delete>" . project-forget-project))
  :custom
  ;; The menu after `project-switch-project'. `C-g' leaves it.
  (project-switch-commands
   '((project-dired "Dired")
     (project-find-file "File")
     (project-find-dir "Directory")
     (project-find-regexp "Regexp")
     (project-shell "Shell")
     (project-vc-dir "VC")))
  ;; A `.project' file makes its directory a project root.
  (project-vc-extra-root-markers '(".project")))

;; TODO Add tabspaces
;;(use-package tabspaces)

;; lsp-bridge - Fast async LSP client
;;
;; https://github.com/manateelazycat/lsp-bridge
;;
;; Has a number of python deps
(use-package lsp-bridge
  :hook (prog-mode . lsp-bridge-mode)
  :init
  ;; Otherwise enabling the mode turns off backups, auto-save and lockfiles
  ;; for every buffer.
  (setq lsp-bridge-disable-backup nil)
  :config
  (global-lsp-bridge-mode)
  (setq lsp-bridge-python-lsp-server "basedpyright")
  (setq lsp-bridge-enable-hover-diagnostic t)
  (setq lsp-bridge-enable-inlay-hint t)
  (setq lsp-bridge-enable-org-babel t)
  (setq acm-enable-tabnine nil)         ; TabNine binary is not provided; enabling
                                        ; it makes acm try to run a missing program
  (setq acm-candidate-match-function 'regexp-quote) ; default, try
                                                    ; 'orderless-flex later

  )

;;; Lisp programming

;; parinfer - Makes writing Lisp relatively easy
;;
;; https://github.com/justinbarclay/parinfer-rust-mode
;;
;; `parinfer-rust-library' is set in nix/emacs/default.nix. Without a library
;; the mode stops at a download prompt. On a buffer's first command it asks
;; before reindenting the buffer, and it turns itself off in files with tabs.
;;
;; FIXME This package has hung Emacs when loading elisp. Try it in a live
;; session before enabling it.
;; (use-package parinfer-rust-mode
;;   :hook emacs-lisp-mode)

;; paredit
;;
;; This is the de-facto standard Lisp/S-exp editing framework that is often
;; recommended.
;;
;; see 'https://paredit.org/'
;;
;; Recently there has been a plethora of alternatives that has been published.
;; Some of them looks very interesting.
;;
;; see 'https://karthinks.com/software/a-consistent-structural-editing-interface/'
(use-package paredit
  :hook ((emacs-lisp-mode lisp-mode scheme-mode clojure-mode) . paredit-mode)
  ;; M-s stays the prefix for consult's search commands, and RET stays acm's
  ;; key to accept a completion.
  :bind (:map paredit-mode-map
         ("M-s" . nil)
         ("RET" . nil)))

;; geiser
;;
;; Scheme REPL interagration
;;
;; * Alternatives:
;;
;; 1. Andrew Tropin has an alternative based on nREPL (used in Clojure land).
;;    Arei is the Emacs client, and guile-ares-rs is the the Repl Server. This
;;    needs to be run in the background. He considers Geiser to have some
;;    limitations, and chose to implement a new REPL integration. He claims to
;;    have tried to contribute to Geiser but that he found it difficult.
(use-package geiser)

;; macrostep-geiser - In-buffer macro expansion for Geiser
;;
;; This is based on macrostep. It currently is in the emacsorphanage
(use-package macrostep-geiser
  :hook (geiser-mode . macrostep-geiser-setup))

;; Common Lisp
;;
;; https://github.com/joaotavora/sly
(use-package sly)

;; Nix mode
;;
;; https://github.com/NixOS/nix-mode
(use-package nix-mode
  :mode "\\.nix\\'")

(provide 'emx-programming)
;;; emx-programming.el ends here
