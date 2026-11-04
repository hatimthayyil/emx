;;; boot.el --- Check that emx boots and loads every package  -*- lexical-binding: t -*-

;;; Commentary:

;; Loaded after emx's init by nix/checks/default.nix.  Fails unless init
;; loaded without error, the main feature of every package named in the file
;; $EMX_PACKAGES loads without an error from its configuration, the native
;; parts are in place, and Emacs finds the tools its modules call.  The build
;; sandbox has none of them, so they can only come from Emacs's wrapper.

;;; Code:

(defconst emx-boot-check-features
  '((auctex . tex)
    (git-modes . gitconfig-mode)
    (poet-theme . nil))
  "Packages whose main feature is not named after them.
A nil feature means the package is a theme: its file is only located.")

(defconst emx-boot-check-tools
  '("rg" "fd" "yt-dlp" "vale" "vale-ls" "basedpyright" "ruff"
    "beancount-language-server")
  "Programs that Emacs's wrapper puts on PATH.")

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

;; Fail instead of compiling the module when it is missing.
(setq vterm-always-compile-module nil)

(let ((packages (emx-boot-check--packages))
      (loaded 0)
      (failures '()))
  (when init-file-had-error
    (push "init: loading the init files signalled an error" failures))
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
  (dolist (tool emx-boot-check-tools)
    (unless (executable-find tool)
      (push (format "%s: not on exec-path" tool) failures)))
  ;; use-package reports an error in a :config block as a warning.
  (when-let* ((warnings (get-buffer "*Warnings*")))
    (with-current-buffer warnings
      (goto-char (point-min))
      (while (re-search-forward "^\\(?:Error\\|Emergency\\) .*$" nil t)
        (push (match-string 0) failures))))
  (princ (format "emx boot check: %d modules, %d of %d packages loaded\n"
                 (length emx-modules-enabled) loaded (length packages)))
  (dolist (failure (nreverse failures))
    (princ (format "FAIL %s\n" failure)))
  (kill-emacs (if failures 1 0)))

;;; boot.el ends here
