# Shiro

A screen-to-LLM desktop assistant: a global hotkey captures the screen, sends
it to a vision LLM (Anthropic or OpenAI), and streams the answer back in an
always-on-top overlay — one that is deliberately invisible to screen
recording and sharing tools.

Built with Tauri (Rust core, React/TypeScript UI).

## Why the capture-exclusion matters

Shiro's overlay sits on top of everything else on screen. Without special
handling, any screen share (Meet, Discord, OBS) or screenshot tool would
capture the overlay along with the rest of the desktop — defeating the
point of an assistant meant to stay private to the user. On Windows, the
Rust core calls `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`
directly via the `user32` API so the OS's own compositor excludes the
window from any capture surface. The reasoning is written up in full in
[`docs/capture-exclusion.md`](docs/capture-exclusion.md), including how the
Windows display pipeline (app → DWM → GPU compositor) actually works.

## How it works

1. A global shortcut (`tauri-plugin-global-shortcut`) triggers a capture.
2. `capture.rs` grabs the primary screen off the main thread (`spawn_blocking`, so the async runtime isn't stalled by the capture call).
3. The PNG bytes go to `llm.rs`, which dispatches to whichever vision provider is configured (`AnthropicProvider` / `OpenAiProvider`) behind a shared `VisionLlm` trait, and streams the response back as `StreamChunk`s.
4. The React side renders two windows — `MainWindow` and `OverlayWindow` — plus hooks for capture queries, history, saved templates, and speech input.
5. A tray icon (`TrayIconBuilder`) keeps the app reachable without a dock/taskbar presence.

## Stack

- **Core**: Rust, Tauri 2, `tokio`, raw Win32 FFI for capture exclusion.
- **UI**: React, TypeScript, Vite.

## Running it

```bash
npm install   # or bun install
npm run tauri dev
```

Requires a vision LLM credential as an environment variable: `ANTHROPIC_API_KEY`
(preferred) or `OPENAI_API_KEY` (with an optional `OPENAI_BASE_URL` override
for OpenAI-compatible endpoints).

## Status

Capture exclusion and the vision LLM round-trip both work end to end on
Windows; this is a personal tool, not yet packaged for distribution or
tested on macOS/Linux capture APIs.
