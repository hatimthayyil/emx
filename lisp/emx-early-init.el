;;; early-init.el --- Early Init -*- lexical-binding: t; -*-

;;; Variables

(defvar emx-ui-features '()
  "List of user interface features to disable in the emx Emacs setup.

This variable holds a list Emacs UI features that can be enabled:
- `context-menu`: Enables the context menu in graphical environments.
- `tool-bar`: Enables the tool bar in graphical environments.
- `menu-bar`: Enables the menu bar in graphical environments.
- `dialogs`: Enables both file dialogs and dialog boxes.
- `tooltips`: Enables tooltips.

Each feature in the list corresponds to a specific UI component that can be
turned on.")

(defvar emx-frame-title-format "%b – eagle Emacs"
  "Template for displaying the title bar of visible and iconified frame.")

(defvar emx-debug nil
  "Non-nil to enable debug.")

(defvar emx-gc-cons-threshold (* 16 1024 1024)
  "The value of `gc-cons-threshold' after Emacs startup.")


;;;  ####################################################################

;;; Garbage collection
;; Garbage collection significantly affects startup times. This setting delays
;; garbage collection during startup but will be reset later. It might also be
;; advantageous to set this as early as possible in your early-init.el file.
(setq gc-cons-threshold most-positive-fixnum)
(setq gc-cons-percentage 0.6)
(add-hook 'emacs-startup-hook
          (lambda ()
            (setq gc-cons-threshold emx-gc-cons-threshold)))

(eval-and-compile                       ; `use-package'
  (setopt use-package-enable-imenu-support t)
  (setopt use-package-verbose t)
  (require 'use-package)
  (require 'use-package-ensure)
  (setopt use-package-always-ensure nil)
  (setopt use-package-always-defer t)

  (when init-file-debug
    (setq use-package-verbose t
          use-package-expand-minimally nil
          use-package-compute-statistics t
          debug-on-error t)))

;;; Files

;; Save manual customizations to a separate file instead of cluttering `init.el'.
;; You can M-x customize, M-x customize-group, or M-x customize-themes, etc.
;; The saves you do manually using the Emacs interface would overwrite this file.
;; The following makes sure those customizations are in a separate file.
(use-package custom
  :no-require t
  :config
  ;; redirect custom-file to XDG cache
  (setq custom-file
	(concat (or (getenv "XDG_CACHE_HOME") "~/.cache")
		"/emacs/custom.el"))

  ;; only load the `custom-file' if it is not `nil'.
  (when (file-exists-p custom-file)
    (load custom-file :noerror)))

;;; Performance

;; Prefer loading newer compiled files
(setq load-prefer-newer t)

;; Font compacting can be very resource-intensive, especially when rendering
;; icon fonts on Windows. This will increase memory usage.
(setq inhibit-compacting-font-caches t)

 (unless (daemonp)
   (unless noninteractive

     ;; Without this, Emacs will try to resize itself to a specific column size
     (setq frame-inhibit-implied-resize t)

     ;; Reduce *Message* noise at startup. An empty scratch buffer (or the
     ;; dashboard) is more than enough, and faster to display.
     (setq inhibit-startup-screen t)
     ;; Suppress the vanilla startup screen completely. We've disabled it with
     ;; `inhibit-startup-screen', but it would still initialize anyway.
     (advice-add #'display-startup-screen :override #'ignore)

     ;; Ignore X resources
     ;; https://www.gnu.org/software/emacs/manual/html_node/emacs/Resources.html
     (setq inhibit-x-resources t)

     ;; Shave seconds off startup time by starting the scratch buffer in
     ;; `fundamental-mode'
     (setq initial-major-mode 'fundamental-mode
           initial-scratch-message nil)
     ))

;;; UI Elements

;; Disable startup screens and messages
(setq inhibit-splash-screen t)

;; Avoid calling `menu-bar-mode', `tool-bar-mode', and
;; `scroll-bar-mode' because manipulating frame parameters can trigger or queue
;; a superfluous and potentially expensive frame redraw at startup, depending
;; on the window system. The variables must also be set to `nil' so users don't
;; have to call the functions twice to re-enable them.
;;
;; Avoiding 
(unless (memq 'menu-bar emx-ui-features)
  (push '(menu-bar-lines . 0) default-frame-alist)
  (unless (memq window-system '(mac ns))
    (setq menu-bar-mode nil)))

(unless (memq 'tool-bar emx-ui-features)
  (push '(tool-bar-lines . 0) default-frame-alist)
  (setq tool-bar-mode nil))

(push '(vertical-scroll-bars) default-frame-alist)
(push '(horizontal-scroll-bars) default-frame-alist)
(setq scroll-bar-mode nil)
(when (fboundp 'horizontal-scroll-bar-mode)
  (horizontal-scroll-bar-mode -1))

(unless (memq 'tooltips emx-ui-features)
  (when (bound-and-true-p tooltip-mode)
    (tooltip-mode -1)))

;; Disable GUIs because they are inconsistent across systems, desktop
;; environments, and themes, and they don't match the look of Emacs.
(unless (memq 'dialogs emx-ui-features)
  (setq use-file-dialog nil)
  (setq use-dialog-box nil))

(provide 'emx-early-init)

;;; early-init.el ends here

;; Local Variables:
;; no-byte-compile: t
;; no-native-compile: t
;; no-update-autoloads: t
;; End:
