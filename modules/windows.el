;;; windows.el --- General window and buffer configurations  -*- lexical-binding: t; -*-

;; Copyright (C) 2025-2026  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Adapted for emx in 2025-2026 from Prot's Emacs configuration,
;; GPL-3.0-or-later: https://github.com/protesilaos/dotfiles
;; Copyright (C) 2020-2025  Protesilaos Stavrou

(use-package uniquify
  :ensure nil
  :config
;;;; `uniquify' (unique names for buffers)
  (setq uniquify-buffer-name-style 'forward)
  (setq uniquify-strip-common-suffix t)
  (setq uniquify-after-kill-buffer-p t))

;;;; Line highlight
(use-package hl-line
  :ensure nil
  :commands (hl-line-mode)
  :config
  (setq hl-line-sticky-flag nil)
  (setq hl-line-overlay-priority -50)) ; emacs28

;;; Frame history (undelete-frame-mode)
(use-package frame
  :ensure nil
  :bind ("C-x u" . undelete-frame) ; I use only C-/ for `undo'
  :hook (after-init . undelete-frame-mode))

;;; Window history (winner-mode)
(use-package winner
  :ensure nil
  :hook (after-init . winner-mode)
  :bind
  (("C-x <right>" . winner-redo)
   ("C-x <left>" . winner-undo)))
