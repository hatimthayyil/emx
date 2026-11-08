;;; daemon.el --- Check that emx's daemon sets itself up as Emacs does  -*- lexical-binding: t -*-

;; Copyright (C) 2026  Hatim Thayyil
;; SPDX-License-Identifier: Apache-2.0

;;; Commentary:

;; nix/checks/default.nix's daemon check loads this from the early-init of
;; the daemon it starts, as emacs.service does, after the warning recorder,
;; with $EMX_CHECK_DIR naming a directory it shares with the check.  The
;; daemon must have set itself up as a standalone Emacs does, for the frames
;; emacsclient opens later.

;;; Code:

(defun emx-daemon-check--file (name)
  "Return the file NAME in the directory the check's Emacs processes share."
  (expand-file-name name (getenv "EMX_CHECK_DIR")))

(defun emx-daemon-check-setup ()
  "Write what the daemon didn't set up to the file setup-failures.
Its frames come later, from emacsclient, so what a standalone Emacs sets up
for its first frame must already be in place."
  (let (failures)
    (pcase-dolist (`(,ok . ,what)
                   `((,(bound-and-true-p spacious-padding-mode)
                      . "spacious-padding-mode is off")
                     (,(bound-and-true-p fontaine-mode) . "fontaine-mode is off")
                     (,(eq (bound-and-true-p fontaine-current-preset) 'regular)
                      . "Fontaine's preset isn't regular")
                     (,(eq (key-binding (kbd "C-c f")) 'fontaine-set-preset)
                      . "C-c f doesn't run fontaine-set-preset")
                     (,(eq (key-binding (kbd "<f8>")) 'spacious-padding-mode)
                      . "<f8> doesn't run spacious-padding-mode")
                     (,(eq frame-inhibit-implied-resize t)
                      . "frame-inhibit-implied-resize isn't t")
                     (,inhibit-x-resources . "inhibit-x-resources is nil")
                     (,(eq initial-major-mode 'fundamental-mode)
                      . "initial-major-mode isn't fundamental-mode")))
      (unless ok
        (push what failures)))
    (write-region (mapconcat (lambda (failure) (concat failure "\n"))
                             (append (emx-boot-check-boot-failures)
                                     (nreverse failures)))
                  nil (emx-daemon-check--file "setup-failures"))
    t))

;;; daemon.el ends here
