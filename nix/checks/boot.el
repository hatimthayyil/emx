;;; boot.el --- Check that emx boots and loads every package  -*- lexical-binding: t -*-

;;; Commentary:

;; Loaded after emx's init by nix/checks/default.nix, whose early-init first
;; loads record-warnings.el.  Fails unless no warning at level `:error' or
;; `:emergency' was logged, init loaded every file in modules/, booting and
;; opening files turns on what the modules promise, the main feature of every
;; package named in the file $EMX_PACKAGES loads without an error from its
;; configuration, the native parts are in place, and Emacs finds the tools its
;; modules call.  The build sandbox has none of them, so they can only come
;; from Emacs's wrapper.  It also fails if loading gptel reads an API key.
;;
;; Under `use-package-always-defer' a form with no trigger never loads its
;; package and nothing says so.  The modes are therefore checked first, before
;; this file loads any package itself.

;;; Code:

(defconst emx-boot-check-features
  '((auctex . tex)
    (git-modes . gitconfig-mode)
    (poet-theme . nil))
  "Packages whose main feature is not named after them.
A nil feature means the package is a theme: its file is only located.")

(defconst emx-boot-check-tools
  '("rg" "fd" "yt-dlp" "claude-agent-acp" "claude-code-ide-claude" "direnv" "vale"
    "vale-ls" "basedpyright" "ruff" "beancount-language-server")
  "Programs that Emacs's wrapper puts on PATH.")

(defconst emx-boot-check-global-modes
  '(beginend-global-mode global-jinx-mode vertico-multiform-mode
    ws-butler-global-mode)
  "Global modes that booting turns on.")

(defconst emx-boot-check-file-modes
  '(("note.txt" flycheck-mode ws-butler-mode)
    ("note.org" flycheck-mode ws-butler-mode)
    ("note.md" flycheck-mode)
    ("table.csv" rainbow-csv-mode)
    ("code.el" paredit-mode ws-butler-mode))
  "Files, and the modes that opening each one turns on.
`jinx--on' refuses batch Emacs, so jinx is checked by its global mode alone,
and ws-butler exempts `markdown-mode' by default.")

(defconst emx-boot-check-prose-files '("note.txt" "note.org" "note.md")
  "Files that vale must lint.")

(defconst emx-boot-check-after-load
  '((pdf-loader . (assoc pdf-loader--auto-mode-alist-item auto-mode-alist))
    (org . (and (fboundp 'org-babel-execute:gptel)
                (eq (org-src-get-lang-mode "gptel") 'text-mode)
                (not (featurep 'gptel))))
    (tempel . (featurep 'tempel-collection))
    (magit . magit-todos-mode)
    (geiser-mode . (memq 'macrostep-geiser-setup geiser-mode-hook))
    (claude-code-ide . claude-code-ide-enable-mcp-server)
    (gptel . (and (gptel-get-tool "introspection") (gptel-get-tool "Eval")
                  (gptel-get-preset 'introspect)))
    (gptel . (and (equal (gptel-backend-name gptel-backend) "Claude")
                  (eq gptel-model 'claude-opus-5-5)
                  (memq gptel-model (gptel-backend-models gptel-backend)))))
  "Features, and what must hold once each one has loaded.")

(defconst emx-boot-check-keys
  '(("C-c s" . agent-shell)
    ("C-c C" . claude-code-ide-menu)
    ("C-c q" . gptel-quick))
  "Global keys, and the commands they must reach.")

(defconst emx-boot-check-api-keys
  '(("ChatGPT" . "api.openai.com")
    ("OpenRouter" . "api.openrouter.ai")
    ("Claude" . "api.anthropic.com")
    ("Claude-thinking" . "api.anthropic.com")
    ("TogetherAI" . "api.together.ai")
    ("DeepSeek" . "api.deepseek.com")
    ("xAI" . "api.x.ai"))
  "gptel backends, and the pass entry each one reads its key from.")

(defun emx-boot-check--modules ()
  "Return a failure for each file in modules/ that init did not load.
A name in `emx-modules' that is also a library is a failure too."
  (let ((failures '()))
    (dolist (file (directory-files (expand-file-name "modules/" emx-source-dir)
                                   t "\\.el\\'"))
      (unless (assoc file load-history)
        (push (format "%s: module not loaded" (file-name-nondirectory file))
              failures)))
    (dolist (module emx-modules)
      (when-let* ((library (locate-library (symbol-name module))))
        (push (format "%s: module has the name of %s" module library) failures)))
    failures))

(defun emx-boot-check--vale (file)
  "Signal an error unless vale reports the repeated word in FILE's buffer."
  (with-current-buffer (find-buffer-visiting file)
    (let ((checker (flycheck-get-checker-for-buffer)))
      (unless (eq checker 'vale)
        (error "Flycheck chose %s" checker)))
    (flycheck-buffer)
    (while (flycheck-running-p)
      (accept-process-output nil 0.1))
    (unless flycheck-current-errors
      (error "vale reported nothing"))))

(defun emx-boot-check--triggers ()
  "Return a failure for each thing a booted Emacs has not turned on."
  (let ((dir (make-temp-file "emx-boot-check" t))
        ;; lsp-bridge would start its Python server.
        (prog-mode-hook nil)
        (failures '()))
    ;; First: anything else that uses Info initialises it, hiding a failure.
    (require 'info)
    (unless (info--manual-names nil)
      (push "info: no manual names, so gptel-agent's manual_names is empty"
            failures))
    ;; Batch Emacs exits before it reaches this hook.
    (run-hooks 'emacs-startup-hook)
    (dolist (mode emx-boot-check-global-modes)
      (unless (and (boundp mode) (symbol-value mode))
        (push (format "%s: off after boot" mode) failures)))
    (write-region "[*]\nBasedOnStyles = Vale\n" nil
                  (expand-file-name ".vale.ini" dir))
    (dolist (entry emx-boot-check-file-modes)
      (let ((file (expand-file-name (car entry) dir)))
        (write-region "This is is a repeated word.\n" nil file)
        (with-current-buffer (find-file-noselect file)
          (dolist (mode (cdr entry))
            (unless (and (boundp mode) (symbol-value mode))
              (push (format "%s: %s is off" (car entry) mode) failures))))))
    (dolist (name emx-boot-check-prose-files)
      (condition-case err
          (emx-boot-check--vale (expand-file-name name dir))
        (error (push (format "%s: %s" name (error-message-string err)) failures))))
    (dolist (entry emx-boot-check-after-load)
      (require (car entry))
      (unless (eval (cdr entry) t)
        (push (format "%s: %S does not hold" (car entry) (cdr entry)) failures)))
    failures))

(defun emx-boot-check--packages ()
  "Return the package names listed in the file $EMX_PACKAGES."
  (with-temp-buffer
    (insert-file-contents (getenv "EMX_PACKAGES"))
    (mapcar #'intern (split-string (buffer-string) "\n" t))))

(defun emx-boot-check--load (package)
  "Load PACKAGE's main feature, or locate its file if it has none."
  (when (eq package 'sly-stepper)
    ;; A SLY contrib: it requires sly-stickers, which SLY puts on load-path
    ;; when a session starts.
    (add-to-list 'load-path
                 (expand-file-name "contrib" (file-name-directory (locate-library "sly")))))
  (let ((entry (assq package emx-boot-check-features)))
    (cond ((not entry) (require package))
          ((cdr entry) (require (cdr entry)))
          ((not (locate-library (symbol-name package)))
           (error "Not on load-path")))))

(defun emx-boot-check--keys ()
  "Return a failure for each key in `emx-boot-check-keys' that misses its command."
  (let ((failures '()))
    (pcase-dolist (`(,key . ,command) emx-boot-check-keys)
      (condition-case err
          (progn
            (unless (eq (keymap-global-lookup key) command)
              (error "Bound to %S" (keymap-global-lookup key)))
            (autoload-do-load (symbol-function command) command)
            (unless (commandp command)
              (error "Not a command")))
        (error (push (format "%s: %s" key (error-message-string err)) failures))))
    failures))

(defun emx-boot-check--parinfer ()
  "Signal an error unless parinfer-rust-mode has a supported library."
  (require 'parinfer-rust-mode)
  (unless (file-exists-p parinfer-rust-library)
    (error "%s is missing" parinfer-rust-library))
  (require 'parinfer-rust parinfer-rust-library)
  (let ((version (parinfer-rust-version)))
    (unless (member version parinfer-rust-supported-versions)
      (error "Library version %s is not supported" version))))

(defun emx-boot-check--api-keys ()
  "Signal an error unless gptel reads its API keys from `pass' on demand.
Each entry must be read once a session, a failed read must be tried again, and
after `emx-pass-key-forget' the next call must read again.  With no `pass' to
run, a read must signal an error naming its entry.  A failed read's error must
carry what `pass' printed on stderr and nothing from its standard output, and a
read must work from a directory that is gone.  ChatGPT must have a key function
of its own, not `gptel-api-key', which `gptel' sets to what the user types when
a key fails.
The `pass' on PATH here logs each call to the file $EMX_PASS_LOG and prints a
key and a second line.  It warns on stderr first for the entry \"noisy\", exits
1 after a line for \"missing\", and prints nothing for \"empty\"."
  (require 'gptel)
  (when (file-exists-p (getenv "EMX_PASS_LOG"))
    (error "Loading the packages ran pass"))
  (pcase-dolist (`(,name . ,entry) emx-boot-check-api-keys)
    (let* ((gptel-backend (gptel-get-backend name))
           (key (gptel--get-api-key)))
      (unless (equal key (concat "key for " entry))
        (error "%s's key is %S" name key))))
  (let ((key (funcall (emx-pass-key "noisy"))))
    (unless (equal key "key for noisy")
      (error "The key read past pass's stderr is %S" key)))
  (dolist (entry '("missing" "empty" "missing"))
    (let ((failure (condition-case err
                       (ignore (funcall (emx-pass-key entry)))
                     (error (error-message-string err)))))
      (unless (and failure (string-search entry failure))
        (error "Pass entry %s gave %S, not an error naming it" entry failure))
      (when (equal entry "missing")
        (unless (string-search "is not in the password store" failure)
          (error "Pass entry missing gave %S, without pass's stderr" failure))
        (when (string-search "partial output" failure)
          (error "Pass entry missing gave %S, which holds pass's standard output"
                 failure)))))
  (let ((default-directory (expand-file-name "emx-boot-check-gone/"
                                             temporary-file-directory)))
    (let ((key (funcall (emx-pass-key "elsewhere"))))
      (unless (equal key "key for elsewhere")
        (error "From a directory that is gone, the key is %S" key))))
  (let ((failure (condition-case err
                     (let ((exec-path nil))
                       (ignore (funcall (emx-pass-key "unfound"))))
                   (error (error-message-string err)))))
    (unless (and failure (string-search "unfound" failure))
      (error "With no pass, entry unfound gave %S, not an error naming it" failure)))
  (pcase-dolist (`(,name . ,_) emx-boot-check-api-keys)
    (let ((gptel-backend (gptel-get-backend name)))
      (gptel--get-api-key)))
  (pcase-dolist (`(,entry . ,reads)
                 `(,@(mapcar (lambda (entry) (cons entry 1))
                             (seq-uniq (mapcar #'cdr emx-boot-check-api-keys)))
                   ("noisy" . 1) ("empty" . 1) ("missing" . 2) ("elsewhere" . 1)))
    (let ((count (emx-boot-check--pass-reads entry)))
      (unless (= count reads)
        (error "pass read %s %d times, not %d" entry count reads))))
  (emx-pass-key-forget)
  (funcall (emx-pass-key "noisy"))
  (let ((count (emx-boot-check--pass-reads "noisy")))
    (unless (= count 2)
      (error "After emx-pass-key-forget, pass read noisy %d times in all, not 2"
             count)))
  (let ((key (gptel-backend-key (gptel-get-backend "ChatGPT"))))
    (unless (and (functionp key) (not (symbolp key)))
      (error "ChatGPT's key is %S, not a function of its own" key)))
  (let* ((gptel-backend (gptel-get-backend "Claude-thinking"))
         (headers (condition-case err
                      (funcall (gptel-backend-header gptel-backend)
                               (list :backend gptel-backend))
                    (error (error "Claude-thinking's header, called with the \
request's info as gptel calls it, fails: %s" (error-message-string err))))))
    (unless (equal (alist-get "x-api-key" headers nil nil #'equal)
                   "key for api.anthropic.com")
      (error "Claude-thinking's headers are %S" headers))))

(defun emx-boot-check--pass-reads (entry)
  "Return how many times the logging `pass' was asked for ENTRY."
  (with-temp-buffer
    (insert-file-contents (getenv "EMX_PASS_LOG"))
    (how-many (concat "^show " (regexp-quote entry) "$") (point-min) (point-max))))

(defun emx-boot-check--told (buffer)
  "Return the `CLAUDE_CODE_AUTO_CONNECT_IDE' a process BUFFER starts gets."
  (let ((process-environment (buffer-local-value 'process-environment buffer)))
    (with-temp-buffer
      (call-process "sh" nil t nil "-c"
                    "printf %s \"${CLAUDE_CODE_AUTO_CONNECT_IDE-unset}\"")
      (buffer-string))))

(defun emx-boot-check--claude-code-ide ()
  "Signal an error unless only claude-code-ide's Claude is told to link to Emacs.
In a project whose .envrc direnv allows, envrc gives two files' buffers the
project's environment.  claude-code-ide, started from the first, runs
`claude-code-ide-cli-path' through eat, and the fake claude that runs must get
a true `CLAUDE_CODE_AUTO_CONNECT_IDE'.  A process either buffer starts
afterwards, and Emacs itself, must keep the value Emacs inherited."
  (let* ((dir (file-name-as-directory (make-temp-file "emx-boot-check-claude" t)))
         (bin (expand-file-name "bin" dir))
         (value (expand-file-name "value" dir))
         (claude-code-ide-terminal-backend 'eat)
         (claude-code-ide-enable-mcp-server nil)
         (inherited (or (getenv-internal "CLAUDE_CODE_AUTO_CONNECT_IDE" initial-environment)
                        "unset"))
         (buffers '())
         (session nil))
    (make-directory bin)
    (let ((claude (expand-file-name "claude" bin)))
      (with-temp-file claude
        (insert (format "#!/bin/sh\nprintf %%s \"${CLAUDE_CODE_AUTO_CONNECT_IDE-unset}\" \
> %s.new && mv %s.new %s\n"
                        value value value)))
      (set-file-modes claude #o755))
    (write-region "export EMX_BOOT_CHECK_ENVRC=1\n" nil (expand-file-name ".envrc" dir))
    ;; eat starts its process through /usr/bin/env, which the build sandbox lacks.
    (advice-add 'make-process :filter-args #'emx-boot-check--env-from-path)
    (unwind-protect
        (with-temp-buffer
          (let ((process-environment
                 (cons (concat "PATH=" bin path-separator (getenv "PATH"))
                       process-environment)))
            (unless (eq 0 (call-process "direnv" nil nil nil "allow" dir))
              (error "direnv allow failed"))
            (dolist (name '("a.txt" "b.txt"))
              (let ((buffer (find-file-noselect (expand-file-name name dir))))
                (push buffer buffers)
                (unless (equal (with-current-buffer buffer (getenv "EMX_BOOT_CHECK_ENVRC"))
                               "1")
                  (error "envrc gave %s no environment from the project's .envrc" name))))
            (setq buffers (nreverse buffers))
            (with-current-buffer (car buffers)
              (setq session (claude-code-ide--create-terminal-session
                             "*emx-boot-check claude*" dir 0 nil nil "emx-boot-check")))
            (with-timeout (10 (error "The fake claude recorded nothing in 10 s"))
              (while (not (file-exists-p value))
                (accept-process-output nil 0.1)))
            (let ((told (with-temp-buffer
                          (insert-file-contents value)
                          (buffer-string))))
              (unless (equal told "true")
                (error "claude-code-ide's Claude gets %s, not true" told)))
            (dolist (buffer buffers)
              (let ((told (emx-boot-check--told buffer)))
                (unless (equal told inherited)
                  (error "A process %s starts gets %s, not the inherited %s"
                         (buffer-name buffer) told inherited))))))
      (advice-remove 'make-process #'emx-boot-check--env-from-path)
      (when-let* ((process (cdr session)))
        (set-process-query-on-exit-flag process nil)
        (delete-process process))
      (when-let* ((buffer (car session)))
        (kill-buffer buffer))
      (mapc #'kill-buffer buffers)
      (delete-directory dir t))
    (unless (equal (or (getenv "CLAUDE_CODE_AUTO_CONNECT_IDE") "unset") inherited)
      (error "Emacs's own value is %S, not the inherited %S"
             (getenv "CLAUDE_CODE_AUTO_CONNECT_IDE") inherited))))

(defun emx-boot-check--env-from-path (args)
  "Return `make-process' ARGS, with a command of /usr/bin/env run from PATH."
  (let ((command (plist-get args :command)))
    (if (equal (car command) "/usr/bin/env")
        (plist-put (copy-sequence args) :command
                   (cons (executable-find "env") (cdr command)))
      args)))

;; Fail instead of compiling the module when it is missing.
(setq vterm-always-compile-module nil)

(let ((packages (emx-boot-check--packages))
      (loaded 0)
      (failures '()))
  (condition-case err
      (setq failures (nconc (emx-boot-check--modules)
                            (emx-boot-check--triggers)
                            (emx-boot-check--keys)
                            failures))
    (error (push (format "boot: %s" (error-message-string err)) failures)))
  (dolist (package packages)
    (condition-case err
        (progn (emx-boot-check--load package)
               (setq loaded (1+ loaded)))
      (error (push (format "%s: %s" package (error-message-string err)) failures))))
  (dolist (module '("vterm-module" "jinx-mod"))
    (unless (locate-file module load-path (list module-file-suffix))
      (push (format "%s: native module missing" module) failures)))
  (let ((epdfinfo (expand-file-name "epdfinfo" (file-name-directory (locate-library "pdf-info")))))
    (unless (file-executable-p epdfinfo)
      (push "pdf-tools: epdfinfo missing" failures)))
  (condition-case err
      (emx-boot-check--parinfer)
    (error (push (format "parinfer-rust: %s" (error-message-string err)) failures)))
  (condition-case err
      (emx-boot-check--api-keys)
    (error (push (format "API keys: %s" (error-message-string err)) failures)))
  (pcase-dolist (`(,what . ,check)
                 '(("claude-code-ide" . emx-boot-check--claude-code-ide)))
    (condition-case err
        (funcall check)
      (error (push (format "%s: %s" what (error-message-string err)) failures))))
  (dolist (tool emx-boot-check-tools)
    (unless (executable-find tool)
      (push (format "%s: not on exec-path" tool) failures)))
  (dolist (failure (if (advice-member-p #'emx-boot-check-warnings--record 'display-warning)
                       (emx-boot-check-boot-failures)
                     '("warnings: nothing recorded them, so the check's early-init did \
not load record-warnings.el")))
    (push failure failures))
  (princ (format "emx boot check: %d modules, %d of %d packages loaded\n"
                 (length emx-modules) loaded (length packages)))
  (dolist (failure (nreverse failures))
    (princ (format "FAIL %s\n" failure)))
  (kill-emacs (if failures 1 0)))

;;; boot.el ends here
