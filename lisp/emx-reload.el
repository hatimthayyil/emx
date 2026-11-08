;;; emx-reload.el --- Bring a running Emacs up to date with emx  -*- lexical-binding: t -*-

;; Copyright (C) 2026  Hatim Thayyil
;; SPDX-License-Identifier: Apache-2.0

;;; Commentary:

;; `emx-reload' moves a running Emacs to the generation of emx's profile that
;; `emx sync', `emx upgrade' or `emx rollback' made current, then loads emx's
;; configuration again, so neither needs a restart.
;;
;; Each build of emx's Emacs carries twist's manifest: the Emacs it runs, the
;; directory of every package, and the native code, Info manuals and tools of
;; the package set.  Comparing the manifest this Emacs runs with the new one
;; gives the packages that changed, came or went.  Their directories move on
;; `load-path', the files this Emacs loaded from a changed package load again
;; from the new one, and the other search paths follow.
;;
;; A reload adds and changes, but can't remove: a package, module or key that
;; went stays in this Emacs until it restarts, so the report names it.  Other
;; removed configuration, such as a hook or a definition, stays unnamed.  A
;; different Emacs can't be loaded at all.  early-init.el doesn't load again.

;;; Code:

(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'emx-warnings (expand-file-name "emx-warnings"
                                         (file-name-directory
                                          (or (macroexp-file-name) buffer-file-name))))

(defvar emx-modules)
(defvar emx-source-dir)
(defvar personal-keybindings)
(defvar twist-current-manifest-file)
(defvar Info-directory-list)
(defvar Info-dir-contents)
(defvar Info-dir-contents-directory)
(defvar Info-dir-file-attributes)

