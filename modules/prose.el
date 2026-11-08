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
(defun emx-vale-errors (output checker buffer)
  "Return the Flycheck errors in OUTPUT, the report of `vale --output=JSON'.
CHECKER and BUFFER are the check's.  The report maps each file to its alerts.
OUTPUT also holds vale's stderr, so each line outside the report, such as
vale's warning about an unknown .vale.ini key, becomes a warning on line 1.
Output with no report, or the error object vale prints when it fails, signals
an error, so that Flycheck marks the check failed rather than finding nothing."
  (let ((report nil)
        (rest output)
        (errors '()))
    (when-let* ((start (string-match "^{" output)))
      (with-temp-buffer
        (insert output)
        (goto-char (1+ start))
        (condition-case nil
            (setq report (json-parse-buffer)
                  rest (concat (substring output 0 start)
                               (buffer-substring (point) (point-max))))
          (json-error nil))))
    (unless (hash-table-p report)
      (error "vale printed no JSON report: %s" (string-trim output)))
    (dolist (line (split-string rest "\n" t "[ \t\r]+"))
      (push (flycheck-error-new-at 1 nil 'warning line
                                   :checker checker :buffer buffer
                                   :filename (buffer-file-name buffer))
            errors))
    (maphash
     (lambda (file alerts)
       (unless (vectorp alerts)
         (let ((text (gethash "Text" report)))
           (error "vale: %s" (string-trim (if (stringp text) text output)))))
       (seq-doseq (alert alerts)
         ;; Span is the alert's first and last column, both included.
         (let ((span (gethash "Span" alert)))
           (push (flycheck-error-new-at
                  (gethash "Line" alert) (aref span 0)
                  (pcase (gethash "Severity" alert)
                    ("suggestion" 'info)
                    ("warning" 'warning)
                    (_ 'error))
                  (gethash "Message" alert)
                  :end-column (1+ (aref span 1))
                  :id (gethash "Check" alert)
                  :checker checker :buffer buffer :filename file)
                 errors))))
     report)
    (nreverse errors)))

(use-package flycheck
  :hook (text-mode . flycheck-mode-on-safe)
  :config
  ;; org-lint runs inside Emacs on every check, which stalls on large Org files.
  (setopt flycheck-disabled-checkers '(org-lint))
  ;; With no .vale.ini vale exits 2, which Flycheck reports as a broken checker.
  (flycheck-define-checker vale
    "Lint prose with vale, configured by the nearest .vale.ini."
    :command ("vale" "--output=JSON" source)
    :predicate (lambda () (locate-dominating-file default-directory ".vale.ini"))
    :error-parser emx-vale-errors
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
