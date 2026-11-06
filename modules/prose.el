;;; prose.el --- EmX Prose, Writing, Reading  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;;; Commentary:

;;
;; This set of packages provides quality of life improvements for writing or
;; reading prose.

;;; Code:

;; titlecase
;;
;; https://github.com/duckwork/titlecase.el
;;
;; A tool to help capitalise titles.
(use-package titlecase)

;; flycheck
;;
;; https://www.flycheck.org/en/latest/user/flycheck-versus-flymake.html
;;
;; Flycheck is presented as featureful alternative to Emacs
(use-package flycheck
  :hook (text-mode . flycheck-mode-on-safe)
  :config
  ;; org-lint runs inside Emacs on every check, which stalls on large Org files.
  (setopt flycheck-disabled-checkers '(org-lint))
  ;; `vale --output=line' prints FILE:LINE:COLUMN:CHECK:MESSAGE and no severity.
  ;; With no .vale.ini it exits 2, which Flycheck reports as a broken checker.
  (flycheck-define-checker vale
    "Lint prose with vale, configured by the nearest .vale.ini."
    :command ("vale" "--output=line" source)
    :predicate (lambda () (locate-dominating-file default-directory ".vale.ini"))
    :error-patterns
    ((error line-start (file-name) ":" line ":" column ":"
            (id (+ (not ":"))) ":" (message) line-end))
    :modes (text-mode markdown-mode org-mode))
  (add-to-list 'flycheck-checkers 'vale))

;; jinx.el - Spell checker using the encant aggregator library
;;
;; https://github.com/minad/jinx
(use-package jinx
  :hook (emacs-startup . global-jinx-mode)
  :bind (("M-$" . jinx-correct)
         ("C-M-$" . jinx-languages)))

;;; prose.el ends here
