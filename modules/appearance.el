;;; appearance.el --- UI enhancements and style  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;; Adapted for emx in 2025-2026 from Prot's Emacs configuration,
;; GPL-3.0-or-later: https://github.com/protesilaos/dotfiles
;; Copyright (C) 2020-2025  Protesilaos Stavrou

;;; Commentary:

;;

;;; Code:

;; The themes are highly customisable.  Read the manual:
;; <https://protesilaos.com/emacs/modus-themes>.
(use-package modus-themes
  :demand t
  :bind ("<f5>" . modus-themes-rotate)
  :config
  (setopt modus-themes-custom-auto-reload nil)
  (setq modus-themes-to-rotate '(modus-operandi
                                   modus-operandi-tinted
                                   modus-vivendi
                                   modus-vivendi-tinted))
  ;; Needed to maintain spacing sensitive constructs (org-tables)
  (setopt modus-themes-mixed-fonts t
          ;; modus-themes-variable-pitch-ui t
          modus-themes-italic-constructs nil
          modus-themes-bold-constructs nil
          modus-themes-completions '((t . (extrabold)))
          modus-themes-prompts '(extrabold))

  ;; Load the theme
  (modus-themes-load-theme 'modus-operandi-tinted))

(use-package ef-themes
  :bind
  (("M-<f5>" . ef-themes-rotate)
   ("C-<f5>" . ef-themes-select))
  :config
  ;; Why is this not ef-themes-to-rotate
  (setopt ef-themes-rotate ef-themes-items)
  (setopt ef-themes-mixed-fonts t))

;; Lin
;; Read the lin manual: <https://protesilaos.com/emacs/lin>.
(use-package lin
  :hook (after-init . lin-global-mode) ; applies to all `lin-mode-hooks'
  :config
  ;; You can use this to live update the face:
  ;;
  ;; (customize-set-variable 'lin-face 'lin-green)
  ;;
  ;; Or `setopt' on Emacs 29: (setopt lin-face 'lin-yellow)
  ;;
  ;; I still prefer `setq' for consistency.
  (setq lin-face 'lin-cyan))

;; spacious-padding
(use-package spacious-padding
  :hook (after-init . spacious-padding-mode)
  :bind ("<f8>" . spacious-padding-mode)
  :init
  ;; These are the defaults, but I keep it here for visiibility.
  (setq spacious-padding-widths
        '( :internal-border-width 15
           :header-line-width 4
           :mode-line-width 4
           :tab-width 4
           :right-divider-width 1
           :scroll-bar-width 8
           :left-fringe-width 10
           :right-fringe-width 10))

  ;; (setq spacious-padding-subtle-mode-line
  ;;       `( :mode-line-active ,(if (or (eq prot-emacs-load-theme-family 'modus)
  ;;                                     (eq prot-emacs-load-theme-family 'standard))
  ;;                                 'default
  ;;                               'help-key-binding)
  ;;          :mode-line-inactive window-divider))

  ;; Read the doc string of `spacious-padding-subtle-mode-line' as
  ;; it is very flexible.
  (setq spacious-padding-subtle-mode-line nil))

;; fontaine.el -- Font configurations
(defun emx-fontaine-reapply-preset (theme)
  "Set the current Fontaine preset again once THEME is enabled.
Fontaine keeps its faces in a theme of its own.  For a face attribute that
two themes both set, such as the weight of `bold', the theme whose settings
were applied last wins, so loading or rotating a theme, or reloading emx,
would otherwise drop the preset's attributes."
  (when (and (bound-and-true-p fontaine-current-preset)
             (not (memq theme '(user fontaine))))
    (fontaine-set-preset fontaine-current-preset)))

(use-package fontaine
  :hook
  ;; Persist the latest font preset across restarts.
  ((after-init . fontaine-mode)
   (after-init . (lambda ()
                   ;; Set last preset or fall back to desired style from `fontaine-presets'.
                   ;; Fontaine sets fonts for a daemon's later frames, but
                   ;; can't in a terminal, where it only warns.
                   (when (or (daemonp) (display-graphic-p))
                     (fontaine-set-preset (or (fontaine-restore-latest-preset) 'regular))))))
  :bind (("C-c f" . fontaine-set-preset)
         ("C-c F" . fontaine-toggle-preset))
  :config
  (add-hook 'enable-theme-functions #'emx-fontaine-reapply-preset)
  ;; Every preset falls back to t, which Fontaine uses instead of its generic
  ;; Monospace and Sans families.  Its fixed-pitch families keep monospaced
  ;; text, such as Org tables and code, monospaced under the quran preset too,
  ;; whose Amiri isn't.  emx names no proportional family, so variable-pitch
  ;; keeps Emacs's own.
  (setq fontaine-presets
        '((small
           :default-height 100)
          (regular
           :default-height 110
           :bold-weight extrabold)
          (medium
           :inherit regular
           :default-height 130)
          (large
           :inherit regular
           :default-height 150)
          (quran
           :default-family "Amiri Quran Colored"
           :default-height 150)
          (t
           :default-family "Iosevka Comfy Motion"
           :fixed-pitch-family "Iosevka Comfy Motion"
           :fixed-pitch-serif-family "Iosevka Comfy Motion"))))

;;;; Show Font (preview fonts)
;; Read the manual: <https://protesilaos.com/emacs/show-font>
(use-package show-font
  :commands (show-font-select-preview show-font-list)
  :config
  ;; These are the defaults, but I keep them here for easier access.
  (setq show-font-pangram 'fox)
  (setq show-font-character-sample
        "
ABCDEFGHIJKLMNOPQRSTUVWXYZ
abcdefghijklmnopqrstuvwxyz
0123456789   !@#$¢%^&*~|
`'\"‘’“”.,;:  ()[]{}—-_+=<>

()[]{}<>«»‹› 6bB8&0ODdoa 1tiIlL|\/
!ij c¢ 5$Ss 7Z2z 9gqp nmMNNMW uvvwWuuw
x×X .,·°;:¡!¿?`'‘’   ÄAÃÀ TODO
"))

;; rainbow-csv - Highlight CSV and TSV spreadsheets
;;
;; https://github.com/emacs-vs/rainbow-csv
(use-package rainbow-csv
  :hook ((csv-mode tsv-mode) . rainbow-csv-mode))
