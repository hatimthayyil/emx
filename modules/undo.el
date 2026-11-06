;;; undo.el --- Undo history  -*- lexical-binding: t; -*-

;; vundo - Visual undo tree
;;
;; 'https://github.com/casouri/vundo'
;;
;; This is an alternative to the much older 'undo-tree' package. Vundo displays
;; a horizontal tree whereas undo-tree displays a vertical tree.
(use-package vundo
  :config
  (setq vundo-glyph-alist vundo-unicode-symbols))
