;;; record-warnings.el --- Record the warnings Emacs logs  -*- lexical-binding: t -*-

;; Copyright (C) 2026  Hatim Thayyil
;; SPDX-License-Identifier: Apache-2.0

;;; Commentary:

;; The boot check's early-init loads this first, after lisp/emx-warnings.el,
;; so it sees every warning from then on: site-start's, init's (a module that
;; fails to load is one), and those packages give as they load.  `warn' and
;; `lwarn' go through `display-warning' too.

;;; Code:

(require 'emx-warnings)

(defvar emx-boot-check-warnings nil
  "The warnings `display-warning' logged, newest first.
Each element is (TYPE LEVEL MESSAGE), with LEVEL a key of `warning-levels'.")

(defun emx-boot-check-warnings--record (type message &optional level _buffer-name)
  "Record the warning TYPE, MESSAGE and LEVEL, unless Emacs would not log it."
  (when-let* ((level (emx-warnings-logged-level type level)))
    (push (list type level message) emx-boot-check-warnings)))

(advice-add 'display-warning :before #'emx-boot-check-warnings--record)

(defun emx-boot-check-boot-failures ()
  "Return what went wrong as Emacs booted, as a list of failures.
An error from the init files is one, and so is each warning logged at
`:error' or `:emergency': init reports a module that fails to load this way,
and `use-package' an error in a `:config' block."
  (append
   (and init-file-had-error
        (list "init: loading the init files signalled an error"))
   (mapcan (pcase-lambda (`(,type ,level ,message))
             (when (memq level '(:error :emergency))
               (list (format "%s (%s): %s"
                             (capitalize (substring (symbol-name level) 1))
                             (if (consp type) (car type) type)
                             message))))
           (reverse emx-boot-check-warnings))))

;;; record-warnings.el ends here