(defvar emx-reload-current-manifest (bound-and-true-p twist-current-manifest-file)
  "The manifest of the generation this Emacs runs, as a store file name.
It starts as `twist-current-manifest-file', the manifest this Emacs started
with, and `emx-reload' moves it to each manifest it loads.")

(defun emx-reload-profile-manifest ()
  "Return the manifest of the current generation of emx's profile.
The profile is $XDG_STATE_HOME/emx/profile, as `emx sync' makes it: an unset,
empty or relative XDG_STATE_HOME stands for ~/.local/state."
  (let ((state (getenv "XDG_STATE_HOME")))
    (expand-file-name "emx/profile/share/emx/manifest.json"
                      (if (and state (file-name-absolute-p state))
                          state
                        (expand-file-name ".local/state" "~")))))

(cl-defstruct (emx-reload--result (:constructor emx-reload--result-create))
  "What one `emx-reload' did."
  changed added removed (files 0) tools info
  modules removed-modules removed-keys failures)

(defvar emx-reload--result nil
  "The result of the `emx-reload' in progress, which display warnings go to.")

;;;; Manifests

(defun emx-reload--read-manifest (file)
  "Return the manifest in FILE as an alist."
  (with-temp-buffer
    (insert-file-contents file)
    (json-parse-buffer :object-type 'alist :array-type 'list :null-object nil)))

(defun emx-reload--packages (old new)
  "Compare OLD and NEW, alists of package names and directories.
Return (CHANGED ADDED REMOVED).  CHANGED holds (NAME OLD-DIR . NEW-DIR), ADDED
and REMOVED hold (NAME . DIR)."
  (let (changed added removed)
    (pcase-dolist (`(,name . ,dir) old)
      (let ((new-dir (alist-get name new)))
        (cond ((null new-dir) (push (cons name dir) removed))
              ((not (equal dir new-dir))
               (push (cons name (cons dir new-dir)) changed)))))
    (pcase-dolist (`(,name . ,dir) new)
      (unless (assq name old)
        (push (cons name dir) added)))
    (list (nreverse changed) (nreverse added) (nreverse removed))))

;;;; Search paths

(defun emx-reload--rebase (path old new)
  "Return PATH moved from directory OLD to directory NEW.
Return nil when PATH isn't OLD or under it."
  (let ((old (file-name-as-directory old)))
    (cond ((string-prefix-p old path)
           (concat (file-name-as-directory new) (substring path (length old))))
          ((equal (file-name-as-directory path) old)
           (directory-file-name new)))))

(defun emx-reload--move (dir moves)
  "Return DIR after MOVES, or :gone when its package went.
MOVES holds (OLD . NEW) package directories, with a nil NEW for a removed
package.  A directory that isn't in the new package goes too."
  (or (and (stringp dir)
           (seq-some (lambda (move)
                       (when-let* ((moved (emx-reload--rebase
                                           dir (car move) (or (cdr move) (car move)))))
                         (if (and (cdr move) (file-directory-p moved)) moved :gone)))
                     moves))
      dir))

(defun emx-reload--same-dir-p (a b)
  "Return non-nil if A and B name the same directory."
  (and (stringp a) (stringp b)
       (equal (file-name-as-directory a) (file-name-as-directory b))))

(defun emx-reload--replace (dirs old new)
  "Return DIRS with directory OLD replaced by NEW.
Without OLD, NEW goes second: the first entry of `native-comp-eln-load-path'
is where Emacs writes the native code it compiles."
  (delete-dups
   (if (seq-some (lambda (dir) (emx-reload--same-dir-p dir old)) dirs)
       (mapcar (lambda (dir) (if (emx-reload--same-dir-p dir old) new dir)) dirs)
     (cons (car dirs) (cons new (cdr dirs))))))

(defun emx-reload--reprefix (dirs old new)
  "Return DIRS without the directories OLD and NEW, after NEW."
  (append new (seq-remove (lambda (dir)
                            (seq-some (lambda (other) (emx-reload--same-dir-p dir other))
                                      (append old new)))
                          dirs)))

(defun emx-reload--swap-tools (old new)
  "Move `exec-path' and PATH from the tool directories OLD to NEW.
Emacs looks programs up in `exec-path', and the processes it starts in PATH."
  (set-default 'exec-path (emx-reload--reprefix (default-value 'exec-path) old
                                                (mapcar #'file-name-as-directory new)))
  (setenv "PATH" (string-join (emx-reload--reprefix
                               (split-string (or (getenv "PATH") "") path-separator t)
                               old new)
                              path-separator)))

(defun emx-reload--swap-info (old new)
  "Move Info from the manual directory OLD to NEW.
`info-initialize' reads INFOPATH, so the environment moves as well as
`Info-directory-list'.  An empty element of INFOPATH means Info's defaults,
so empty elements stay."
  (setenv "INFOPATH" (string-join (emx-reload--replace
                                   (split-string (or (getenv "INFOPATH") "") path-separator)
                                   old new)
                                  path-separator))
  (when (and (featurep 'info) Info-directory-list)
    (setq Info-directory-list (emx-reload--replace Info-directory-list old new)
          Info-dir-contents nil
          Info-dir-contents-directory nil
          Info-dir-file-attributes nil)))

;;;; Loading

(defun emx-reload--fail (what err)
  "Note in the reload's result that WHAT failed with ERR."
  (push (format "%s: %s" what (error-message-string err))
        (emx-reload--result-failures emx-reload--result)))

(defun emx-reload--load (file &optional missing-ok)
  "Load FILE, noting a failure in the reload's result.
Return non-nil if it loaded.  With MISSING-OK, a missing FILE isn't a failure."
  (condition-case err
      (load file missing-ok t)
    (error (emx-reload--fail (abbreviate-file-name file) err)
           nil)))

(defun emx-reload--note-warning (type message &optional level &rest _)
  "Note warning MESSAGE of TYPE as a failure when Emacs logs it as an error.
LEVEL is the level `display-warning' got.  init.el reports a module that
fails this way, and use-package a failing `:config'."
  (when (and emx-reload--result
             (memq (emx-warnings-logged-level type level) '(:error :emergency)))
    (push (format "%s: %s" type message)
          (emx-reload--result-failures emx-reload--result))))

(defun emx-reload--loaded-files (changed)
  "Return (FILE . NEW-FILE) for each file loaded from a CHANGED package.
Only files that provide a feature count, autoloads aside.  The oldest-loaded
file comes first, so a package's libraries load again in their first order."
  (let (files)
    (dolist (entry load-history)
      (when-let* ((file (car entry))
                  ((stringp file))
                  ((not (string-suffix-p "-autoloads" (file-name-base file))))
                  ((seq-some (lambda (item) (eq (car-safe item) 'provide)) (cdr entry)))
                  (new-file (seq-some (lambda (change)
                                        (emx-reload--rebase file (cadr change) (cddr change)))
                                      changed)))
        (push (cons file new-file) files)))
    files))

(defun emx-reload--swap-packages (old new)
  "Move this Emacs from manifest OLD to manifest NEW."
  (pcase-let* ((result emx-reload--result)
               (`(,changed ,added ,removed)
                (emx-reload--packages (alist-get 'elispPackages old)
                                      (alist-get 'elispPackages new)))
               (files (emx-reload--loaded-files changed))
               (moves (append (mapcar (lambda (change) (cons (cadr change) (cddr change)))
                                      changed)
                              (mapcar (lambda (package) (cons (cdr package) nil))
                                      removed))))
    (setf (emx-reload--result-changed result) (mapcar #'car changed)
          (emx-reload--result-added result) (mapcar #'car added)
          (emx-reload--result-removed result) (mapcar #'car removed))
    ;; Native code first: a file loaded again finds its compiled code there.
    (when (and (boundp 'native-comp-eln-load-path)
               (not (equal (alist-get 'nativeLoadPath old) (alist-get 'nativeLoadPath new))))
      (setq native-comp-eln-load-path
            (emx-reload--replace native-comp-eln-load-path
                                 (alist-get 'nativeLoadPath old)
                                 (alist-get 'nativeLoadPath new))))
    (setq load-path
          (append (seq-remove (lambda (dir) (member dir load-path)) (mapcar #'cdr added))
                  (mapcan (lambda (dir)
                            (let ((moved (emx-reload--move dir moves)))
                              (unless (eq moved :gone) (list moved))))
                          load-path)))
    (unless (equal (alist-get 'executablePackages old) (alist-get 'executablePackages new))
      (emx-reload--swap-tools (alist-get 'executablePackages old)
                              (alist-get 'executablePackages new))
      (setf (emx-reload--result-tools result) t))
    (unless (equal (alist-get 'infoPath old) (alist-get 'infoPath new))
      (emx-reload--swap-info (alist-get 'infoPath old) (alist-get 'infoPath new))
      (setf (emx-reload--result-info result) t))
    (pcase-dolist (`(,name . ,dir)
                   (append added (mapcar (lambda (change) (cons (car change) (cddr change)))
                                         changed)))
      (emx-reload--load (expand-file-name (format "%s-autoloads" name) dir) t))
    (pcase-dolist (`(,file . ,new-file) files)
      (when (emx-reload--load (file-name-sans-extension new-file))
        (setq load-history (assoc-delete-all file load-history))
        (cl-incf (emx-reload--result-files result))))))

(defvar emx-reload--queued nil
  "What emx's configuration queued in `after-load-alist' since it last loaded.
An alist of keys of `after-load-alist' and the functions queued under each.")

(defun emx-reload--config-file-p (file)
  "Return non-nil if FILE is one of emx's configuration files."
  (and file (bound-and-true-p emx-source-dir)
       (string-prefix-p (file-name-as-directory (expand-file-name emx-source-dir))
                        (expand-file-name file))))

(defvar emx-reload--running nil
  "While a function emx's configuration queued runs, a list of the file loading.
The list holds `load-file-name' as the function started: what it queues is
the configuration's until another file loads.")

(defun emx-reload--configuration-p ()
  "Return non-nil if what queues now is emx's configuration.
It is while a configuration file loads, and while a function the
configuration queued runs, as when `:after' queues a `:config' once the
package it waits for loads.  A package that loads then queues for itself, and
its queue must stay."
  (or (emx-reload--config-file-p load-file-name)
      (and emx-reload--running (equal (car emx-reload--running) load-file-name))))

(defun emx-reload--as-configuration (form)
  "Return FORM, as `eval-after-load' takes it, as a function of the configuration.
The function runs FORM with `emx-reload--running' set, so what it queues
counts as the configuration's."
  (let ((function (if (functionp form) form (eval `(lambda () ,form) lexical-binding))))
    (lambda ()
      (let ((emx-reload--running (list load-file-name)))
        (funcall function)))))

(defun emx-reload--track (eval-after-load file form)
  "Call EVAL-AFTER-LOAD on FILE and FORM, noting what emx's configuration queues."
  (if (not (emx-reload--configuration-p))
      (funcall eval-after-load file form)
    (let* ((key (if (stringp file) (load-history-regexp file) file))
           (before (copy-sequence (cdr (assoc key after-load-alist))))
           (value (funcall eval-after-load file (emx-reload--as-configuration form))))
      (when-let* ((queued (seq-remove (lambda (function) (memq function before))
                                      (cdr (assoc key after-load-alist)))))
        (push (cons key queued) emx-reload--queued))
      value)))

;;;###autoload
(defun emx-reload-track-queue ()
  "Note what emx's configuration queues to run after a package loads.
What its previous load queued comes off `after-load-alist' first.  use-package
names each `:config' afresh every time its form expands, so `eval-after-load'
keeps the copy an earlier load queued beside the new one, and a package that
loads after N reloads would run its `:config' N+1 times.  init.el calls this
before the configuration loads, at startup and on each `emx-reload'."
  (pcase-dolist (`(,key . ,functions) emx-reload--queued)
    (when-let* ((entry (assoc key after-load-alist)))
      (setcdr entry (seq-remove (lambda (function) (memq function functions))
                                (cdr entry)))))
  (setq emx-reload--queued nil)
  (advice-add 'eval-after-load :around #'emx-reload--track))

(defun emx-reload--key-name (key)
  "Return a description of KEY, a key of `personal-keybindings'."
  (if (cdr key) (format "%s in %s" (car key) (cdr key)) (car key)))

(defun emx-reload--load-config ()
  "Load emx's init.el again, and note in the reload's result what it found.
Startup hooks have run, so the functions the configuration newly adds to
them run now.  Comparing with what was there before gives the modules and
`bind-key' keys it no longer has."
  (let* ((result emx-reload--result)
         (old-modules (bound-and-true-p emx-modules))
         (old-keys (copy-tree personal-keybindings))
         (hooks '(after-init-hook emacs-startup-hook))
         (old-hooks (mapcar #'default-value hooks))
         new-hooks new-keys)
    (let ((after-init-hook nil)
          (emacs-startup-hook nil)
          (personal-keybindings nil))
      (emx-reload--load (expand-file-name "init.el" emx-source-dir))
      (setq new-hooks (list after-init-hook emacs-startup-hook)
            new-keys personal-keybindings))
    (cl-mapc (lambda (hook old new)
               (dolist (function new)
                 (unless (or (eq function t) (member function old))
                   (condition-case err
                       (funcall function)
                     (error (emx-reload--fail function err)))
                   (add-hook hook function t))))
             hooks old-hooks new-hooks)
    (setf (emx-reload--result-removed-keys result)
          (mapcar (lambda (entry) (emx-reload--key-name (car entry)))
                  (seq-remove (lambda (entry) (assoc (car entry) new-keys)) old-keys)))
    ;; A removed key stays bound, but leaves `personal-keybindings', so the
    ;; next reload doesn't report it again.
    (setq personal-keybindings
          (seq-filter (lambda (entry) (assoc (car entry) new-keys)) personal-keybindings))
    (dolist (entry new-keys)
      (if-let* ((known (assoc (car entry) personal-keybindings)))
          (setcar (cdr known) (cadr entry))
        (push entry personal-keybindings)))
    (setf (emx-reload--result-modules result) emx-modules
          (emx-reload--result-removed-modules result) (seq-difference old-modules emx-modules))))

;;;; Report

(defun emx-reload--names (names)
  "Return NAMES as a comma-separated string."
  (mapconcat (lambda (name) (format "%s" name)) names ", "))

(defun emx-reload--count (names what)
  "Describe NAMES, the packages that WHAT, or return nil when there are none."
  (when names
    (format "%d %s (%s)" (length names) what (emx-reload--names names))))

(defun emx-reload--summary (result)
  "Describe how RESULT moved the packages, as a list of strings."
  (append
   (or (delq nil (list (emx-reload--count (emx-reload--result-changed result) "changed")
                       (emx-reload--count (emx-reload--result-added result) "added")
                       (emx-reload--count (emx-reload--result-removed result) "removed")))
       (list "no package changed"))
   (list (let ((files (emx-reload--result-files result)))
           (format "%d %s loaded again" files (if (= files 1) "file" "files"))))
   (and (emx-reload--result-tools result) (list "tools moved"))
   (and (emx-reload--result-info result) (list "Info moved"))))

(defun emx-reload--removed (result)
  "Return what RESULT says this Emacs still holds, as a list of strings."
  (delq nil
        (list (when-let* ((names (emx-reload--result-removed result)))
                (concat "packages " (emx-reload--names names)))
              (when-let* ((names (emx-reload--result-removed-modules result)))
                (concat "modules " (emx-reload--names names)))
              (when-let* ((names (emx-reload--result-removed-keys result)))
                (concat "keys " (emx-reload--names names))))))

(defun emx-reload--report (result swapped)
  "Return the report of RESULT.  SWAPPED says whether the packages moved."
  (let ((removed (emx-reload--removed result))
        (failures (reverse (emx-reload--result-failures result))))
    (string-join
     (delq nil
           (list (format "emx-reload: packages: %s; configuration loaded, %d modules."
                         (if swapped
                             (string-join (emx-reload--summary result) ", ")
                           "same generation")
                         (length (emx-reload--result-modules result)))
                 (and removed
                      (format "Restart Emacs to drop what was removed: %s."
                              (string-join removed "; ")))
                 (and failures
                      (format "Restart Emacs to recover from what failed: %s."
                              (string-join failures "; ")))))
     "\n")))

;;;; Command

(defun emx-reload (&optional manifest)
  "Bring this Emacs up to date with emx's profile and configuration.
MANIFEST is the manifest of the generation to load, by default the one
`emx-reload-profile-manifest' returns; interactively, a prefix argument asks
for it.  The packages, native code, Info manuals and tools move to that
generation, then init.el loads again from `emx-source-dir'.  Nothing prompts:
a prompt fails as an error.

Return `reloaded'; `restart-recommended' when something was removed, which
this Emacs keeps until it restarts, or failed; or `restart-required' when the
generation runs a different Emacs, in which case nothing changes."
  (interactive (list (and current-prefix-arg
                          (read-file-name "Manifest: " nil nil t))))
  (let ((file (file-truename (or manifest (emx-reload-profile-manifest)))))
    (unless (file-readable-p file)
      (user-error "No manifest at %s: restart Emacs to load this generation" file))
    (unless emx-reload-current-manifest
      (user-error "This Emacs didn't start from emx's profile, so it has no manifest"))
    (let ((old (emx-reload--read-manifest emx-reload-current-manifest))
          (new (emx-reload--read-manifest file)))
      (if (not (equal (alist-get 'emacsPath old) (alist-get 'emacsPath new)))
          (progn
            (message "emx-reload: this generation runs a different Emacs, %s; restart \
Emacs to load it.  Nothing changed." (alist-get 'emacsPath new))
            'restart-required)
        (let* ((emx-reload--result (emx-reload--result-create))
               (swapped (not (equal file (file-truename emx-reload-current-manifest))))
               (inhibit-interaction t))
          (advice-add 'display-warning :before #'emx-reload--note-warning)
          (unwind-protect
              ;; Away from the current buffer, which may hold its own
              ;; `process-environment' and `exec-path'.
              (with-temp-buffer
                ;; init.el doesn't load it, so an edit to it reaches this
                ;; Emacs only here.
                (emx-reload--load (expand-file-name "lisp/emx-warnings" emx-source-dir))
                (when swapped
                  (emx-reload--swap-packages old new)
                  (setq emx-reload-current-manifest file))
                (emx-reload--load-config))
            (advice-remove 'display-warning #'emx-reload--note-warning))
          (message "%s" (emx-reload--report emx-reload--result swapped))
          (if (or (emx-reload--removed emx-reload--result)
                  (emx-reload--result-failures emx-reload--result))
              'restart-recommended
            'reloaded))))))

(provide 'emx-reload)

;;; emx-reload.el ends here
