;;; completions.el --- Eagle Completion Configuration  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil

;; Adapted for emx in 2025-2026 from the following, all GPL-3.0-or-later:
;; - The Vertico README, Copyright (C) 2021-2025 Free Software Foundation, Inc.
;; - The Consult README, Copyright (C) 2021-2025 Free Software Foundation, Inc.
;; - The Embark README, Copyright (C) 2021-2025  Free Software Foundation, Inc.
;; - The Tempel README, Copyright (C) 2022-2025 Free Software Foundation, Inc.
;; - The minimal-emacs.d README by James Cherti
;;   https://github.com/jamescherti/minimal-emacs.d
;; - lem-setup-completion.el in Lambda Emacs by Colin McLear
;;   https://github.com/Lambda-Emacs/lambda-emacs

;;; Commentary:

;; This module provides enhancements to the Emacs minibuffer and completion
;; features. These include user experience enhancements and includes user
;; interface enhancements.
;;
;; The core completion UI that we are using is Vertico. This package is designed
;; to work well with a number of other complementary packages.
;;
;; *Alternatives:*
;;
;; There are various options for completion-related tasks in Emacs. A popular
;; recent one is company-mode
;;
;; 'https://utcc.utoronto.ca/~cks/space/blog/programming/EmacsUnderstandingCompletion'
;; 'https://utcc.utoronto.ca/~cks/space/blog/programming/EmacsUnderstandingOrderless'


;;; Code:

;;; Helper functions

(defun emx/sort-directories-first (files)
  "Sort directories before FILES."
  ;; Still sort by history position, length and alphabetically
  (setq files (vertico-sort-history-length-alpha files))
  ;; But then move directories first
  (nconc (seq-filter (lambda (x) (string-suffix-p "/" x)) files)
         (seq-remove (lambda (x) (string-suffix-p "/" x)) files)))

;;; Package Configuration

;; vertico.el - VERTical Interactive COmpletion
;;
;; https://github.com/minad/vertico
;;
;; Vertico has a number of extensions libraries that provide additional
;; functionality.
(use-package vertico
  :init
  (vertico-mode)
  :bind (:map vertico-map
         ("<left>" . backward-char)
         ("<right>" . forward-char))
  :config
  (setopt vertico-cycle t)
  (setopt vertico-resize t)
  (setopt vertico-count 15)
  (setopt vertico-scroll-margin 0)

  (advice-add
   'vertico--format-candidate :around
   (lambda (orig cand prefix suffix index start)
     (let ((cand (funcall orig cand prefix suffix index start)))
       (concat
        (if (= vertico--index index)
            (propertize "» " 'face 'vertico-current)
          "  ")
        cand)))))

