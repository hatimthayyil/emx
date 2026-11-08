;;; daemon.el --- Check emx's daemon: its server and its setup  -*- lexical-binding: t -*-

;; Copyright (C) 2026  Hatim Thayyil
;; SPDX-License-Identifier: Apache-2.0

;;; Commentary:

;; nix/checks/default.nix's daemon check loads this from the early-init of
;; every Emacs it starts, after the warning recorder, with $EMX_CHECK_DIR
;; naming a directory they share.  It starts a daemon as emacs.service does,
;; whose init waits until a plain Emacs, started meanwhile with the init files
;; the home-manager module writes when services.emacs runs the daemon, has
;; booted.  That Emacs must leave the server to the daemon, which then starts
;; it.  The daemon must have set itself up as a standalone Emacs does, for
;; the frames emacsclient opens later.  A plain Emacs started beside the
;; running daemon, with no daemon owning the server, must start no server and
;; give no warning.  A second daemon, booted once the first has gone and with
;; init files that don't mark a daemon as the server's owner, must leave the
;; server to Emacs, which starts a daemon's server after init: its init must
;; make no `server-start' call.

;;; Code:

(defun emx-daemon-check--file (name)
  "Return the file NAME in the directory the check's Emacs processes share."
  (expand-file-name name (getenv "EMX_CHECK_DIR")))

(defun emx-daemon-check--wait-for (name)
  "Wait until the file NAME exists, for at most two minutes.
Return non-nil if it does."
  (let ((deadline (+ (float-time) 120)))
    (while (and (not (file-exists-p (emx-daemon-check--file name)))
                (< (float-time) deadline))
      (sleep-for 0.1))
    (file-exists-p (emx-daemon-check--file name))))

(defvar emx-daemon-check--init-server-starts 0
  "How many times `server-start' was called before init finished.")

(advice-add 'server-start :before
            (lambda (&rest _)
              (unless after-init-time
                (setq emx-daemon-check--init-server-starts
                      (1+ emx-daemon-check--init-server-starts)))))

;; The daemon's init goes on only once the plain Emacs has booted, so that
;; Emacs boots while the daemon initialises.
(when (and (daemonp) (getenv "EMX_CHECK_HOLD_DAEMON"))
  (emx-daemon-check--wait-for "plain-booted"))

(defun emx-daemon-check--server-failures ()
  "Return why this Emacs's boot didn't leave the server to the daemon."
  (append
   (and server-process (list "it started a server"))
   (mapcan (pcase-lambda (`(,type ,_level ,message))
             (when (eq (if (consp type) (car type) type) 'server)
               (list (format "it warned: %s" message))))
           (reverse emx-boot-check-warnings))
   (emx-boot-check-boot-failures)))

(defun emx-daemon-check--exit (what failures)
  "Print each of FAILURES of WHAT, then exit, with 1 if there are any."
  (dolist (failure failures)
    (message "%s: %s" what failure))
  (kill-emacs (if failures 1 0)))

(defun emx-daemon-check-plain-during-init ()
  "Check a plain Emacs that booted while the daemon initialised.
It must have started no server, so the daemon can start its own.  It lets
the daemon's init go on, then stays up until the daemon has been checked, as
an Emacs left open would."
  (let ((failures (emx-daemon-check--server-failures)))
    (write-region "" nil (emx-daemon-check--file "plain-booted"))
    (unless (emx-daemon-check--wait-for "daemon-checked")
      (push "the daemon wasn't checked within two minutes" failures))
    (emx-daemon-check--exit
     "A plain Emacs started while the daemon initialised" failures)))

(defun emx-daemon-check-own-server ()
  "Write why this daemon's init didn't leave the server to Emacs.
The reasons go to the file own-server-failures, one per line."
  (write-region (mapconcat (lambda (failure) (concat failure "\n"))
                           (append
                            (and (> emx-daemon-check--init-server-starts 0)
                                 (list "its init called server-start"))
                            (emx-boot-check-boot-failures)))
                nil (emx-daemon-check--file "own-server-failures"))
  t)

(defun emx-daemon-check-plain-beside ()
  "Check a plain Emacs started beside the running daemon.
No daemon owns the server in its init files, so it finds the daemon's
server running, and must neither start one nor warn."
  (emx-daemon-check--exit "A plain Emacs started beside the daemon"
                          (emx-daemon-check--server-failures)))

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
