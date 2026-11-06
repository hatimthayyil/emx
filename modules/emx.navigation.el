;;; emx-navigation.el --- Navigation helpers       -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@thayyil.net>

;;; Code

;; beginend - Move to meaningful locations in buffer
;;
;; https://github.com/DamienCassou/beginend
;;
;; Remap M-< and M-> to move to more meaningful locations, such as the beginning
;; of sourcecode.
(use-package beginend
  :hook (after-init . beginend-global-mode))

;; mwim - Move Where I Mean
;;
;; https://github.com/alezost/mwim.el
;;
;; This package allows for the cursor to be moved to code beginning or end, in a
;; line.
(use-package mwim
  :bind (("C-a" . mwim-beginning-of-code-or-line)
         ("C-e" . mwim-end-of-code-or-line)))

(provide 'emx-navigation)
