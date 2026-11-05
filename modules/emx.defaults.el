;;; emx-defaults.el --- EmX Defaults                 -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@owl>
;; Keywords:

;;; Commentary:

;; Essential packages that improve the default funcionality of Emacs.

;;; Code:

;;; ws-butler - Cleanup whitespace at end of lines
(use-package ws-butler
  :delight
  :config (ws-butler-global-mode t))

(use-package transient)

(provide 'emx-defaults)
;;; emx-defaults.el ends here
