# Speech to text in the chat composer

Status: implemented on 2026-10-31; native microphone runtime check remains.

## Goal

A microphone button beside the send button in the chat composer. Press it, speak, press it again: the spoken words
are typed into the prompt at the caret, where the user can edit them before sending.

## Current state

- No voice or speech code exists in the app (checked: no `SpeechRecognition`, `getUserMedia`, `MediaRecorder`, or
  Whisper code in `apps/desktop/src` or `apps/desktop/src-tauri`).
- The composer text is a contenteditable; inserting at the caret goes through `replaceEditorRange` in
  `apps/desktop/src/lib/components/chat/composer-editor.ts`.

## Options for the speech engine

| Option | Pros | Cons |
| --- | --- | --- |
| A. Web Speech API (`SpeechRecognition`) in the WebView | No backend work | Unreliable in WebView2 on Windows; needs an online speech service; not testable here |
| B. Local Whisper (`whisper-rs` in `src-tauri`) | Works offline; audio stays on the machine | Model download on first use (about 150 MB for `base`); CPU cost while transcribing |
| C. Cloud transcription through an existing provider key (for example OpenAI Whisper API) | Small code; no model file | Audio leaves the machine; needs a key and network; a provider that has no audio endpoint cannot be used |

Recommendation: B, with C as an optional setting later.

## Plan (option B)

1. **Capture (frontend).** `navigator.mediaDevices.getUserMedia({ audio: true })` and `MediaRecorder` in a new
   `apps/desktop/src/lib/chat/voice.svelte.ts` store (`idle | recording | transcribing`). Mic permission goes in the Tauri
   capability file if WebView2 asks for it.
2. **Transcribe (Rust).** New command in `apps/desktop/src-tauri/src/voice.rs`: decode the recorded audio to 16 kHz mono
   PCM, run `whisper-rs`, return the text. Errors use `thiserror`/`anyhow` per `AGENTS.md`; no `unwrap()` outside tests.
3. **Model.** Download the `base` model to the app data folder on first use, with a progress line in the button's tooltip.
   Never download inside tests (`AGENTS.md`).
4. **UI.** Mic button left of the send button in `Composer.svelte`. Use `ZPopover`/`TRIGGER_ON` only if a popover is added.
   While recording, the button shows the active state (`TRIGGER_ON`) and a timer; Escape cancels.
5. **Insert.** Put the text at the caret with `replaceEditorRange` (same path as Shift+Enter), then `setDraft(...)`.
   Do not send automatically.
6. **Tests.** Unit test for the PCM conversion and the store state machine. Manual check in the desktop app (the
   Preview web page cannot reach Tauri commands).

## Decisions used by the implementation

- Local Whisper is the default; recorded audio does not leave the machine.
- The multilingual `base` model auto-detects Vietnamese and English.
- Recording is a toggle: press once to start and again to transcribe; Escape cancels.
- WebView2 decodes the MediaRecorder container to interleaved float PCM. Rust owns downmixing, 16 kHz resampling,
  model download, and Whisper inference, avoiding a second native Opus decoder.
