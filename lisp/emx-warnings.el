;;; emx-warnings.el --- Tell the warnings Emacs logs from those it drops  -*- lexical-binding: t -*-

;; Copyright (C) 2026  Hatim Thayyil
;; SPDX-License-Identifier: Apache-2.0

;;; Commentary:

;; Advice on `display-warning' sees every warning, including those Emacs
;; drops without logging: below `warning-minimum-log-level', or of a type
;; `warning-inhibit-types' or `warning-suppress-log-types' names.
;; `emx-warnings-logged-level' says which level a warning is logged at, if
;; Emacs logs it at all, for code that records warnings, such as the boot
;; check's warning recorder.

;;; Code:

(require 'warnings)

(defun emx-warnings-logged-level (type level)
  "Return the level `display-warning' logs a warning of TYPE at LEVEL at.
LEVEL is what `display-warning' takes: nil means `:warning', and an obsolete
alias in `warning-level-aliases' means the level it names.  Return that level,
a key of `warning-levels', or nil when Emacs doesn't log the warning."
  (let ((level (or level :warning)))
    (with-suppressed-warnings ((obsolete warning-level-aliases))
      (setq level (or (cdr (assq level warning-level-aliases)) level)))
    (unless (ignore-errors
              (or (< (warning-numeric-level level)
                     (warning-numeric-level warning-minimum-log-level))
                  (warning-suppress-p type warning-inhibit-types)
                  (warning-suppress-p type warning-suppress-log-types)))
      level)))

(provide 'emx-warnings)

;;; emx-warnings.el ends here
