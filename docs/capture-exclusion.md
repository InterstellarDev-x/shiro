# How Shiro Is Invisible to Screen Recorders

## The Problem

Shiro is an always-on-top overlay window. Without any special handling, tools like Google Meet, OBS, Discord screen share, and the Windows Snipping Tool would capture it along with everything else on screen — leaking your AI assistant UI to everyone you're sharing with.

---

## First Principles: How Screen Capture Works on Windows

### 1. The Display Pipeline

When Windows renders your screen, it goes through several layers:

```
Your App (pixels in memory)
        ↓
DWM — Desktop Window Manager
        ↓
GPU Compositor
        ↓
Physical Monitor
```

**DWM (Desktop Window Manager)** is the Windows service responsible for compositing all windows into the final image you see on screen. It runs entirely on the GPU and manages things like transparency, blur (`backdrop-filter`), shadows, and animations.

### 2. How Screen Capture Tools Work

There are two main capture APIs on Windows:

**BitBlt (old, GDI-based)**
- Copies raw pixels from a device context
- Works at the GDI layer, below DWM
- Slow, doesn't capture hardware-accelerated content well
- Used by old tools and basic screenshot utilities

**DXGI Desktop Duplication API (modern)**
- Introduced in Windows 8
- Taps directly into DWM's composited output
- Gets the exact pixels DWM sends to the monitor
- Used by OBS, Google Meet, Discord, Teams, xcap, and all modern capture tools
- Fast, captures everything including hardware-accelerated content

Both of these ask DWM: *"give me what's on screen."*

### 3. The Key Insight

Since DWM controls everything that goes to both the monitor AND to capture tools, it's the single point where you can intercept and say:

> "Send this window's pixels to the monitor, but NOT to capture tools."

This is exactly what `SetWindowDisplayAffinity` does.

---

## The Windows API: `SetWindowDisplayAffinity`

Defined in `user32.dll`, available since Windows 7:

```c
BOOL SetWindowDisplayAffinity(
  HWND  hWnd,      // handle to the window
  DWORD dwAffinity // what to do with it
);
```

### Affinity Values

| Value | Hex | Meaning |
|-------|-----|---------|
| `WDA_NONE` | `0x00000000` | Default — show everywhere |
| `WDA_MONITOR` | `0x00000001` | Show only on monitor (old, Windows 7+) |
| `WDA_EXCLUDEFROMCAPTURE` | `0x00000011` | Exclude from all capture (Windows 10 2004+) |

`WDA_EXCLUDEFROMCAPTURE` is what we use. It instructs DWM to:
- Continue rendering the window normally to your physical display
- Replace the window's pixels with **black/transparent** in any capture output

### What "Capture Output" Means

DWM intercepts requests from:
- DXGI Desktop Duplication (OBS, Meet, Discord, xcap)
- PrintWindow API
- BitBlt on the desktop DC
- Windows Game Bar (Win+G)
- Snipping Tool / Print Screen (partially — depends on Windows version)

All of them get the black rectangle instead of your window content.

---

## How We Get the Window Handle (HWND)

Every native window on Windows has an HWND — a unique integer identifier assigned by the OS when the window is created. Tauri wraps the native Win32 window but exposes it via the `raw-window-handle` crate:

```rust
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

let handle = overlay.window_handle()?;

if let RawWindowHandle::Win32(h) = handle.as_raw() {
    let hwnd = h.hwnd.get() as isize;  // the raw Win32 HWND
    exclude_from_capture(hwnd);
}
```

`RawWindowHandle::Win32` is the variant for Windows — on macOS it would be `AppKit`, on Linux `Xlib` or `Wayland`, etc.

---

## The Rust Implementation

We skip the `windows-sys` crate and call `user32.dll` directly via raw FFI:

```rust
#[cfg(windows)]
fn exclude_from_capture(hwnd: isize) {
    #[link(name = "user32")]
    extern "system" {
        fn SetWindowDisplayAffinity(hwnd: isize, affinity: u32) -> i32;
    }
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;
    unsafe {
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    }
}
```

**Why raw FFI instead of `windows-sys`?**

`windows-sys` organizes functions behind feature flags. `SetWindowDisplayAffinity` lives under `Win32_UI_WindowsAndMessaging`, but also requires `Win32_Foundation` for the `HWND` type. Getting the right combination of features caused compile errors. Raw FFI is simpler: just declare the function signature and the linker finds `user32.dll` automatically — it's always present on Windows.

**`extern "system"`** — uses the Windows calling convention (stdcall on 32-bit, same as C on 64-bit). Required for Win32 API calls.

**`#[link(name = "user32")]`** — tells the linker to link against `user32.dll`.

**`#[cfg(windows)]`** — the entire function is compiled out on macOS and Linux. The no-op stub handles those platforms:

```rust
#[cfg(not(windows))]
fn exclude_from_capture(_hwnd: isize) {}
```

---

## Where It's Called

In Tauri's `setup()` hook, right after the app starts:

```rust
.setup(move |app| {
    // ... shortcut registration ...

    #[cfg(windows)]
    if let Some(overlay) = app.get_webview_window("overlay") {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = overlay.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                exclude_from_capture(h.hwnd.get() as isize);
            }
        }
    }

    Ok(())
})
```

This runs once at startup. The affinity flag persists for the lifetime of the window — you don't need to re-apply it.

---

## Why We Removed `capture_without_windows()`

Originally Shiro hid all its windows for 150ms before taking a screenshot, then restored them:

```
hide windows → wait 150ms → capture → show windows
```

This was necessary because xcap uses DXGI Desktop Duplication — the same pipeline that `WDA_EXCLUDEFROMCAPTURE` blocks. Once we applied the affinity flag, xcap stopped seeing the overlay entirely, making the hide/restore dance redundant.

The simplified capture is now just:

```rust
async fn do_capture() -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(capture::capture_primary_screen).await?
}
```

Benefits:
- No 150ms delay before every query
- No window flicker
- Simpler code

---

## Limitations

| Scenario | Behaviour |
|----------|-----------|
| Windows 10 2004+ | Works fully |
| Windows 10 before 2004 | `WDA_MONITOR` only (not full exclusion) |
| Windows 7/8 | `WDA_MONITOR` available but not `EXCLUDEFROMCAPTURE` |
| macOS | No equivalent public API |
| Linux | No equivalent public API |
| Physical camera pointed at monitor | Cannot be prevented — OS has no control |
| Some older capture tools (BitBlt) | May still capture depending on implementation |

---

## Summary

```
Normal window:   DWM → monitor ✓   DWM → capture tools ✓
After affinity:  DWM → monitor ✓   DWM → capture tools ✗ (black)
```

One API call at startup. No performance cost. No polling. No timers.
The OS compositor handles everything transparently after that.
