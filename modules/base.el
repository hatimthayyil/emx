;;; base.el --- Base Emacs defaults  -*- lexical-binding: t -*-
;;;
;;; Some sane defaults for a base Emacs installation.
;;;
;;; https://protesilaos.com/codelog/2024-11-28-basic-emacs-configuration/

;; Copyright (C) 2025-2026  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Adapted for emx in 2025-2026 from Prot's Emacs configuration,
;; GPL-3.0-or-later: https://github.com/protesilaos/dotfiles
;; Copyright (C) 2020-2025  Protesilaos Stavrou

(require 'use-package)

;;; Editing

;; Delete the selected/highlighted text as soon as the user types something.
(use-package delsel
  :ensure nil				; built-in
  :hook (after-init . delete-selection-mode))

;; Show the dict definition of word at point.
(use-package dictionary
  :ensure nil 				; built-in
  :config
  ;; Show dictionary definition on the left
  (add-to-list 'display-buffer-alist
               '("^\\*Dictionary\\*"
		 (display-buffer-in-side-window)
		 (side . left)
		 (window-width . 70))))

;;; Helper functions

(defun emx/keyboard-quit-dwim ()
  "Do-What-I-Mean behaviour for a general `keyboard-quit'.

The generic `keyboard-quit' does not do the expected thing when
the minibuffer is open.  Whereas we want it to close the
minibuffer, even without explicitly focusing it.

The DWIM behaviour of this command is as follows:

- When the region is active, disable it.
- When a minibuffer is open, but not focused, close the minibuffer.
- When the Completions buffer is selected, close it.
- In every other case use the regular `keyboard-quit'."
  (interactive)
  (cond
   ((region-active-p)
    (keyboard-quit))
   ((derived-mode-p 'completion-list-mode)
    (delete-completion-window))
   ((> (minibuffer-depth) 0)
    (abort-recursive-edit))
   (t
    (keyboard-quit))))

(bind-key "C-g" #'emx/keyboard-quit-dwim)



;; Local Variables:
;; indent-tabs-mode: nil
;; End:
;;; base.el ends here
