;;; finance.el --- Financial tools  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@owl>

;;; Commentary:

;; Modes for plain-text accounting

;; beancount-mode
;;
;; https://github.com/beancount/beancount-mode
(use-package beancount
  :config
  (add-to-list 'auto-mode-alist '("\\.beancount\\'" . beancount-mode)))
