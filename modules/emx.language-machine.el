;;; emx-language-machine.el --- Language machines              -*- lexical-binding: t; -*-

;; Copyright (C) 2025  Hatim Thayyil
;; SPDX-License-Identifier: GPL-3.0-or-later

;; Author: Hatim Thayyil <hatim@thayyil.net>
;; Keywords:

;; `gptel-rename-chat' comes from the gptel wiki, which states no licence:
;; https://github.com/karthink/gptel/wiki/Defining-custom-gptel-commands

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

(defun gptel-rename-chat ()
  (interactive)
  (unless gptel-mode
    (user-error "This command is intended to be used in gptel chat buffers."))
  (let ((gptel-model 'gpt-4o-mini))
    (gptel-request
        (list nil                                    ;user
              "What is the chat content?"            ;llm
              (concat "```" (if (eq major-mode 'org-mode) "org" "markdown") "\n"
                      (buffer-substring-no-properties (point-min) (point-max))
                      "\n```"))                      ;user
      :system
      (list (format                                  ;system message
             "I will provide a transcript of a chat with an LLM.  \
Suggest a short and informative name for a file to store this chat in.  \
Use the following guidelines:
- be very concise, one very short sentence at most
- no spaces, use underscores if required
- return ONLY the title, no explanation or summary
- append the extension .%s"
             (if (eq major-mode 'org-mode) "org" "md")))
      :callback
      (lambda (resp info)                           ;callback called with response and request info
        (if (stringp resp)
            (let ((buf (plist-get info :buffer)))
              (when (and (buffer-live-p buf)
                         (y-or-n-p (format "Rename buffer %s to %s? " (buffer-name buf) resp)))
                (with-current-buffer buf (rename-visited-file resp))))
          (message "Error(%s): did not receive a response from the LLM."
                   (plist-get info :status)))))))

(provide 'emx-language-machine)
