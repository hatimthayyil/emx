;;; research.el --- EmX research, references  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;;; Commentary:
;;
;; Witnesses unto them.
;;
;; I intend to use Emacs as a key tool to manage study notes, research notes,
;; references and related tasks such as organisation.
;;
;; I don't intend on restricting myself to tools that already exist. I might
;; assimilate packages into this Emacs distribution.
;;
;; Reference management, archiving research papers, manuscripts, books, etc.
;;
;; Triggers:
;;
;; 1. Philosophical, theological research
;; 2. Islamic studies, study of Quran, Arabic Grammer, Linguistics
;; 3. Politics, world human history, natural history
;; 4. Natural sciences, geometric sciences
;; 5. Mathematical sciences, algebraic sciences
;;
;; Writing:
;;
;; 1. Quick capture notes
;; 2. Curated notes, with structure, and integrated into a coherent framework
;; 3. Logical provenance (DAG) of conceptual/logical/argument frameworks
;; 4. Short-form writing (essays) about a topic
;; 5. Long-form writing (articles, large essays, book length etc)
;; 6. Study notes for a topic
;;
;; General life
;; 1. Personal notes about people I interact with in my life
;;
;; Reference management
;; 1. Drag and drop addition of references, PDFs, DOIs, etc, and organic
;;    curation of reference listings.

;;; Note-taking and personal knowledge management
;;
;; A number of packages has been developed to help manage a "knowledge base"
;; using Emacs. Some of these have good integration with reference management as
;; well.
;;
;; The options that I want to explore are:
;;
;;   1. howm
;;   2. zk
;;   3. denote
;;
;; There are others such org-roam, deft, zettledeft. However, from a cursory
;; look at them, I have decided against trying them for now.

;;; Code:

;;; Note-taking

;; howm - A note-taking framework that encourages organic note taking
;;
;; https://github.com/kaorahi/howm
;;
;; https://leahneukirchen.org/blog/archive/2022/03/note-taking-in-emacs-with-howm.html
(use-package howm
  :init
  ;; Beside the zk and Denote notes that `consult-notes' searches.
  (setq howm-directory "~/work/howm")
  (setq howm-home-directory howm-directory)
  ;; Notes are Org files, so headings and timestamps take Org's form.
  (setq howm-file-name-format "%Y-%m-%d-%H%M%S.org")
  (setq howm-view-title-header "*")
  (setq howm-dtime-format "<%Y-%m-%d %a %H:%M>")
  ;; Org keeps "C-c ," for `org-priority'.
  (setq howm-prefix (kbd "C-c ;"))
  :bind*
  ;; Org binds "C-c ;" as well, so this has to override its map.
  ("C-c ; ;" . howm-menu))

;; zk - simple zettlekasten implementation
;;
;; https://github.com/localauthor/zk
(use-package zk
  :config
  (setq zk-directory "~/work/zk")
  (setq zk-file-extension "org")
  (setq zk-file-name-separator "-")
  (zk-setup-auto-link-buttons)
  (zk-setup-embark)
  ;;zk-consult
  (setq zk-tag-search-function #'zk-consult-grep-tag-search)
  (setq zk-search-function #'zk-consult-grep))


;; pdf-tools - PDF viewer
;;
;; https://github.com/vedang/pdf-tools
;;
;; Loads when the first PDF opens.  `epdfinfo' comes from Nix, so the install
;; never needs the prompt that would stop a daemon.
(use-package pdf-tools
  :init
  (pdf-loader-install t))

;; denote - prot's note taking system
;;
;; https://protesilaos.com/emacs/denote
;;
;; The key feature of this package is it's naming scheme.
(use-package denote)

;; org-node - Alternative to org-roam
;;
;; https://github.com/meedstrom/org-node.git
(use-package org-node
  :after org
  :bind (("M-s M-f" . org-node-find)
         :map org-mode-map
         ("M-s M-i" . org-node-insert-link))
  :config
  (org-node-cache-mode)
  ;; Context buffer
  ;;(org-node-context-follow-mode)
  (org-node-backlink-mode))

;; consult-notes - Integrate various note taking systems
;;
;; https://github.com/mclear-tools/consult-notes
;;
;; This package allows for notes from various note-taking frameworks to be
;; searched together.
(use-package consult-notes
  :after (consult embark)
  :commands (consult-notes
             consult-notes-search-in-all-notes)
  :config
  (setq consult-notes-file-dir-sources
        '(("Howm"    ?h "~/work/howm/")
          ("zk"      ?z "~/work/zk/")
          ("Denote"  ?d "~/work/denote/"))))

;;; Typesetting/LaTeX-related packages

;; AUCTeX - Document creation
;;
;; https://www.gnu.org/software/auctex/
(use-package auctex)

;; Quickly insert LaTeX environments
(use-package cdlatex)

;;; Reference management

;; ebib - Bibliography management
;;
;; https://github.com/joostkremers/ebib
;;
;; https://joostkremers.github.io/ebib/
(use-package ebib)

;;; research.el ends here
