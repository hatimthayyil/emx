;;; footer.el --- Configuration to be run towards the end of the init  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;; envrc.el - buffer-local direnv integration for Emacs
(use-package envrc
  :hook (after-init . envrc-global-mode))