(use-package vertico-multiform
  :after vertico
  :demand t
  :config
  (setopt vertico-multiform-commands
          `((consult-outline buffer ,(lambda (_) (text-scale-set -1)))
            (consult-imenu buffer)))
  (setopt vertico-multiform-categories
          '((consult-grep buffer)
            (jinx grid (vertico-grid-annotate . 20) (vertico-count . 4))
            (file (vertico-resize . nil)
                  (vertico-sort-function . emx/sort-directories-first))))

  (vertico-multiform-mode 1))

(use-package vertico-directory
  :after vertico
  :bind ( :map vertico-map
          ("RET" . vertico-directory-enter)
          ("DEL" . vertico-directory-delete-char)
          ("M-DEL" . vertico-directory-delete-word))
  :config
  (with-eval-after-load 'rfn-eshadow
    ;; This works with `file-name-shadow-mode' enabled.  When you are in
    ;; a sub-directory and use, say, `find-file' to go to your home '~/'
    ;; or root '/' directory, Vertico will clear the old path to keep
    ;; only your current input.
    (add-hook 'rfn-eshadow-update-overlay-hook #'vertico-directory-tidy)))


;; Here we have additional configuration to improve Emacs.
;;
;; This configuration is provided in the README for vertico.el
(use-package emacs
  :custom
  ;; Support opening new minibuffers from inside existing minibuffers.
  (enable-recursive-minibuffers t)
  ;; Hide commands in M-x which do not work in the current mode.  Vertico
  ;; commands are hidden in normal buffers. This setting is useful beyond
  ;; Vertico.
  (read-extended-command-predicate #'command-completion-default-include-p)
  :init
  ;; Add prompt indicator to `completing-read-multiple'.
  ;; We display [CRM<separator>], e.g., [CRM,] if the separator is a comma.
  (defun crm-indicator (args)
    (cons (format "[CRM%s] %s"
                  (replace-regexp-in-string
                   "\\`\\[.*?]\\*\\|\\[.*?]\\*\\'" ""
                   crm-separator)
                  (car args))
          (cdr args)))
  (advice-add #'completing-read-multiple :filter-args #'crm-indicator)

  ;; Do not allow the cursor in the minibuffer prompt
  (setq minibuffer-prompt-properties
        '(read-only t cursor-intangible t face minibuffer-prompt))
  (add-hook 'minibuffer-setup-hook #'cursor-intangible-mode))

;; orderless.el - Flexible completion styles
;;
;; https://github.com/oantolin/orderless
(use-package orderless
  :demand t
  :config
  (setopt completion-styles '(orderless basic))
  (setopt completion-category-defaults nil)
  (setopt completion-category-overrides '((file (styles basic partial-completion)))))

;; marginalia.el - Marginalia in the minibuffer
;;
;; https://github.com/minad/marginalia
;;
;; Marginalia provides annotations (marginalia) to minibuffer completion
;; candidates.
;;
;; Marginalia also integrats with Embark to offer support for actions in more
;; contexts.
;;
;; TODO Want to dynamically change the width of the marginalia annotation field.
;;      When the primary field takes up a lot of the space (for example when
;;      opening vertico-buffer, the annotation fields are hidden). The
;;      alternative is to open the imenu buffer horizontally, but then would see less
;;      context in the primary buffer. Another alternative is to reduce the font size
;;      of the margianl text, when in the vertico buffer.
(use-package marginalia
  :commands (marginalia-mode marginalia-cycle)
  :hook (after-init . marginalia-mode))

;; embark.el
;;
;; https://github.com/oantolin/embark
;;
;; Embark is an Emacs package that acts like a context menu, allowing
;; users to perform context-sensitive actions on selected items
;; directly from the completion interface.
(use-package embark
  :defer t
  :commands (embark-act
             embark-dwim
             embark-export
             embark-collect
             embark-bindings
             embark-prefix-help-command)
  :bind
  (("C-." . embark-act)                 ; pick some comfortable binding
   ("M-." . embark-dwim)                ; good alternative: C-;
   ("C-h B" . embark-bindings))         ; alternative for `describe-bindings'

  :init
  (setq prefix-help-command #'embark-prefix-help-command)

  :config
  ;; Hide the mode line of the Embark live/completions buffers
  (add-to-list 'display-buffer-alist
               '("\\`\\*Embark Collect \\(Live\\|Completions\\)\\*"
                 nil
                 (window-parameters (mode-line-format . none)))))

;; consult.el - Consulting completing-read
;;
;; https://github.com/minad/consult
;;
;; 
;; Example configuration for Consult
(use-package consult
  ;; Replace bindings. Lazily loaded by `use-package'.
  :bind (;; C-c bindings in `mode-specific-map'
         ("C-c M-x" . consult-mode-command)
         ("C-c h" . consult-history)
         ("C-c k" . consult-kmacro)
         ("C-c m" . consult-man)
         ("C-c i" . consult-info)
         ([remap Info-search] . consult-info)
         ;; C-x bindings in `ctl-x-map'
         ("C-x M-:" . consult-complex-command)     ;; orig. repeat-complex-command
         ("C-x b" . consult-buffer)                ;; orig. switch-to-buffer
         ("C-x 4 b" . consult-buffer-other-window) ;; orig. switch-to-buffer-other-window
         ("C-x 5 b" . consult-buffer-other-frame)  ;; orig. switch-to-buffer-other-frame
         ("C-x t b" . consult-buffer-other-tab)    ;; orig. switch-to-buffer-other-tab
         ("C-x r b" . consult-bookmark)            ;; orig. bookmark-jump
         ("C-x p b" . consult-project-buffer)      ;; orig. project-switch-to-buffer
         ;; Custom M-# bindings for fast register access
         ("M-#" . consult-register-load)
         ("M-'" . consult-register-store)          ;; orig. abbrev-prefix-mark (unrelated)
         ("C-M-#" . consult-register)
         ;; Other custom bindings
         ("M-y" . consult-yank-pop)                ;; orig. yank-pop
         ;; M-g bindings in `goto-map'
         ("M-g e" . consult-compile-error)
         ("M-g f" . consult-flymake)               ;; Alternative: consult-flycheck
         ("M-g g" . consult-goto-line)             ;; orig. goto-line
         ("M-g M-g" . consult-goto-line)           ;; orig. goto-line
         ("M-g o" . consult-outline)               ;; Alternative: consult-org-heading
         ("M-g m" . consult-mark)
         ("M-g k" . consult-global-mark)
         ("M-g i" . consult-imenu)
         ("M-g I" . consult-imenu-multi)
         ;; M-s bindings in `search-map'
         ("M-s d" . consult-find)                  ;; Alternative: consult-fd
         ("M-s c" . consult-locate)
         ("M-s g" . consult-grep)
         ("M-s G" . consult-git-grep)
         ("M-s r" . consult-ripgrep)
         ("M-s l" . consult-line)
         ("M-s L" . consult-line-multi)
         ("M-s k" . consult-keep-lines)
         ("M-s u" . consult-focus-lines)
         ;; Isearch integration
         ("M-s e" . consult-isearch-history)
         :map isearch-mode-map
         ("M-e" . consult-isearch-history)         ;; orig. isearch-edit-string
         ("M-s e" . consult-isearch-history)       ;; orig. isearch-edit-string
         ("M-s l" . consult-line)                  ;; needed by consult-line to detect isearch
         ("M-s L" . consult-line-multi)            ;; needed by consult-line to detect isearch
         ;; Minibuffer history
         :map minibuffer-local-map
         ("M-s" . consult-history)                 ;; orig. next-matching-history-element
         ("M-r" . consult-history))                ;; orig. previous-matching-history-element

  ;; Enable automatic preview at point in the *Completions* buffer. This is
  ;; relevant when you use the default completion UI.
  :hook (completion-list-mode . consult-preview-at-point-mode)

  ;; The :init configuration is always executed (Not lazy)
  :init

  ;; Tweak the register preview for `consult-register-load',
  ;; `consult-register-store' and the built-in commands.  This improves the
  ;; register formatting, adds thin separator lines, register sorting and hides
  ;; the window mode line.
  (advice-add #'register-preview :override #'consult-register-window)
  (setq register-preview-delay 0.5)

  ;; Use Consult to select xref locations with preview
  (setq xref-show-xrefs-function #'consult-xref
        xref-show-definitions-function #'consult-xref))

(use-package embark-consult
  :after (embark consult)
  :hook
  (embark-collect-mode . consult-preview-at-point-mode))

;; yasnippet - Snippets
;;
;; https://github.com/joaotavora/yasnippet
(use-package yasnippet
  :config
  (yas-global-mode 1))

(use-package yasnippet-snippets
  :after yasnippet)

;; tempel - Templating/snippet system
;;
;; Templating system inspired by the Tempo Emacs builtin package.
;;
;; https://github.com/minad/tempel
(use-package tempel
  :bind (("M-+" . tempel-complete) ;; Alternative tempel-expand
         ("M-*" . tempel-insert)))

;; tempel-collection
;;
;; Base snippet colletion
(use-package tempel-collection
  :after tempel
  :demand t)
