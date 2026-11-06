;;; ai.el --- AI tools  -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;; The gptel backends were adapted for emx in 2025-2026 from gptel's README,
;; GPL-3.0-or-later: https://github.com/karthink/gptel
;; Copyright (C) 2023-2025  Karthik Chikmagalur
;;
;; The `ellama' settings were adapted for emx in 2025-2026 from ellama's README,
;; GPL-3.0-or-later: https://github.com/s-kostyaev/ellama
;; Copyright (C) 2023-2025  Free Software Foundation, Inc.

;;; Commentary:

;;

(defun key (pass-entry)
  (string-trim
   (shell-command-to-string (concat "pass " pass-entry))))

(use-package gptel
  :defer t
  :bind (("C-c RET" . gptel-send))
  :config
  ;;;
  ;;; ================ Language models
  ;;;

  ;; OpenAI
  (setq gptel-api-key (key "api.openai.com"))
  (gptel-make-openai "ChatGPT"
    :stream t
    :key 'gptel-api-key)

  ;; OpenRouter
  (gptel-make-openai "OpenRouter"
    :host "openrouter.ai"
    :endpoint "/api/v1/chat/completions"
    :stream t
    :key (key "api.openrouter.ai")
    :models '(openai/gpt-3.5-turbo
              google/gemini-pro))

  ;; Claude
  (gptel-make-anthropic "Claude"
    :stream t
    :key (key "api.anthropic.com"))
  (gptel-make-anthropic "Claude-thinking"
    :key (key "api.anthropic.com")
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
    :key (key "api.together.ai")
    :stream t
    :models '(deepseek-ai/DeepSeek-R1
              mistralai/Mixtral-8x7B-Instruct-v0.1
              codellama/CodeLlama-13b-Instruct-hf
              codellama/CodeLlama-34b-Instruct-hf))

  (gptel-make-openai "DeepSeek"
    :host "api.deepseek.com"
    :endpoint "/chat/completions"
    :stream t
    :key (key "api.deepseek.com")
    :models '(deepseek-chat deepseek-coder deepseek-reasoner))

  (gptel-make-openai "xAI"
    :host "api.x.ai"
    :key (key "api.x.ai")
    :endpoint "/v1/chat/completions"
    :stream t
    :models '(;; xAI now only offers `grok-beta` as of the time of this writing
              grok-beta)))

;; ellama - Tool for interacting with LLMs
(use-package ellama
  :bind ("C-c e" . ellama)
  ;; send last message in chat buffer with C-c C-c
  :hook (org-ctrl-c-ctrl-c-final . ellama-chat-send-last-message)
  :init
  (require 'llm-ollama)
  (setopt ellama-summarization-provider
          (make-llm-ollama
           :chat-model "qwen2.5:3b"
	       :embedding-model "nomic-embed-text"
           :default-chat-non-standard-params '(("num_ctx" . 32768))))
  (setopt ellama-coding-provider
          (make-llm-ollama
           :chat-model "qwen2.5-coder:3b"
           :embedding-model "nomic-embed-text"
           :default-chat-non-standard-params '(("num_ctx" . 32768)))))


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
