# Shiro

Shiro is a desktop overlay for live calls. It listens, answers questions while the call is still going, and can include what is on your screen. The overlay is kept out of screen capture.

## Run it

```bash
npm install
npm run tauri dev
```

On macOS the dev app relaunches as `Shiro.app` so microphone and screen recording permissions attach to Shiro, not to the terminal.

A release build is at `src-tauri/target/release/bundle/macos/shiro.app` after `npm run tauri build`.

## Staying out of sight

Shiro starts hidden. It has no Dock icon, no Cmd+Tab entry, and no menu bar icon unless you turn one on. Both windows are kept out of screen capture.

- `Ctrl+Shift+0` (or your own shortcut) opens and closes the overlay.
- **⚙** in the overlay, or opening Shiro again from Spotlight or Applications, shows the main window.
- **Settings → Menu bar icon** adds a menu bar icon if you want one.
- A release build turns on **Launch at startup** the first time it runs. Turn it off in Settings and it stays off. Dev builds never add a login item.

## First-time setup

1. Press `Ctrl+Shift+0`, press **⚙**, and choose **Settings**.
2. Pick Anthropic or OpenAI, paste the API key, and press **Save**.
3. Paste a Deepgram API key under **Call transcription**.
4. Press **Listen** in the overlay. Allow the microphone prompt.
5. For the other person's audio and for screenshots, turn on Shiro in **System Settings → Privacy & Security → Screen Recording**, then quit and open Shiro again.

If the Settings panel is taller than the window, scroll inside the panel. You do not need to fullscreen the app.

## A call

1. In the main window, fill in **Call session**: title, Meeting or Interview, and Spoken, Script, or Coding.
2. Interview mode can include a résumé. Reference notes and a text or PDF file are sent with full answers.
3. Pick the call language. That language is sent to Deepgram.
4. Open the overlay with `Ctrl+Shift+0`.
5. Pick what to hear with the button next to **Listen**: **Both** (microphone and system audio), **Mic** (microphone or an external input only), or **System** (system audio only). You can change it only while Listen is stopped.
6. Press **Listen**. Your microphone is labeled Me. System audio is labeled Them.
7. A question in the transcript is answered on its own. **Answer** replies to the latest line even when it is not a question.
8. **Call** answers a typed question from the transcript and does not take a new screenshot. **📷 Ask** captures the screen and sends that picture.
9. **🖥 On** attaches a screenshot to spoken answers. **Quick** sends only the latest line. **Full** sends the session, the résumé, the notes, and the last eight lines. **⚡ Fast** and **🧠 Smart** choose the model.
10. **Notes** writes a summary, the questions, and the next steps.
11. **Mock** asks one interview question out loud from the session. Press it again for the next question.

The call is saved on the main window under **Calls**. Open it to read the transcript, the answers, and the notes. The sidebar line is only the title and a preview.

## Screen and diagrams

**🖥 On** and **📷 Ask** both send a screenshot of the primary display, scaled so the longest edge is 1280 pixels. Shiro's own overlay is excluded from that capture. Quick mode still attaches the screenshot when **🖥 On** is selected. If the reply says no image arrived, confirm Screen Recording is enabled for this copy of Shiro and look for a warning under the answer. A new build is a new app identity, so macOS asks again.

A chart is drawn only when the question asks for one with words like "draw", "diagram", "flowchart", or "mermaid". Other answers, including system design and coding, have no chart. **Coding** style answers with a code block, plus the complexity and the edge cases. The chart is drawn after the answer finishes, so the transcript stays put. A chart that is not valid stays as text.

## If something fails

| What you see | What to do |
| --- | --- |
| Shiro is missing from the microphone list | Quit it and start it again from `Shiro.app`, then press Listen and choose Allow. |
| The other person's audio failed, or the screenshot is blank | Turn on Screen Recording for Shiro, then restart Shiro. |
| `Command answer_call not found` | An older Shiro is still open. Quit every copy and start this build. |
| Deepgram shows no audio | The Deepgram key in Settings is empty, or Listen was started before the key was saved. |
| The overlay jumps when a diagram appears | Reload the overlay. Charts now draw once, after the answer is done. |
| Settings are cut off | Scroll the settings panel. The main window scrolls the same way. |
| A saved call shows a title and no transcript | That call was saved before anyone finished a line. Listen again and let a line finish; open the call from the main window to see the transcript and answers. |

Zoom, Teams, Webex, or the Google Meet app can prompt the overlay to listen. A Meet tab inside Chrome is not detected.
