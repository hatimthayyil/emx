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
  :if (display-graphic-p)
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
(use-package fontaine
  :if (display-graphic-p)
  :hook
  ;; Persist the latest font preset when closing/starting Emacs and
  ;; while switching between themes.
  ((after-init . fontaine-mode)
   (after-init . (lambda ()
                   ;; Set last preset or fall back to desired style from `fontaine-presets'.
                   (fontaine-set-preset (or (fontaine-restore-latest-preset) 'regular)))))
  :bind (("C-c f" . fontaine-set-preset)
         ("C-c F" . fontaine-toggle-preset))
  :config
  (setq fontaine-presets
        '((small
           :default-height 100)
          (regular
           :default-family "Iosevka Comfy Motion"
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
           :default-height 150))))

;;;; Show Font (preview fonts)
;; Read the manual: <https://protesilaos.com/emacs/show-font>
(use-package show-font
  :if (display-graphic-p)
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
