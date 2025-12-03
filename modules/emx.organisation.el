;;; emx-organisation.el --- Org-mode and tools to stay organised  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;; Calendar
(use-package calendar
  :ensure nil
  :commands (calendar)
  :config
  (setq calendar-time-display-form
        '( 24-hours ":" minutes
           (when time-zone (format "(%s)" time-zone))))
  (setq calendar-time-zone-style 'numeric)

  (require 'cal-dst)
  (setq calendar-standard-time-zone-name "+0000")
  (setq calendar-daylight-time-zone-name "+0100"))

;; org-mode
(use-package org
  :init
  (setq org-directory (expand-file-name "~/Documents/org"))
  (setq org-imenu-depth 7)
  :bind
  ( :map global-map
    ("C-c l" . org-store-link)
    ("C-c o" . org-open-at-point-global)
    ("C-c c" . org-capture))
  :config
  (setq org-ellipsis "⮧")
  :config
  (setq org-support-shift-select t))

(use-package org-agenda
  :init
  (setq org-agenda-files '("~/Documents/org"))
  :bind
  ( :map global-map
    ("C-c a" . org-agenda)
    ;; Easily access org-agenda. <F12> or <XF86Favorites> (same physical key on
    ;; Thinkpad P52)
    ("<XF86Favorites>" . org-agenda)
    ("<f12>" . org-agenda)

    )
  :config
  (setq org-refile-targets
        '((org-agenda-files . (:maxlevel . 2))
          (nil . (:maxlevel . 2))))
  (setq org-todo-keywords
        (quote ((sequence "TODO(t)" "NEXT(n)" "|" "DONE(d)")
                (sequence "WAITING(w@/!)" "HOLD(h@/!)" "|" "INACTIVE(i)" "CANCELLED(c@/!)" "PHONE" "MEETING"))))

  ;; Cycle through states without updating timestamps or notes
  (setq org-treat-S-cursor-todo-selection-as-state-change nil)

  (setq org-todo-state-tags-triggers
        (quote (("CANCELLED" ("CANCELLED" . t))
                ("WAITING" ("WAITING" . t))
                ("HOLD" ("WAITING") ("HOLD" . t))
                (done ("WAITING") ("HOLD"))
                ("TODO" ("WAITING") ("CANCELLED") ("HOLD"))
                ("NEXT" ("WAITING") ("CANCELLED") ("HOLD"))
                ("DONE" ("WAITING") ("CANCELLED") ("HOLD")))))

  ;; end org-agenda
  )

(use-package org-capture
  :config
  (setq org-capture-templates
        `(("c" "Capture" entry
           (file "capture.org")
           ,(concat "* %^{Title}\n"
                    ":PROPERTIES:\n"
                    ":CAPTURED: %U\n"
                    ":CUSTOM_ID: h:%(format-time-string \"%Y%m%dT%H%M%S\")\n"
                    ":END:\n"
                    "%a\n%i%?")
           :empty-lines-after 1
           :clock-in t
           :clock-resume t)

          ("q" "Note on a Quranic Ayah" entry
           (file+headline "quran.org" "Notes")
           ,(concat "* Q%^{Surah}\n"
                    "** Q%\\1:%^{Ayah}\n"
                    "*** %^{Title}\n"
                    ":PROPERTIES:\n"
                    ":CAPTURED: %U\n"
                    ":CUSTOM_ID: h:%(format-time-string \"%Y%m%dT%H%M%S\")\n"
                    ":QURAN_ID: q:%(format-time-string \"%Y%m%dT%H%M%S\")-q%\\1:%\\2\n"
                    ":END:\n\n"
                    "%?\n")
           :empty-lines-after 1)

          ("z" "Hifdh log" entry
           (file+olp+datetree "quran.org" "Memorisation log")
           ,(concat "* HIFDH J%^{Juz'} Q%^{From Surah}:%^{Ayah}-Q%^{To Surah}:%^{Ayah}\n"
                    ":PROPERTIES:\n"
                    ":CAPTURED: %U\n"
                    ":LISTENER: %^{Recited to}\n"
                    ":END:")
           :empty-lines-after 1)

          ("t" "Task to be done" entry
           (file+headline "tasks.org" "Tasks")
           ,(concat "* TODO %^{Title} %^g\n"
                    ":PROPERTIES:\n"
                    ":CAPTURED: %U\n"
                    ":CUSTOM_ID: h:%(format-time-string \"%Y%m%dT%H%M%S\")\n"
                    ":END:\n\n"
                    "%a\n%?")
           :empty-lines-after 1
           :clock-in t
           :clock-resume t)

          ("j" "Journal" entry
           (file+datetree "diary.org")
           "* %?\n%U\n"
           :clock-in t
           :clock-resume t)

          ("r" "respond" entry
           (file "capture.org")
           ,(concat "* NEXT Respond to %:from on %:subject\n"
                    "SCHEDULED: %t\n"
                    "%U\n%a\n")
           :clock-in t
           :clock-resume t
           :immediate-finish t)

          ("n" "note" entry
           (file "capture.org")
           ,(concat "* %? :NOTE:\n"
                    "%U\n%a\n")
           :clock-in t
           :clock-resume t)

          ("m" "Meeting" entry (file "~/git/org/refile.org")
           "* MEETING with %? :MEETING:\n%U" :clock-in t :clock-resume t)

          ("p" "Phone call" entry (file "~/git/org/refile.org")
           "* PHONE %? :PHONE:\n%U"
           :clock-in t
           :clock-resume t)

          ("h" "Habit" entry
           (file+headline "personal.org" "Habits")
           ,(concat "* NEXT %?\n"
                    "%U\n"
                    "%a\n"
                    "SCHEDULED: %(format-time-string \"%<<%Y-%m-%d %a .+1d/3d>>\")\n"
                    ":PROPERTIES:\n"
                    ":STYLE: habit\n"
                    ":REPEAT_TO_STATE: NEXT\n"
                    ":END:\n")
           :empty-lines-after 1)
          ))
  )
