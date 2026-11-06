;;; defaults.el --- EmX Defaults  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@owl>
;; Keywords:

;;; Commentary:

;; Essential packages that improve the default funcionality of Emacs.

;;; Code:

;;; ws-butler - Cleanup whitespace at end of lines
(use-package ws-butler
  :delight
  :hook (after-init . ws-butler-global-mode))

(use-package transient)

;;; defaults.el ends here
