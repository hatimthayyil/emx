;;; emx-version-control.el --- EmX version-control, magit, diff  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords: vc

;; Adapted for emx in 2025-2026 from the following, both GPL-3.0-or-later:
;; - Prot's Emacs configuration, https://github.com/protesilaos/dotfiles
;;   Copyright (C) 2020-2025  Protesilaos Stavrou
;; - lem-setup-vc.el in Lambda Emacs by Colin McLear
;;   https://github.com/Lambda-Emacs/lambda-emacs

;;; Commentary:

;;

;;; Code:

;;; Patches/diffs

;; ediff - visual interface to diff and patch
;;
;; Built-in Emacs package.
;;
;; Alternatives: vdiff (similar to vimdiff for Vim).
;;
;; Imported from Prot's config
(use-package ediff
  :ensure nil
  :commands (ediff-buffers ediff-files ediff-buffers3 ediff-files3)
  :init
  (setq ediff-split-window-function 'split-window-horizontally)
  (setq ediff-window-setup-function 'ediff-setup-windows-plain)
  :config
  (setq ediff-keep-variants nil)
  (setq ediff-make-buffers-readonly-at-startup nil)
  (setq ediff-merge-revisions-with-ancestor t)
  (setq ediff-show-clashes-only t))

;; diff-mode
;;
;; Config imported from prot.
;;
;; TODO Review this config
(use-package diff-mode
  :ensure nil
  :defer t
  :config
  (setq diff-default-read-only t)
  (setq diff-advance-after-apply-hunk t)
  (setq diff-update-on-the-fly t)
  ;; The following are from Emacs 27.1
  (setq diff-refine nil) ; I do it on demand, with my `agitate' package (more below)
  (setq diff-font-lock-prettify t) ; I think nil is better for patches, but let me try this for a while
  (setq diff-font-lock-syntax 'hunk-also))

;; diff-hl - Highlight uncommitted changes in the buffer margins (gutter)
;;
;; https://github.com/dgutov/diff-hl
;;
;; This package allows for the reference to be set to any revision.
;;
;; Alternatives: git-gutter (allows for hunks to be staged)
(use-package diff-hl
  :hook
  ((prog-mode . diff-hl-mode)
   (text-mode . diff-hl-mode)
   (dired-mode . diff-hl-dired-mode)
   (magit-pre-refresh . diff-hl-magit-pre-refresh)
   (magit-post-refresh . diff-hl-magit-post-refresh))
  :custom
  (diff-hl-side 'left)
  (diff-hl-margin-symbols-alist
   '((insert . "┃")
     (delete . "┃")
     (change . "┃")
     (unknown . "?")
     (ignored . "i"))))

;;; Version control tools

;; vc - Version control for multiple backends
;;
;; Builtin to Emacs.
;;
;; NOTE Study this package. It seems versatile, and can handle multiple types of
;; version-control systems.
(use-package vc
  :ensure nil
  :hook (emacs-startup . vc-mode)
  :custom
  (vc-follow-symlinks t)
  (vc-log-short-style '(file)))

;; vc-git - VC backend for Git
(use-package vc-git
  :ensure nil
  :after vc
  :custom
  (vc-git-diff-switches "--patch-with-stat")) ; display additional statistics

;; magit - Interactive and powerful git front-end
;;
;; https://magit.vc/ https://github.com/magit/magit
;;
;; A featureful git porcelain that is well-regarded in the Emacs community.
;;
;; TODO Review config as it is imported from Prot for now.
(use-package magit
  :bind ("C-c g" . magit-status)
  :init
  (setq magit-define-global-key-bindings nil)
  (setq magit-section-visibility-indicator '("⮧"))
  :config
  (setq git-commit-summary-max-length 50)
  ;; NOTE 2023-01-24: I used to also include `overlong-summary-line'
  ;; in this list, but I realised I do not need it.  My summaries are
  ;; always in check.  When I exceed the limit, it is for a good
  ;; reason.
  (setq git-commit-style-convention-checks '(non-empty-second-line))

  (setq magit-diff-refine-hunk t))

(use-package magit-repos
  :ensure nil ; part of `magit'
  :after magit
  :commands (magit-list-repositories)
  :init
  (setq magit-repository-directories
        '(("~/src" . 1))))

;; transient - keyboard driven menus
;;
;; We add a delay to the popup showing up.
(use-package transient
  :defer t
  :config
  (setq transient-show-popup 0.5))

;; magit-todo - Show source code TODO in Magit
;;
;; https://github.com/alphapapa/magit-todos
;;
;; deps: ripgrep
(use-package magit-todos
  :after magit
  :config (magit-todos-mode 1))

;; git-modes -
(use-package git-modes)


;;;; EmX commands

;; Quick commits for the current buffer (credit: Lambda Emacs)
(defun ee-quick-commit ()
  "Quickly commit the current file-visiting buffer from the mini-buffer."
  (interactive)
  (shell-command
   (concat "git add " (buffer-file-name) "&& "
           "git commit -m '" (read-string "Enter commit message: ") "'")))

(provide 'emx-version-control)
;;; emx-version-control.el ends here
