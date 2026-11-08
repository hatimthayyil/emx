;;; init.el --- user-init-file                    -*- lexical-binding: t -*-

;; Copyright (C) 2025-2026  Hatim Thayyil
;; Copyright 2016-2019 Jonas Bernoulli
;; SPDX-License-Identifier: Apache-2.0 AND ISC

;; The `before-user-init-time' lines are adapted from emacs.g, under ISC:
;; https://github.com/emacscollective/emacs.g

;; Early birds
(progn					;     startup
  (defvar before-user-init-time (current-time)
    "Value of `current-time' when Emacs begins loading `user-init-file'.")
  (message "Loading Emacs...done (%.3fs)"
           (float-time (time-subtract before-user-init-time
                                      before-init-time))))

;; Support direct evaluation with fallbacks if not run through shim. This
;; ensures that the paths are in a clean Emacs env (eg: emacs -Q).
(unless (boundp 'emx-source-dir)
  (defvar emx-source-dir
    (file-name-directory
     (or load-file-name                   ; when loaded from shim
         buffer-file-name                 ; when buffer is evaluated
         (expand-file-name "init.el" user-emacs-directory))) ; fallback
    "Directory for EMX source files."))

(unless (boundp 'emx-config-dir)
  (defvar emx-config-dir
    (expand-file-name "~/.config/emx/")
    "Directory for EMX configuration files."))

(unless (boundp 'emx-data-dir)
  (defvar emx-data-dir
    (expand-file-name "~/.local/share/emx/")
    "Directory for EMX data files."))

(unless (boundp 'emx-cache-dir)
  (defvar emx-cache-dir
    (expand-file-name "~/.cache/emx/")
    "Directory for EMX cache files."))

;; Set user-emacs-directory if not already set by shim
(unless (equal user-emacs-directory emx-config-dir)
  (setq user-emacs-directory emx-config-dir))

;; Benchmark
(when init-file-debug
  (require 'benchmark-init)
  (add-hook 'after-init-hook #'benchmark-init/deactivate))

;; Before the configuration queues anything, so a reload can take it back.
(autoload 'emx-reload-track-queue (expand-file-name "lisp/emx-reload" emx-source-dir))
(emx-reload-track-queue)

;; Load EmX init shim
(load (expand-file-name "lisp/emx.el" emx-source-dir))

(autoload 'emx-reload (expand-file-name "lisp/emx-reload" emx-source-dir)
  "Bring this Emacs up to date with emx's profile and configuration." t)
(autoload 'emx-reload-record-choices (expand-file-name "lisp/emx-reload" emx-source-dir))
;; Last, so it records what every other startup function chose.
(add-hook 'emacs-startup-hook #'emx-reload-record-choices 90)

(progn					;     startup
  (message "Loading early birds...done (%.3fs)"
           (float-time (time-subtract (current-time)
                                      before-user-init-time))))

(defconst emx-modules
  '(undo file-manager completions terminal base defaults appearance multimedia
    communications programming prose version-control organisation research
    windows navigation finance ai footer)
  "The files in modules/ that init loads, in order.
No name may equal an Emacs library's: loading such a file by path fires that
library's string-keyed `eval-after-load' forms early.")

;; A broken module costs only itself.
(dolist (module emx-modules)
  (condition-case err
      (load (expand-file-name (format "modules/%s.el" module) emx-source-dir)
            nil t)
    (error (display-warning
            'emx (format "Module %s: %s" module (error-message-string err))
            :error))))

;; Local Variables:
;; indent-tabs-mode: nil
;; End:
;;; init.el ends here
