;;; reload.el --- Check that emx-reload brings a running Emacs up to date  -*- lexical-binding: t -*-

;;; Commentary:

;; Loaded after emx's init by nix/checks/default.nix, with $EMX_EMACS naming
;; the Emacs being checked.  It has `emx-reload' load the same generation
;; again, then made-up generations: one that moves packages, tools, native code
;; and manuals, and one that runs a different Emacs.  Fails if Emacs booted
;; with an error or an error-level warning, as the boot check does, and unless
;; each reload leaves this Emacs as a fresh start of that generation would be,
;; apart from what was removed, which the reload must name.

;;; Code:

(require 'cl-lib)
(require 'emx-reload (expand-file-name "lisp/emx-reload" emx-source-dir))

(defvar emx-reload-check--failures nil
  "What failed, newest first.")

(defvar emx-reload-check--result nil
  "The result of the last `emx-reload'.")

(defvar emx-reload-check--dir (make-temp-file "emx-reload-check" t)
  "Where the made-up generations live.")

(defun emx-reload-check--expect (ok what)
  "Note WHAT as a failure unless OK."
  (unless ok
    (push what emx-reload-check--failures)))

(defun emx-reload-check--note-result (result _swapped)
  "Keep RESULT, the result `emx-reload' reports."
  (setq emx-reload-check--result result))

(advice-add 'emx-reload--report :before #'emx-reload-check--note-result)

(defun emx-reload-check--file (name contents)
  "Write CONTENTS to NAME under the check's directory, and return its name."
  (let ((file (expand-file-name name emx-reload-check--dir)))
    (make-directory (file-name-directory file) t)
    (write-region contents nil file)
    file))

(defun emx-reload-check--package (generation name &rest files)
  "Make package NAME of GENERATION with FILES, alternating names and contents.
Return its directory, as twist's manifest names it."
  (let ((dir (format "%s/%s/share/emacs/site-lisp/" generation name)))
    (make-directory (expand-file-name dir emx-reload-check--dir) t)
    (while files
      (emx-reload-check--file (concat dir (pop files)) (pop files)))
    (expand-file-name dir emx-reload-check--dir)))

