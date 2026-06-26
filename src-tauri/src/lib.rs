mod capture;
mod llm;

use base64::Engine as _;
use llm::{AnthropicProvider, OpenAiProvider, StreamChunk, VisionLlm};
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Capture exclusion (Windows only)
// ---------------------------------------------------------------------------

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

#[cfg(not(windows))]
fn exclude_from_capture(_hwnd: isize) {}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub llm: Arc<dyn VisionLlm>,
    pub last_screenshot: Option<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn do_capture() -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(capture::capture_primary_screen)
        .await
        .map_err(|e| format!("Capture task panicked: {e}"))?
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

async fn run_query(
    png_bytes: Vec<u8>,
    prompt: String,
    channel: tauri::ipc::Channel<StreamChunk>,
    llm: Arc<dyn VisionLlm>,
) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamChunk>(64);
    let llm_task = tokio::spawn(async move { llm.query(&png_bytes, &prompt, tx).await });
    while let Some(chunk) = rx.recv().await {
        channel.send(chunk).map_err(|e| format!("Channel send failed: {e}"))?;
    }
    llm_task.await.map_err(|e| format!("LLM task panicked: {e}"))??;
    Ok(())
}

/// Capture fresh screenshot, cache it, then query.
#[tauri::command]
async fn capture_and_query(
    prompt: String,
    channel: tauri::ipc::Channel<StreamChunk>,
    state: State<'_, Mutex<AppState>>,
    _app: tauri::AppHandle,
) -> Result<(), String> {
    let png_bytes = do_capture().await?;
    let llm = {
        let mut s = state.lock().await;
        s.last_screenshot = Some(png_bytes.clone());
        s.llm.clone()
    };
    run_query(png_bytes, prompt, channel, llm).await
}

/// Re-use the last cached screenshot — no new capture.
#[tauri::command]
async fn query_cached(
    prompt: String,
    channel: tauri::ipc::Channel<StreamChunk>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let (png_bytes, llm) = {
        let s = state.lock().await;
        let bytes = s.last_screenshot.clone().ok_or("No screenshot cached yet — ask once first")?;
        (bytes, s.llm.clone())
    };
    run_query(png_bytes, prompt, channel, llm).await
}


