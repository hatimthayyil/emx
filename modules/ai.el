;;; ai.el --- AI tools  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;; The gptel backends were adapted for emx in 2025-2026 from gptel's README,
;; GPL-3.0-or-later: https://github.com/karthink/gptel
;; Copyright (C) 2023-2025  Karthik Chikmagalur

;;; Commentary:

;; Each gptel backend reads its API key from pass through `emx-pass-key', once
;; a session.  After changing a key in pass, run `emx-pass-key-forget' so the
;; next request reads it again.

(defvar emx-pass-key--cache (make-hash-table :test #'equal)
  "The API keys read from pass this session, by pass entry.
Inspecting this variable shows the keys.")

(defun emx-pass-key (entry)
  "Return a function that gives the API key in pass ENTRY, for gptel's `:key'.
The function's first call reads the key with `pass', and later calls return
it, so a session runs `pass' once per entry and never when gptel loads.  A read
that fails signals an error naming ENTRY and caches nothing, so the next call
reads again.  `emx-pass-key-forget' drops the keys read so far."
  (lambda ()
    (or (gethash entry emx-pass-key--cache)
        (puthash entry (emx-pass-key--read entry) emx-pass-key--cache))))

(defun emx-pass-key--read (entry)
  "Return the API key in pass ENTRY: the first line `pass show' prints.
Its error stream is discarded, so a gpg warning can't become the key.  No
`pass' on `exec-path', a non-zero exit or an empty key signals an error naming
ENTRY, and that error holds nothing `pass' printed."
  (with-temp-buffer
    (let ((status (condition-case nil
                      (call-process "pass" nil '(t nil) nil "show" entry)
                    (file-missing
                     (error "pass show %s gave no key: pass isn't on exec-path" entry)))))
      (goto-char (point-min))
      (let ((key (buffer-substring-no-properties (point) (line-end-position))))
        (unless (and (eql status 0) (not (string-empty-p key)))
          (error "pass show %s gave no key (exit status %s)" entry status))
        key))))

(defun emx-pass-key-forget ()
  "Forget the API keys read from pass, so each is read again when next needed.
Run it after changing a key in pass."
  (interactive)
  (clrhash emx-pass-key--cache))

(use-package gptel
  :defer t
  :bind (("C-c RET" . gptel-send))
  :custom
  (gptel-default-mode 'org-mode)
  (gptel-prompt-prefix-alist
   (mapcar (lambda (mode) (cons mode (format "%s ›  " (user-login-name))))
           '(markdown-mode org-mode text-mode)))
  (gptel-response-prefix-alist
   (mapcar (lambda (mode) (cons mode "language machine  "))
           '(markdown-mode org-mode text-mode)))
  :config
  ;;;
  ;;; ================ Language models
  ;;;

  ;; OpenAI
  (setq gptel-api-key (emx-pass-key "api.openai.com"))
  (gptel-make-openai "ChatGPT"
    :stream t
    :key 'gptel-api-key)

  ;; OpenRouter
  (gptel-make-openai "OpenRouter"
    :host "openrouter.ai"
    :endpoint "/api/v1/chat/completions"
    :stream t
    :key (emx-pass-key "api.openrouter.ai")
    :models '(openai/gpt-3.5-turbo
              google/gemini-pro))

  ;; Claude
  (gptel-make-anthropic "Claude"
    :stream t
    :key (emx-pass-key "api.anthropic.com"))
  (gptel-make-anthropic "Claude-thinking"
    :key (emx-pass-key "api.anthropic.com")
    :stream t
    :models '(claude-3-7-sonnet-20250219)
    :header (lambda () (when-let* ((key (gptel--get-api-key)))
                         `(("x-api-key" . ,key)
                           ("anthropic-version" . "2023-06-01")
                           ("anthropic-beta" . "pdfs-2024-09-25")
                           ("anthropic-beta" . "output-128k-2025-02-19")
                           ("anthropic-beta" . "prompt-caching-2024-07-31"))))
    :request-params '(:thinking (:type "enabled" :budget_tokens 2048)
                                :max_tokens 4096))

  ;; TogetherAI
  (gptel-make-openai "TogetherAI"
    :host "api.together.xyz"
    :key (emx-pass-key "api.together.ai")
    :stream t
    :models '(deepseek-ai/DeepSeek-R1
              mistralai/Mixtral-8x7B-Instruct-v0.1
              codellama/CodeLlama-13b-Instruct-hf
              codellama/CodeLlama-34b-Instruct-hf))

  (gptel-make-openai "DeepSeek"
    :host "api.deepseek.com"
    :endpoint "/chat/completions"
    :stream t
    :key (emx-pass-key "api.deepseek.com")
    :models '(deepseek-chat deepseek-coder deepseek-reasoner))

  (gptel-make-openai "xAI"
    :host "api.x.ai"
    :key (emx-pass-key "api.x.ai")
    :endpoint "/v1/chat/completions"
    :stream t
    :models '(;; xAI now only offers `grok-beta` as of the time of this writing
              grok-beta))

  ;; New chats.
  (setq gptel-backend (gptel-get-backend "Claude")
        gptel-model 'claude-opus-5-5)

  ;;;
  ;;; ================ Chat buffers
  ;;;

  (add-hook 'gptel-post-stream-hook #'gptel-auto-scroll)
  (add-hook 'gptel-post-response-functions #'gptel-end-of-response)
  (add-to-list 'gptel-directives '(arabic . "Reply in plain Classical Arabic.") t)

  ;;;
  ;;; ================ Presets
  ;;;

  ;; `gptel-tools' stays empty, so a chat sends tools only under a preset.
  (gptel-make-preset 'introspect
    :description "Let the model read this Emacs: symbols, source and manuals"
    :tools '("introspection" "Eval")))

;; gptel-agent's tools, without its agent.  Loading them only registers them.
(use-package gptel-agent-tools
  :after gptel
  :demand t)

(use-package gptel-agent-tools-introspection
  :after gptel
  :demand t)

;; `manual_names' reads `Info-directory-list', which is empty until this runs.
(with-eval-after-load 'info
  (info-initialize))

;; Explains the region, a pdf-tools selection or the sexp at point in a line.
(use-package gptel-quick
  :bind ("C-c q" . gptel-quick))

;; LLM calls as Org source blocks.  ob-gptel loads gptel, so it waits for the
;; first block that runs.
(use-package ob-gptel
  :autoload org-babel-execute:gptel
  :init
  ;; Org reads a block's defaults before it calls the autoload.
  (defvar org-babel-default-header-args:gptel
    '((:results . "replace")
      (:exports . "both")
      (:eval . "never-export")
      (:format . "org"))
    "Default header arguments for gptel source blocks.")
  ;; Otherwise fontifying a block runs `gptel-mode', which loads gptel.
  (with-eval-after-load 'org-src
    (add-to-list 'org-src-lang-modes '("gptel" . text))))

;; Drives agents over ACP.  Claude Code runs through `claude-agent-acp', signed
;; in with the machine's Claude Code login.
(use-package agent-shell
  :bind ("C-c s" . agent-shell)
  :custom
  (agent-shell-preferred-agent-config 'claude-code))

;; Runs Claude Code's own terminal interface and links it to Emacs.
(use-package claude-code-ide
  :bind ("C-c C" . claude-code-ide-menu)
  :custom
  (claude-code-ide-terminal-backend 'eat)
  (claude-code-ide-enable-execute-code nil)
  :config
  ;; A false value in the environment beats `--ide' and leaves Emacs unlinked.
  (setenv "CLAUDE_CODE_AUTO_CONNECT_IDE" "true")
  (claude-code-ide-emacs-tools-setup))


;;; Functions

(defvar gptel-backend)
(defvar gptel-mode)
(defvar gptel-model)

(defun emx/gptel-rename-chat ()
  "Rename this gptel chat's file, starting from a cheap model's suggestion.
The suggested name can be edited first, or declined with \\[keyboard-quit]."
  (interactive)
  (unless (bound-and-true-p gptel-mode)
    (user-error "Not a gptel chat buffer"))
  (let ((chat (current-buffer))
        (extension (if (derived-mode-p 'org-mode) "org" "md"))
        (gptel-backend (gptel-get-backend "ChatGPT"))
        (gptel-model 'gpt-4o-mini))
    (gptel-request (buffer-substring-no-properties (point-min) (point-max))
      :system "You name files. The message holds a saved conversation with \
an AI assistant; do not answer it. Reply with a file name and nothing else: \
two to six lowercase words joined by hyphens, with no extension."
      :callback
      (lambda (name info)
        (cond ((not (stringp name))
               (message "No chat name from the model: %s"
                        (plist-get info :status)))
              ((buffer-live-p chat)
               (with-current-buffer chat
                 (rename-visited-file
                  (read-string "Rename chat to: "
                               (file-name-with-extension (string-trim name)
                                                         extension))))))))))
