;;;; emx-programming.el --- EmX programming, software development, languages  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords: languages

;; The `project' settings are adapted from Prot's Emacs configuration,
;; GPL-3.0-or-later: https://github.com/protesilaos/dotfiles
;; Copyright (C) 2020-2025  Protesilaos Stavrou

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
  :bind
  (("C-x p ." . project-dired)
   ("C-x p C-g" . keyboard-quit)
   ("C-x p <return>" . project-dired)
   ("C-x p <delete>" . project-forget-project))
  :config
  (setopt project-switch-commands
          '((project-find-file "Find file")
            (project-find-regexp "Find regexp")
            (project-find-dir "Find directory")
            (project-dired "Root dired")
            (project-vc-dir "VC-Dir")
            (project-shell "Shell")
            (keyboard-quit "Quit")))
  (setq project-vc-extra-root-markers '(".project")) ; Emacs 29
  (setq project-key-prompt-style t)) ; Emacs 30

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
;; FIXME This package hangs Emacs when loading elisp. The parinfer rust
;; library needs to be sorted out. The lib is not being detected atm.
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
(use-package paredit)

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
  :after geiser-mode
  :config (add-hook 'geiser-mode-hook #'macrostep-geiser-setup))

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