(defun emx-reload-check--tool (generation)
  "Make the tool emx-fake-tool of GENERATION, and return its directory."
  (let ((file (emx-reload-check--file (format "%s/bin/emx-fake-tool" generation)
                                      "#!/bin/sh\n")))
    (set-file-modes file #o755)
    (directory-file-name (file-name-directory file))))

(defun emx-reload-check--manifest (name manifest)
  "Write MANIFEST, an alist, as the manifest file NAME, and return its name."
  (let ((manifest (copy-alist manifest)))
    (setf (alist-get 'executablePackages manifest)
          (vconcat (alist-get 'executablePackages manifest)))
    (emx-reload-check--file name (json-serialize
                                  (assq-delete-all 'configurationRevision manifest)))))

(defun emx-reload-check--alter (manifest &rest pairs)
  "Return a copy of MANIFEST with PAIRS, alternating keys and values, set."
  (let ((manifest (copy-alist manifest)))
    (while pairs
      (let ((key (pop pairs))
            (value (pop pairs)))
        (setf (alist-get key manifest) value)))
    manifest))

(defun emx-reload-check--dir-p (dir dirs)
  "Return non-nil if DIRS holds directory DIR."
  (seq-some (lambda (other) (emx-reload--same-dir-p dir other)) dirs))

(defun emx-reload-check--path ()
  "Return the directories of PATH."
  (split-string (getenv "PATH") path-separator t))

(defun emx-reload-check--edited-config (text &optional file)
  "Return a copy of the configuration whose FILE ends with TEXT.
FILE is relative to the configuration, and is modules/appearance.el by
default."
  (let ((dir (file-name-as-directory
              (make-temp-file (expand-file-name "edited" emx-reload-check--dir) t))))
    (copy-directory emx-source-dir dir nil nil t)
    (let ((file (expand-file-name (or file "modules/appearance.el") dir)))
      (set-file-modes file #o644)
      (write-region (concat "\n" text "\n") nil file t))
    dir))

(defun emx-reload-check--queued ()
  "Return how many functions wait in `after-load-alist' for features not loaded.
A loaded feature's functions have run, and the configuration needn't queue
them again."
  (apply #'+ (mapcar (lambda (entry)
                       (if (and (symbolp (car entry)) (not (featurep (car entry))))
                           (length (cdr entry))
                         0))
                     after-load-alist)))

;;;; The profile's manifest

(defun emx-reload-check--profile ()
  "Check that the profile carries the manifest this Emacs runs."
  (let ((state (expand-file-name "state" emx-reload-check--dir)))
    (make-directory (expand-file-name "emx" state) t)
    (make-symbolic-link (getenv "EMX_EMACS") (expand-file-name "emx/profile" state))
    (setenv "XDG_STATE_HOME" "relative")
    (emx-reload-check--expect
     (equal (emx-reload-profile-manifest)
            (expand-file-name "~/.local/state/emx/profile/share/emx/manifest.json"))
     "profile: a relative XDG_STATE_HOME doesn't fall back to ~/.local/state")
    (setenv "XDG_STATE_HOME" state)
    (emx-reload-check--expect
     (equal (file-truename (emx-reload-profile-manifest)) twist-current-manifest-file)
     (format "profile: %s isn't the manifest this Emacs runs, %s"
             (emx-reload-profile-manifest) twist-current-manifest-file))
    (emx-reload-check--expect (commandp 'emx-reload) "init: emx-reload isn't a command")))

;;;; The same generation

(defun emx-reload-check--same ()
  "Check that loading the same generation again changes nothing.
A module adding a startup hook function for the first time, as after an
update, must have it run."
  (let ((server server-process)
        (welcome (emx-reload-check--welcomes))
        (jansson (cl-count 'jansson features))
        (queued (emx-reload-check--queued)))
    (emx-reload-check--expect (process-live-p server) "same: no server runs to keep")
    (ws-butler-global-mode -1)
    (remove-hook 'after-init-hook #'ws-butler-global-mode)
    ;; As a configuration that last loaded without it chose.
    (emx-reload-record-choices)
    (let ((outcome (emx-reload)))
      (emx-reload-check--expect (eq outcome 'reloaded)
                                (format "same: emx-reload returned %s" outcome)))
    (let ((result emx-reload-check--result))
      (dolist (slot '(removed removed-modules removed-keys failures))
        (when-let* ((value (cl-struct-slot-value 'emx-reload--result slot result)))
          (push (format "same: %s %S" slot value) emx-reload-check--failures))))
    (emx-reload-check--expect (and ws-butler-global-mode
                                   (memq #'ws-butler-global-mode after-init-hook))
                              "same: a newly hooked after-init function didn't run")
    (dolist (mode '(beginend-global-mode global-jinx-mode vertico-multiform-mode))
      (emx-reload-check--expect (symbol-value mode) (format "same: %s is off" mode)))
    (emx-reload-check--expect (eq server-process server)
                              "same: reloading restarted the server")
    (emx-reload-check--expect (= (emx-reload-check--welcomes) welcome)
                              "same: an after-init function ran again")
    (emx-reload-check--expect (= (cl-count 'jansson features) jansson)
                              "same: reloading added to `features' again")
    (emx-reload-check--expect (= (emx-reload-check--queued) queued)
                              (format "same: what waits for packages to load grew from %d \
to %d functions" queued (emx-reload-check--queued)))))

(defvar emx-check-config-runs 0
  "How often the edited configuration's `:config' for emx-check-deferred ran.")

(defvar emx-check-after-runs 0
  "How often the edited configuration's `:config' for emx-check-after ran.")

(defun emx-reload-check--queue ()
  "Check that reloads leave one copy of a deferred `:config' waiting.
use-package names each `:config' afresh as its form expands, so a package
must still run its configuration once when it loads after two reloads.  So
must one whose form waits `:after' a package that loaded between them,
outside the configuration's loading, and queued the `:config' then."
  (let ((source emx-source-dir))
    (push (emx-reload-check--package "queue" "deferred"
                                     "emx-check-deferred.el" "(provide 'emx-check-deferred)\n"
                                     "emx-check-host.el" "(provide 'emx-check-host)\n"
                                     "emx-check-after.el" "(provide 'emx-check-after)\n")
          load-path)
    (setq emx-source-dir (emx-reload-check--edited-config
                          "(use-package emx-check-deferred :defer t
  :config (cl-incf emx-check-config-runs))
(use-package emx-check-after :defer t :after emx-check-host
  :config (cl-incf emx-check-after-runs))"))
    (unwind-protect
        (progn (emx-reload emx-reload-current-manifest)
               (require 'emx-check-host)
               (emx-reload emx-reload-current-manifest))
      (setq emx-source-dir source))
    (require 'emx-check-deferred)
    (require 'emx-check-after)
    (pcase-dolist (`(,package . ,runs) `((emx-check-deferred . ,emx-check-config-runs)
                                         (emx-check-after . ,emx-check-after-runs)))
      (emx-reload-check--expect (= runs 1)
                                (format "queue: after two reloads, %s's :config ran %d \
times when it loaded" package runs)))))

(defun emx-reload-check--warnings ()
  "Check that a reload fails on the error warnings Emacs logs, and only those.
A warning at an obsolete alias of `:error' is logged at `:error', and a
suppressed one isn't logged."
  (let ((source emx-source-dir))
    (setq emx-source-dir (emx-reload-check--edited-config
                          "(let ((warning-suppress-log-types '((emx-check-suppressed))))
  (display-warning 'emx-check-suppressed \"suppressed\" :error))
(with-suppressed-warnings ((obsolete warning-level-aliases))
  (display-warning 'emx-check-aliased \"aliased\" 'error))"))
    (unwind-protect
        (let ((outcome (emx-reload emx-reload-current-manifest))
              (failures (emx-reload--result-failures emx-reload-check--result)))
          (emx-reload-check--expect
           (and (eq outcome 'restart-recommended)
                (equal failures '("emx-check-aliased: aliased")))
           (format "warnings: emx-reload returned %s, with failures %S" outcome failures)))
      (setq emx-source-dir source))))

(defun emx-reload-check--warnings-edited ()
  "Check that a reload loads an edited lisp/emx-warnings.el."
  (let ((source emx-source-dir))
    (setq emx-source-dir (emx-reload-check--edited-config
                          "(defun emx-reload-check-warnings-edited () t)"
                          "lisp/emx-warnings.el"))
    (unwind-protect
        (progn
          (emx-reload emx-reload-current-manifest)
          (emx-reload-check--expect
           (fboundp 'emx-reload-check-warnings-edited)
           "edited emx-warnings.el: the reload didn't load it"))
      (fmakunbound 'emx-reload-check-warnings-edited)
      (setq emx-source-dir source))))

(defun emx-reload-check--welcomes ()
  "Return how many welcomes *scratch* holds."
  (with-current-buffer (get-buffer-create "*scratch*")
    (how-many "Welcome to EMX" (point-min) (point-max))))

;;;; Removals

(defun emx-reload-check--removals ()
  "Check that a reload names the module and key that went, and only once.
Each stands for one an older configuration had."
  (setq emx-modules (append emx-modules '(emx-check-gone)))
  (push '(("C-c z") emx-check-command nil) personal-keybindings)
  (let ((outcome (emx-reload)))
    (emx-reload-check--expect (eq outcome 'restart-recommended)
                              (format "removals: emx-reload returned %s" outcome)))
  (let ((result emx-reload-check--result))
    (pcase-dolist (`(,slot . ,expected)
                   '((removed-modules emx-check-gone)
                     (removed-keys "C-c z")
                     (failures)))
      (let ((value (cl-struct-slot-value 'emx-reload--result slot result)))
        (emx-reload-check--expect (equal value expected)
                                  (format "removals: %s is %S, not %S"
                                          slot value expected)))))
  (let ((outcome (emx-reload)))
    (emx-reload-check--expect (eq outcome 'reloaded)
                              (format "removals: the next emx-reload returned %s, naming %S"
                                      outcome (emx-reload--removed emx-reload-check--result)))))

;;;; A generation that moves things

(defun emx-reload-check--swap ()
  "Check a reload to a generation that changes, adds and removes packages.
It also moves the tools, native code and manuals."
  (let* ((base (emx-reload--read-manifest emx-reload-current-manifest))
         (provide "(provide 'emx-fake)\n")
         (old-fake (emx-reload-check--package
                    "old" "fake" "emx-fake.el"
                    (concat "(defvar emx-fake-generation 1)\n" provide)
                    "sub/emx-fake-sub.el" "" "gone-sub/emx-fake-gone.el" ""))
         (new-fake (emx-reload-check--package
                    "new" "fake" "emx-fake.el"
                    (concat "(setq emx-fake-generation 2)\n"
                            "(defvar emx-fake-inhibited inhibit-interaction)\n"
                            provide)
                    "emx-fake-autoloads.el" "(defvar emx-fake-autoloaded t)\n"
                    "sub/emx-fake-sub.el" ""))
         (added (emx-reload-check--package
                 "new" "added" "emx-added-autoloads.el" "(defvar emx-added-autoloaded t)\n"))
         (gone (emx-reload-check--package "old" "gone" "emx-gone.el" ""))
         (old-tools (emx-reload-check--tool "old"))
         (new-tools (emx-reload-check--tool "new"))
         (old-native (expand-file-name "old/native-lisp/" emx-reload-check--dir))
         (new-native (expand-file-name "new/native-lisp/" emx-reload-check--dir))
         (old-info (expand-file-name "old/info" emx-reload-check--dir))
         (new-info (expand-file-name "new/info" emx-reload-check--dir))
         (packages (alist-get 'elispPackages base))
         (tools (alist-get 'executablePackages base))
         (old (emx-reload-check--alter
               base
               'elispPackages `((emx-fake . ,old-fake) (emx-gone . ,gone) ,@packages)
               'executablePackages (cons old-tools tools)
               'nativeLoadPath old-native
               'infoPath old-info))
         (new (emx-reload-check--alter
               base
               'elispPackages `((emx-fake . ,new-fake) (emx-added . ,added) ,@packages)
               'executablePackages (cons new-tools tools)
               'nativeLoadPath new-native
               'infoPath new-info))
         (eln-first (car native-comp-eln-load-path)))
    ;; This Emacs as the old generation would have started it.
    (setq emx-reload-current-manifest (emx-reload-check--manifest "old.json" old))
    (dolist (dir (list (concat old-fake "gone-sub") (concat old-fake "sub") old-fake gone))
      (push dir load-path))
    (require 'emx-fake)
    (push (file-name-as-directory old-tools) exec-path)
    (setenv "PATH" (concat old-tools path-separator (getenv "PATH")))
    (setq native-comp-eln-load-path
          (cons eln-first (cons old-native (cdr native-comp-eln-load-path))))
    (setenv "INFOPATH" (concat old-info path-separator (getenv "INFOPATH")))
    (require 'info)
    (info-initialize)
    (push old-info Info-directory-list)
    (let* ((file (emx-reload-check--manifest "new.json" new))
           (outcome (emx-reload file)))
      (emx-reload-check--expect (eq outcome 'restart-recommended)
                                (format "swap: emx-reload returned %s" outcome))
      (emx-reload-check--expect (equal emx-reload-current-manifest (file-truename file))
                                "swap: the current manifest didn't move"))
    (let ((result emx-reload-check--result))
      (pcase-dolist (`(,slot . ,expected)
                     '((changed emx-fake) (added emx-added) (removed emx-gone) (failures)))
        (let ((value (cl-struct-slot-value 'emx-reload--result slot result)))
          (emx-reload-check--expect (equal value expected)
                                    (format "swap: %s is %S, not %S" slot value expected)))))
    (pcase-dolist (`(,ok . ,what)
                   `((,(eql (bound-and-true-p emx-fake-generation) 2)
                      . "the loaded feature didn't load again from the new package")
                     (,(eq (bound-and-true-p emx-fake-inhibited) t)
                      . "a file loaded again could prompt")
                     (,(bound-and-true-p emx-fake-autoloaded)
                      . "a changed package's autoloads didn't load")
                     (,(bound-and-true-p emx-added-autoloaded)
                      . "an added package's autoloads didn't load")
                     (,(and (member new-fake load-path) (member added load-path))
                      . "load-path lacks the new packages")
                     (,(member (concat new-fake "sub") load-path)
                      . "a package's subdirectory didn't move")
                     (,(not (seq-some (lambda (dir)
                                        (and (stringp dir)
                                             (string-prefix-p
                                              (expand-file-name "old/" emx-reload-check--dir)
                                              dir)))
                                      load-path))
                      . "load-path keeps old directories")
                     (,(equal (executable-find "emx-fake-tool")
                              (expand-file-name "emx-fake-tool" new-tools))
                      . "exec-path finds the old tool")
                     (,(equal (string-trim (shell-command-to-string "command -v emx-fake-tool"))
                              (expand-file-name "emx-fake-tool" new-tools))
                      . "a child process finds the old tool")
                     (,(not (or (emx-reload-check--dir-p old-tools exec-path)
                                (emx-reload-check--dir-p old-tools (emx-reload-check--path))))
                      . "exec-path or PATH keeps the old tools")
                     (,(and (equal (car native-comp-eln-load-path) eln-first)
                            (emx-reload-check--dir-p new-native native-comp-eln-load-path)
                            (not (emx-reload-check--dir-p old-native native-comp-eln-load-path)))
                      . "native-comp-eln-load-path didn't move to the new native code")
                     (,(and (emx-reload-check--dir-p new-info Info-directory-list)
                            (not (emx-reload-check--dir-p old-info Info-directory-list)))
                      . "Info-directory-list didn't move to the new manuals")
                     (,(let ((dirs (split-string (getenv "INFOPATH") path-separator)))
                         (and (emx-reload-check--dir-p new-info dirs)
                              (not (emx-reload-check--dir-p old-info dirs))))
                      . "INFOPATH didn't move to the new manuals")))
      (emx-reload-check--expect ok (concat "swap: " what)))))

;;;; A different Emacs

(defun emx-reload-check--other-emacs ()
  "Check that a generation running a different Emacs changes nothing."
  (let* ((manifest (emx-reload--read-manifest emx-reload-current-manifest))
         (current emx-reload-current-manifest)
         (paths (list load-path exec-path (getenv "PATH") native-comp-eln-load-path))
         (init (assoc (expand-file-name "init.el" emx-source-dir) load-history)))
    (setf (alist-get 'emacsPath manifest) "/nix/store/00000000000000000000000000000000-emacs")
    (let ((outcome (emx-reload (emx-reload-check--manifest "other.json" manifest))))
      (emx-reload-check--expect (eq outcome 'restart-required)
                                (format "other Emacs: emx-reload returned %s" outcome)))
    (emx-reload-check--expect
     (and (equal emx-reload-current-manifest current)
          (equal (list load-path exec-path (getenv "PATH") native-comp-eln-load-path) paths)
          (eq (assoc (expand-file-name "init.el" emx-source-dir) load-history) init))
     "other Emacs: the reload changed something")))

;;;; The report emx's CLI reads

(defun emx-reload-check--read-report (file)
  "Return the report in FILE, as an alist."
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents file))
    (json-parse-buffer :object-type 'alist)))

(defun emx-reload-check--report-file ()
  "Check the report `emx-reload-write-report' writes for emx's CLI.
A manifest that doesn't exist, as for a generation built before emx-reload,
must give `restart-required' with the refusal, while `emx-reload' itself
still refuses with a `user-error'."
  (let ((file (expand-file-name "report.json" emx-reload-check--dir))
        (missing (expand-file-name "missing.json" emx-reload-check--dir)))
    (emx-reload-check--expect
     (eq (condition-case nil (progn (emx-reload missing) 'returned)
           (user-error 'refused))
         'refused)
     "report: emx-reload didn't refuse a missing manifest")
    (pcase-dolist (`(,manifest ,outcome ,mention)
                   `((,missing "restart-required" "No manifest at")
                     (,emx-reload-current-manifest "reloaded" "same generation")))
      (when (file-exists-p file)
        (delete-file file))
      (let ((returned (emx-reload-write-report file manifest)))
        (emx-reload-check--expect (equal (symbol-name returned) outcome)
                                  (format "report: returned %s, not %s" returned outcome)))
      (let ((report (and (file-exists-p file) (emx-reload-check--read-report file))))
        (emx-reload-check--expect
         (and (equal (alist-get 'outcome report) outcome)
              (string-search mention (or (alist-get 'report report) ""))
              (eq (alist-get 'daemon report) :false))
         (format "report: %S isn't %s naming %S, from no daemon" report outcome mention))))
    (cl-letf (((symbol-function 'daemonp) #'always))
      (emx-reload-write-report file emx-reload-current-manifest))
    (emx-reload-check--expect
     (eq (alist-get 'daemon (emx-reload-check--read-report file)) t)
     "report: a daemon's report doesn't say it came from a daemon")))

;;;; Choices made while Emacs runs

(defun emx-reload-check--choices ()
  "Check that a reload keeps the user's theme and global modes.
A choice the configuration's edit changes is the configuration's again.
The hook function that records choices must be there once, however often
Emacs reloads."
  (modus-themes-load-theme 'modus-vivendi)
  (blink-cursor-mode 1)
  (emx-reload)
  (emx-reload-check--expect (eq (car custom-enabled-themes) 'modus-vivendi)
                            (format "choices: the theme the user chose became %s"
                                    (car custom-enabled-themes)))
  (emx-reload-check--expect blink-cursor-mode
                            "choices: blink-cursor-mode, which the user turned on, is off")
  (let ((source emx-source-dir))
    (setq emx-source-dir (emx-reload-check--edited-config
                          "(load-theme 'modus-vivendi-tinted t)"))
    (unwind-protect (emx-reload)
      (setq emx-source-dir source)))
  (emx-reload-check--expect (eq (car custom-enabled-themes) 'modus-vivendi-tinted)
                            (format "choices: the theme the edited configuration loads became %s"
                                    (car custom-enabled-themes)))
  (let ((count (cl-count #'emx-reload-record-choices
                         (default-value 'emacs-startup-hook))))
    (emx-reload-check--expect (= count 1)
                              (format "choices: emacs-startup-hook holds \
emx-reload-record-choices %d times"
                                      count))))

(defun emx-reload-check--choices-moved ()
  "Check that an edited configuration's mode wins when the mode's package moved.
The same reload loads the package that defines the mode from a new directory."
  (let* ((base (emx-reload--read-manifest emx-reload-current-manifest))
         (mode (concat "(define-minor-mode emx-check-mode \"A mode the check edits.\" \
:global t)\n" "(provide 'emx-check-mode)\n"))
         (old-dir (emx-reload-check--package "mode-old" "mode" "emx-check-mode.el" mode))
         (new-dir (emx-reload-check--package "mode-new" "mode" "emx-check-mode.el" mode))
         (packages (alist-get 'elispPackages base))
         (source emx-source-dir))
    (setq emx-reload-current-manifest
          (emx-reload-check--manifest
           "mode-old.json"
           (emx-reload-check--alter base 'elispPackages
                                    `((emx-check-mode . ,old-dir) ,@packages))))
    (push old-dir load-path)
    (require 'emx-check-mode)
    ;; As a configuration that last loaded with the mode off chose.
    (emx-reload-record-choices)
    (setq emx-source-dir (emx-reload-check--edited-config
                          "(require 'emx-check-mode)\n(emx-check-mode 1)"))
    (unwind-protect
        (emx-reload (emx-reload-check--manifest
                     "mode-new.json"
                     (emx-reload-check--alter base 'elispPackages
                                              `((emx-check-mode . ,new-dir) ,@packages))))
      (setq emx-source-dir source))
    (emx-reload-check--expect
     (and (equal (symbol-file 'emx-check-mode 'defun)
                 (expand-file-name "emx-check-mode.el" new-dir))
          emx-check-mode)
     "choices: the mode the edited configuration turns on is off, as its package moved")))

(defun emx-reload-check--gone-theme ()
  "Check that a chosen theme whose file is gone doesn't stop the reload.
The configuration's theme stays, and the report names the theme."
  (let ((dir (file-name-directory
              (emx-reload-check--file "themes/emx-check-theme.el"
                                      "(deftheme emx-check)\n(provide-theme 'emx-check)\n"))))
    (push dir custom-theme-load-path)
    (load-theme 'emx-check t)
    (setq custom-theme-load-path (delete dir custom-theme-load-path))
    (let ((outcome (emx-reload emx-reload-current-manifest))
          (failures (emx-reload--result-failures emx-reload-check--result)))
      (emx-reload-check--expect
       (and (eq outcome 'restart-recommended)
            (seq-some (lambda (failure) (string-search "emx-check" failure)) failures))
       (format "gone theme: emx-reload returned %s, with failures %S" outcome failures)))
    (emx-reload-check--expect (not (eq (car custom-enabled-themes) 'emx-check))
                              (format "gone theme: the themes are %S" custom-enabled-themes))))

;; Batch Emacs exits before it reaches this hook.
(run-hooks 'emacs-startup-hook)

;; What went wrong as Emacs booted, before the checks give warnings of their
;; own.  The check's early-init loads record-warnings.el.
(setq emx-reload-check--failures
      (condition-case err
          (reverse (emx-boot-check-boot-failures))
        (error (list (format "boot: %s" (error-message-string err))))))

(dolist (check '(emx-reload-check--profile emx-reload-check--same emx-reload-check--queue
                 emx-reload-check--warnings emx-reload-check--warnings-edited
                 emx-reload-check--removals emx-reload-check--swap
                 emx-reload-check--other-emacs emx-reload-check--report-file
                 emx-reload-check--choices emx-reload-check--choices-moved
                 emx-reload-check--gone-theme))
  (condition-case err
      (funcall check)
    (error (push (format "%s: %s" check (error-message-string err))
                 emx-reload-check--failures))))

(princ (format "emx reload check: %d failures\n" (length emx-reload-check--failures)))
(dolist (failure (reverse emx-reload-check--failures))
  (princ (format "FAIL %s\n" failure)))
(kill-emacs (if emx-reload-check--failures 1 0))

;;; reload.el ends here
