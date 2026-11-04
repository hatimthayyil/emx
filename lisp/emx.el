;;; init.el --- Init -*- no-byte-compile: t; lexical-binding: t; -*-

;;; Networking

;; Don't ping things that look like domain names.
(setq ffap-machine-p-known 'reject)

;; Some features that are not represented as packages can be found in
;; `features', but this can be inconsistent. The following enforce consistency:
(if (fboundp #'json-parse-string)
    (push 'jansson features))
(if (string-match-p "HARFBUZZ" system-configuration-features) ; no alternative
    (push 'harfbuzz features))
(if (bound-and-true-p module-file-suffix)
    (push 'dynamic-modules features))

;; no-littering
;;
;; This package updates the default locations for configuration and persistent
;; data in a number of emacs lisp packages.
;;
;; https://github.com/emacscollective/no-littering
(use-package no-littering
  :init
  (setq no-littering-etc-directory
        (expand-file-name "config/" user-emacs-directory))
  (setq no-littering-var-directory
        (expand-file-name "data/" user-emacs-directory)))

(use-package emacs
  :ensure nil
  :init                         ; Initialization settings that apply before the
                                ; package is loaded.
  (global-hl-line-mode 1)       ; Enable highlight of the current line
  (global-auto-revert-mode 1)   ; Enable global auto-revert mode to keep buffers
                                ; up to date with their corresponding files.
  (indent-tabs-mode -1)         ; Disable the use of tabs for indentation (use
                                ; spaces instead).
  (recentf-mode 1)              ; Enable tracking of recently opened files.
  (savehist-mode 1)             ; Enable saving of command history.
  (save-place-mode 1)           ; Enable saving the place in files for easier
                                ; return.
  (winner-mode 1)               ; Enable winner mode to easily undo window
                                ; configuration changes.
  (xterm-mouse-mode 1)          ; Enable mouse support in terminal mode.
  (file-name-shadow-mode 1)     ; Enable shadowing of filenames for clarity.

  ;; Set the default coding system for files to UTF-8.
  (modify-coding-system-alist 'file "" 'utf-8)

  ;; Add a hook to run code after Emacs has fully initialized.
  ;; TODO: Add bismillah
  (add-hook 'after-init-hook
            (lambda ()
              (message "Emacs has fully loaded. This code runs after startup.")

              ;; Insert a welcome message in the *scratch* buffer displaying
              ;; loading time and activated packages.
              (with-current-buffer (get-buffer-create "*scratch*")
                (insert (format
                         ";;    Welcome to EMX Emacs!
;;
;;    Loading time : %s
"
                         (emacs-init-time))))))

  ;; Auto-revert in Emacs is a feature that automatically updates the contents
  ;; of a buffer to reflect changes made to the underlying file on disk.
  (add-hook 'after-init-hook #'global-auto-revert-mode)

  ;; recentf is an Emacs package that maintains a list of recently accessed
  ;; files, making it easier to reopen files you have worked on recently.
  (add-hook 'after-init-hook #'recentf-mode)

  ;; savehist is an Emacs feature that preserves the minibuffer history between
  ;; sessions. It saves the history of inputs in the minibuffer, such as
  ;; commands, search strings, and other prompts, to a file. This allows users
  ;; to retain their minibuffer history across Emacs restarts.
  (add-hook 'after-init-hook #'savehist-mode)

  ;; save-place-mode enables Emacs to remember the last location within a file
  ;; upon reopening. This feature is particularly beneficial for resuming work
  ;; at the precise point where you previously left off.
  (add-hook 'after-init-hook #'save-place-mode)

  :config
  (setopt column-number-mode t)    ; Display the column number in the mode line.
  (setopt create-lockfiles nil)    ; Prevent the creation of lock files when
                                   ; editing.
  (setopt delete-by-moving-to-trash t) ; Move deleted files to the trash instead
                                       ; of permanently deleting them.
  (setopt global-auto-revert-non-file-buffers t) ; Automatically refresh
                                                 ; non-file buffers.
  (setopt history-length 25)         ; Set the length of the command history.
  (setopt inhibit-startup-message t) ; Disable the startup message when Emacs
                                     ; launches.
  (setopt initial-scratch-message "") ; Clear the initial message in the
                                      ; *scratch* buffer.

  (setopt pixel-scroll-precision-mode t) ; Enable precise pixel scrolling.
  (setopt pixel-scroll-precision-use-momentum nil) ; Disable momentum scrolling
                                                   ; for pixel precision.
  (setq ring-bell-function 'ignore)                ; Disable the audible bell.
  (setq use-short-answers t)          ; Use short answers in prompts for quicker
                                      ; responses (y instead of yes)

  ;; Cursor The blinking cursor is distracting and interferes with cursor
  ;; settings in some minor modes that try to change it buffer-locally (e.g.,
  ;; Treemacs).Additionally, it can cause freezing, especially on macOS, for
  ;; users with customized and colored cursors.
  (blink-cursor-mode -1)

  ;; Don't blink the paren matching the one at point, it's too distracting.
  (setq blink-matching-paren t)

  ;; Don't stretch the cursor to fit wide characters, it is disorienting,
  ;; especially for tabs.
  (setq x-stretch-cursor nil)

  ;; Reduce rendering/line scan work by not rendering cursors or regions in
  ;; non-focused windows.
  (setq-default cursor-in-non-selected-windows nil)
  (setq highlight-nonselected-windows nil)


  ;; Prefer spaces over tabs. Spaces offer a more consistent default compared to
  ;; 8-space tabs. This setting can be adjusted on a per-mode basis as needed.
  (setq-default indent-tabs-mode nil
		        tab-width 4)
  (setopt tab-always-indent 'complete) ; Make the TAB key complete text instead
                                        ; of just indenting.

  ;; Enable multi-line commenting which ensures that `comment-indent-new-line'
  ;; properly continues comments onto new lines, which is useful for writing
  ;; longer comments or docstrings that span multiple lines.
  (setq comment-multi-line t)

  ;; We often split terminals and editor windows or place them side-by-side,
  ;; making use of the additional horizontal space.
  (setq-default fill-column 80)

  ;; Disable the obsolete practice of end-of-line spacing from the typewriter
  ;; era.
  (setq sentence-end-double-space nil)

  ;; According to the POSIX, a line is defined as "a sequence of zero or
  ;; more non-newline characters followed by a terminating newline".
  (setq require-final-newline t)

  ;; Remove duplicates from the kill ring to reduce clutter
  (setq kill-do-not-save-duplicates t)

  ;; Ensures that empty lines within the commented region are also commented
  ;; out. This prevents unintended visual gaps and maintains a consistent
  ;; appearance, ensuring that comments apply uniformly to all lines, including
  ;; those that are otherwise empty.
  (setq comment-empty-lines t)

  ;; Eliminate delay before highlighting search matches
  (setq lazy-highlight-initial-delay 0)

  ;; Display the current line and column numbers in the mode line
  (setq line-number-mode t)
  (setq column-number-mode t)

  ;; Show Buffer boundaries on the left gutter
  (setq-default indicate-buffer-boundaries 'left)
  (setq-default indicate-empty-lines t)

  (setq history-length 300))

;; Frames and windows
(use-package emacs
  :ensure nil
  :demand t
  :config
  ;; Resizing the Emacs frame can be costly when changing the font. Disable this
  ;; to improve startup times with fonts larger than the system default.
  (setq frame-resize-pixelwise t)

  ;; However, do not resize windows pixelwise, as this can cause crashes in some
  ;; cases when resizing too many windows at once or rapidly.
  (setq window-resize-pixelwise nil)

  (setq resize-mini-windows 'grow-only))

(use-package treesit
  :ensure nil
  :init
  (setopt treesit-font-lock-level 4)) ; Use advanced font locking for Treesit
                                      ; mode.

;; Window management
(use-package window
  :ensure nil
  :init
  (setopt split-width-threshold 300) ; Prevent automatic window splitting if the
                                     ; window width exceeds 300 pixels.
  (setopt switch-to-buffer-obey-display-actions t) ; Make buffer switching
                                                   ; respect display actions.

  ;; Enables faster scrolling through unfontified regions. This may result in
  ;; brief periods of inaccurate syntax highlighting immediately after scrolling,
  ;; which should quickly self-correct.
  (setq fast-but-imprecise-scrolling t)
  
  ;; Move point to top/bottom of buffer before signaling a scrolling error.
  (setq scroll-error-top-bottom t)
  
  ;; Keeps screen position if the scroll command moved it vertically out of the
  ;; window.
  (setq scroll-preserve-screen-position t)

  ;; Emacs spends excessive time recentering the screen when the cursor moves
  ;; more than N lines past the window edges (where N is the value of
  ;; `scroll-conservatively`). This can be particularly slow in larger files
  ;; during extensive scrolling. If `scroll-conservatively` is set above 100,
  ;; the window is never automatically recentered. The default value of 0
  ;; triggers recentering too aggressively. Setting it to 10 reduces excessive
  ;; recentering and only recenters the window when scrolling significantly
  ;; off-screen.
  (setq scroll-conservatively 10)

  ;; Reduce cursor lag by :
  ;; 1. Prevent automatic adjustments to `window-vscroll' for long lines.
  ;; 2. Resolve the issue of random half-screen jumps during scrolling.
  (setq auto-window-vscroll nil)

  ;; Number of lines of margin at the top and bottom of a window.
  (setq scroll-margin 0)

  ;; Horizontal scrolling
  (setq hscroll-margin 2
	    hscroll-step 1))

(use-package frame
  :ensure nil
  :init
  ;; The native border "uses" a pixel of the fringe on the rightmost splits,
  ;; whereas `window-divider` does not.
  (setopt window-divider-default-bottom-width 1)
  (setopt window-divider-default-places t)
  (setopt window-divider-default-right-width 1))

(use-package mouse
  :ensure nil
  :init
  ;; Yank at point instead of at click.
  (setopt mouse-yank-at-point t))

(use-package emacs
  :hook
  (prog-mode . display-line-numbers-mode)
  :config
  (setopt display-line-numbers-type 'relative)) ; Use relative line numbering in
                                                ; programming modes.

(use-package files
  :ensure nil
  :init
  (setopt auto-save-default nil) ;; Disable automatic saving of buffers.

  ;; Avoid generating backups or lockfiles to prevent creating world-readable
  ;; copies of files.
  (setq create-lockfiles nil)
  (setopt make-backup-files nil) ;; Disable creation of backup files.

  ;; Disable the warning "X and Y are the same file". Ignoring this warning is
  ;; acceptable since it will redirect you to the existing buffer regardless.
  (setopt find-file-suppress-same-file-warnings t)

  ;; Resolve symlinks when opening files, so that any operations are conducted
  ;; from the file's true directory (like `find-file').
  (setopt find-file-visit-truename t)
  (setopt vc-follow-symlinks t)

  ;; Skip confirmation prompts when creating a new file or buffer
  (setopt confirm-nonexistent-file-or-buffer nil)

  ;; Show more of the file's path name when two buffers have the same name.
  (setopt uniquify-buffer-name-style 'forward)
  
  (setopt backup-directory-alist
	      `(("." . ,(expand-file-name
		             "emacs/backup"
		             (or (getenv "XDG_CACHE_HOME") "~/.cache")))))

  (setopt backup-by-copying-when-linked t)
  (setopt backup-by-copying t)       ; Backup by copying rather renaming
  (setopt delete-old-versions t)     ; Delete theexcess backup versions silently
  (setopt version-control t)         ; Use version numbers for backup files
  (setopt kept-new-versions 5)
  (setopt kept-old-versions 5)
  (setopt vc-make-backup-files nil)     ; Do not backup version controlled files
  (setq require-final-newline t)        ; Require empty line at EOF

  ;; Auto save
  ;;
  ;; Enable auto-save to safeguard against crashes or data loss. The
  ;; `recover-file' or `recover-session' functions can be used to restore
  ;; auto-saved data.
  (setopt auto-save-default t)

  ;; Do not auto-disable auto-save after deleting large chunks of text. The
  ;; purpose of auto-save is to provide a failsafe, and disabling it contradicts
  ;; this objective.
  (setq auto-save-include-big-deletions t)

  (setq auto-save-list-file-prefix
	    (expand-file-name "emacs/autosave/"
			              (or (getenv "XDG_CACHE_HOME") "~/.cache")))

  ;; Auto save options
  (setq kill-buffer-delete-auto-save-files t)

  ;; Auto revert
  ;;
  ;; Auto-revert in Emacs is a feature that automatically updates the contents
  ;; of a buffer to reflect changes made to the underlying file on disk.
  (setq revert-without-query (list ".")	; Do not prompt
	    auto-revert-stop-on-user-input nil
	    auto-revert-verbose t)

  ;; Revert other buffers (e.g, Dired)
  (setq global-auto-revert-non-file-buffers t)

  ;; recentf
  ;; 
  ;; `recentf' is an Emacs package that maintains a list of recently accessed
  ;; files, making it easier to reopen files you have worked on recently.
  (setq recentf-max-saved-items 300)	; default is 20
  (setq recentf-auto-cleanup 'mode)

  ;; saveplace `save-place-mode` enables Emacs to remember the last location
  ;; within a file upon reopening. This feature is particularly beneficial for
  ;; resuming work at the precise point where you previously left off.
  (setq save-place-file (expand-file-name "emacs/saveplace"
					                      (or (getenv "XDG_CACHE_HOME") "~/.cache")))
  (setq save-place-limit 600))

;; `savehist' - Persist minibuffer history over Emacs restarts.
;;
;; savehist is an Emacs feature that preserves the minibuffer history between
;; sessions. It saves the history of inputs in the minibuffer, such as commands,
;; search strings, and other prompts, to a file. This allows users to retain
;; their minibuffer history across Emacs restarts.
;;
;; This is useful for packages that may want to retain minibuffer history. For
;; eg: Vertico sorts by history position.
(use-package savehist
  :init
  (setq savehist-save-minibuffer-history t) ; Default
  (setq savehist-file (expand-file-name "emacs/history"
					                    (or (getenv "XDG_CACHE_HOME") "~/.cache")))
  (savehist-mode))

(use-package tramp
  :ensure nil
  :init
  (setopt tramp-backup-directory-alist backup-directory-alist)

  (setq tramp-auto-save-directory
	    (expand-file-name "emacs/tramp-autosave/"
			              (or (getenv "XDG_CACHE_HOME") "~/.cache"))))

(use-package compat)

;; start server
(server-start)

(provide 'emx-init)
