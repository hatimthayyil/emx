;;; file-manager.el --- Filesystem navigation and manipulation  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;; Adapted for emx in 2025-2026 from the following, both GPL-3.0-or-later:
;; - Prot's Emacs configuration, https://github.com/protesilaos/dotfiles
;;   Copyright (C) 2020-2025  Protesilaos Stavrou
;; - The Dirvish documentation, https://github.com/alexluigit/dirvish
;;   Copyright (C) 2021-2025 Alex Lu

;;; Code:

(use-package dired
  :ensure nil
  :commands (dired)
  :config
  (setq dired-recursive-copies 'always)
  (setq dired-recursive-deletes 'always)
  (setq delete-by-moving-to-trash t)
  ;; (setq dired-listing-switches
  ;;       "-AGFhlv --group-directories-first --time-style=long-iso")
  (setq dired-listing-switches
        "--almost-all --human-readable --group-directories-first --no-group")
  (put 'dired-find-alternate-file 'disabled nil)

  (setq dired-dwim-target t)

  (setq dired-auto-revert-buffer #'dired-directory-changed-p) ; also see `dired-do-revert-buffer'
  (setq dired-make-directory-clickable t) ; Emacs 29.1
  (setq dired-free-space nil) ; Emacs 29.1
  (setq dired-mouse-drag-files t) ; Emacs 29.1

  (add-hook 'dired-mode-hook #'dired-hide-details-mode)
  (add-hook 'dired-mode-hook #'hl-line-mode))

(use-package dired-aux
  :ensure nil
  :after dired
  :bind
  ( :map dired-mode-map
    ("C-+" . dired-create-empty-file)
    ("M-s f" . nil)
    ("C-<return>" . dired-do-open) ; Emacs 30
    ("C-x v v" . dired-vc-next-action)) ; Emacs 28
  :config
  (setq dired-isearch-filenames 'dwim)
  (setq dired-create-destination-dirs 'ask) ; Emacs 27
  (setq dired-vc-rename-file t)             ; Emacs 27
  (setq dired-do-revert-buffer (lambda (dir) (not (file-remote-p dir)))) ; Emacs 28
  (setq dired-create-destination-dirs-on-trailing-dirsep t)) ; Emacs 29

(use-package dired-x
  :ensure nil
  :after dired
  :bind
  ( :map dired-mode-map
    ("I" . dired-info))
  :config
  (setq dired-clean-up-buffers-too t)
  (setq dired-clean-confirm-killing-deleted-buffers t)
  (setq dired-x-hands-off-my-keys t)    ; easier to show the keys I use
  (setq dired-bind-man nil)
  (setq dired-bind-info nil))

(use-package dirvish
  :custom
  (dirvish-quick-access-entries
   '(("h" "~/"                        "Home")
     ("d" "~/downloads/"              "Downloads")
     ("s" "~/src/"                    "Source Files")))
  :config
  (setq dirvish-mode-line-format
        '(:left (sort symlink) :right (omit yank index)))
  (setq dirvish-attributes
        '(vc-state subtree-state collapse git-msg file-time file-size)
        dirvish-side-attributes
        '(vc-state collapse file-size))
  :bind
  (("C-c d" . dirvish)
   :map dirvish-mode-map
   ("?" . dirvish-dispatch)
   ("f" . dirvish-history-go-forward)
   ("b" . dirvish-history-go-backward)
   ("y" . dirvish-yank-menu)
   ("N" . dirvish-narrow)
   ("^" . dirvish-history-last)
   ("s" . dirvish-setup-menu)
   ("h" . dirvish-history-jump)
   ("r" . dirvish-quicksort)
   ("v" . dirvish-vc-menu)
   ("TAB" . dirvish-subtree-toggle)
   ("M-a" . dirvish-quick-access)
   ("M-f" . dirvish-file-info-menu)
   ("M-l" . dirvish-ls-switches-menu)
   ("M-m" . dirvish-mark-menu)
   ("M-t" . dirvish-layout-toggle)
   ("M-e" . dirvish-emerge-menu)
   ("M-j" . dirvish-fd)))