#[tauri::command]
async fn set_autostart(
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
async fn debug_capture(_app: tauri::AppHandle) -> Result<String, String> {
    let png_bytes = do_capture().await?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok(format!("data:image/png;base64,{b64}"))
}

#[tauri::command]
async fn set_provider(
    provider: String,
    api_key: String,
    model: Option<String>,
    base_url: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let new_llm: Arc<dyn VisionLlm> = match provider.as_str() {
        "anthropic" => {
            let mut p = AnthropicProvider::new(api_key);
            if let Some(m) = model { p = p.with_model(m); }
            Arc::new(p)
        }
        "openai" => {
            let mut p = OpenAiProvider::new(api_key);
            if let Some(m) = model { p = p.with_model(m); }
            if let Some(url) = base_url {
                if !url.is_empty() { p = p.with_base_url(url); }
            }
            Arc::new(p)
        }
        other => return Err(format!("Unknown provider: {other}")),
    };
    state.lock().await.llm = new_llm;
    Ok(())
}

#[tauri::command]
async fn set_hotkey(
    shortcut_str: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let new_shortcut = parse_shortcut(&shortcut_str)
        .ok_or_else(|| format!("Invalid shortcut: {shortcut_str}"))?;

    // Unregister all existing shortcuts then register the new one
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| format!("Unregister failed: {e}"))?;

    let handle = app.clone();
    gs.on_shortcut(new_shortcut, move |_app, _shortcut, event| {
        if event.state() != ShortcutState::Pressed { return; }
        if let Some(overlay) = handle.get_webview_window("overlay") {
            let visible = overlay.is_visible().unwrap_or(false);
            if visible { let _ = overlay.hide(); }
            else { let _ = overlay.show(); let _ = overlay.set_focus(); }
        }
    }).map_err(|e| format!("Register failed: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Shortcut parser  "Ctrl+Shift+0" → Shortcut
// ---------------------------------------------------------------------------

fn parse_shortcut(s: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in &parts {
        match *part {
            "Ctrl" | "Control" => mods |= Modifiers::CONTROL,
            "Shift" => mods |= Modifiers::SHIFT,
            "Alt" => mods |= Modifiers::ALT,
            "Meta" | "Super" | "Win" => mods |= Modifiers::META,
            key => {
                code = match key {
                    "0" => Some(Code::Digit0), "1" => Some(Code::Digit1),
                    "2" => Some(Code::Digit2), "3" => Some(Code::Digit3),
                    "4" => Some(Code::Digit4), "5" => Some(Code::Digit5),
                    "6" => Some(Code::Digit6), "7" => Some(Code::Digit7),
                    "8" => Some(Code::Digit8), "9" => Some(Code::Digit9),
                    "A" => Some(Code::KeyA), "B" => Some(Code::KeyB),
                    "C" => Some(Code::KeyC), "D" => Some(Code::KeyD),
                    "E" => Some(Code::KeyE), "F" => Some(Code::KeyF),
                    "G" => Some(Code::KeyG), "H" => Some(Code::KeyH),
                    "I" => Some(Code::KeyI), "J" => Some(Code::KeyJ),
                    "K" => Some(Code::KeyK), "L" => Some(Code::KeyL),
                    "M" => Some(Code::KeyM), "N" => Some(Code::KeyN),
                    "O" => Some(Code::KeyO), "P" => Some(Code::KeyP),
                    "Q" => Some(Code::KeyQ), "R" => Some(Code::KeyR),
                    "S" => Some(Code::KeyS), "T" => Some(Code::KeyT),
                    "U" => Some(Code::KeyU), "V" => Some(Code::KeyV),
                    "W" => Some(Code::KeyW), "X" => Some(Code::KeyX),
                    "Y" => Some(Code::KeyY), "Z" => Some(Code::KeyZ),
                    "F1"  => Some(Code::F1),  "F2"  => Some(Code::F2),
                    "F3"  => Some(Code::F3),  "F4"  => Some(Code::F4),
                    "F5"  => Some(Code::F5),  "F6"  => Some(Code::F6),
                    "F7"  => Some(Code::F7),  "F8"  => Some(Code::F8),
                    "F9"  => Some(Code::F9),  "F10" => Some(Code::F10),
                    "F11" => Some(Code::F11), "F12" => Some(Code::F12),
                    "Space" => Some(Code::Space),
                    "Tab"   => Some(Code::Tab),
                    _ => None,
                };
            }
        }
    }

    code.map(|c| Shortcut::new(if mods.is_empty() { None } else { Some(mods) }, c))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let llm: Arc<dyn VisionLlm> = if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        Arc::new(AnthropicProvider::new(key))
    } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        let mut p = OpenAiProvider::new(key);
        if let Ok(url) = std::env::var("OPENAI_BASE_URL") {
            if !url.is_empty() { p = p.with_base_url(url); }
        }
        Arc::new(p)
    } else {
        Arc::new(AnthropicProvider::new(""))
    };

    let state = Mutex::new(AppState { llm, last_screenshot: None });

    let default_hotkey = "Ctrl+Shift+0";
    let shortcut = parse_shortcut(default_hotkey).expect("default shortcut must parse");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            capture_and_query,
            query_cached,
            debug_capture,
            set_provider,
            set_hotkey,
            set_autostart,
            get_autostart,
        ])
        .setup(move |app| {
            // Register hotkey
            let handle = app.handle().clone();
            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state() != ShortcutState::Pressed { return; }
                if let Some(overlay) = handle.get_webview_window("overlay") {
                    let visible = overlay.is_visible().unwrap_or(false);
                    if visible { let _ = overlay.hide(); }
                    else { let _ = overlay.show(); let _ = overlay.set_focus(); }
                }
            })?;

            // Exclude overlay from screen capture (Windows)
            #[cfg(windows)]
            if let Some(overlay) = app.get_webview_window("overlay") {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(handle) = overlay.window_handle() {
                    if let RawWindowHandle::Win32(h) = handle.as_raw() {
                        exclude_from_capture(h.hwnd.get() as isize);
                    }
                }
            }

            // System tray
            let show_main = MenuItem::with_id(app, "show_main", "Open Shiro", true, None::<&str>)?;
            let show_overlay = MenuItem::with_id(app, "show_overlay", "Open overlay", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_main, &show_overlay, &quit])?;

            let icon = Image::from_path("icons/32x32.png")
                .unwrap_or_else(|_| Image::from_bytes(&[]).unwrap());

            let tray_handle = app.handle().clone();
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Shiro — Ctrl+Shift+0 to capture")
                .menu(&menu)
                .on_menu_event(move |_tray, event| {
                    match event.id().as_ref() {
                        "show_main" => {
                            if let Some(w) = tray_handle.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "show_overlay" => {
                            if let Some(w) = tray_handle.get_webview_window("overlay") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => std::process::exit(0),
                        _ => {}
                    }
                })
                .on_tray_icon_event(move |tray, event| {
                    // Left-click toggles main window
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let visible = w.is_visible().unwrap_or(false);
                            if visible { let _ = w.hide(); }
                            else { let _ = w.show(); let _ = w.set_focus(); }
                        }
                    }
                })
                .build(app)?;

            // Hide main window on close instead of quitting
            let main_win = app.get_webview_window("main").unwrap();
            let main_win_clone = main_win.clone();
            main_win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = main_win_clone.hide();
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
