;;; emx-prose.el --- EmX Prose, Writing, Reading     -*- lexical-binding: t; -*-

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
  :config
  ;; `vale --output=line' prints FILE:LINE:COLUMN:CHECK:MESSAGE and no severity.
  (flycheck-define-checker vale
    "Lint prose with vale, configured by the nearest .vale.ini."
    :command ("vale" "--output=line" source)
    :error-patterns
    ((error line-start (file-name) ":" line ":" column ":"
            (id (+ (not ":"))) ":" (message) line-end))
    :modes (text-mode markdown-mode org-mode))
  (add-to-list 'flycheck-checkers 'vale t))

;; jinx.el - Spell checker using the encant aggregator library
;;
;; https://github.com/minad/jinx
(use-package jinx
  :hook (emacs-startup . global-jinx-mode)
  :bind (("M-$" . jinx-correct)
         ("C-M-$" . jinx-languages)))
 
(provide 'emx-prose)
;;; emx-prose.el ends here
