//! Undetectable capture exclusion, and a display-only process label.
//!
//! The capture sweep follows Natively's `capture_exclusion.rs`: Electron's
//! content protection only covers its own web windows, so tooltips, menus,
//! file dialogs, and notification windows this process opens stay visible in a
//! screen share unless something walks the process's other windows too.
//! Marking a window private keeps it off a normal capture. A camera pointed
//! at the screen still sees it. System Notification Center banners are drawn
//! by another process and are not ours to mark.
//!
//! The label follows Natively's disguise: Activity Monitor's process name is
//! the Launch Services display name, which a running app can change without
//! editing its bundle. The Dock and taskbar icon is the system app's icon,
//! read at runtime. The executable path, code signature, and binary stay
//! Voice Input's.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, State};

const REAL_NAME: &str = "Voice Input";
const TRAY_ID: &str = "shiro";

static SWEEP_STOP: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisguiseMode {
    None,
    Terminal,
    Settings,
    Activity,
}

impl DisguiseMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Terminal => "terminal",
            Self::Settings => "settings",
            Self::Activity => "activity",
        }
    }
}

/// Name presented in the process list. The trailing space is intentional:
/// it keeps the label from being the exact string of the real system app.
/// `windows` selects the Windows wording (Command Prompt, Settings, Task Manager).
pub fn disguise_label(mode: DisguiseMode, windows: bool) -> &'static str {
    match (mode, windows) {
        (DisguiseMode::None, _) => REAL_NAME,
        (DisguiseMode::Terminal, true) => "Command Prompt ",
        (DisguiseMode::Terminal, false) => "Terminal ",
        (DisguiseMode::Settings, true) => "Settings ",
        (DisguiseMode::Settings, false) => "System Settings ",
        (DisguiseMode::Activity, true) => "Task Manager ",
        (DisguiseMode::Activity, false) => "Activity Monitor ",
    }
}

pub fn parse_disguise(value: &str) -> DisguiseMode {
    match value {
        "terminal" => DisguiseMode::Terminal,
        "settings" => DisguiseMode::Settings,
        "activity" => DisguiseMode::Activity,
        _ => DisguiseMode::None,
    }
}

fn visible_label(mode: DisguiseMode) -> String {
    disguise_label(mode, cfg!(target_os = "windows"))
        .trim()
        .to_string()
}

fn process_list_name(mode: DisguiseMode) -> String {
    disguise_label(mode, cfg!(target_os = "windows")).to_string()
}

pub struct Presence {
    undetectable: AtomicBool,
    disguise: Mutex<DisguiseMode>,
    /// Counts sweep ticks so the process-list name is re-applied every couple
    /// of seconds, not on every tooltip pass.
    name_tick: AtomicU32,
    #[cfg(windows)]
    icons: Mutex<WinIcons>,
}

#[cfg(windows)]
struct WinIcons {
    big: isize,
    small: isize,
}

impl Presence {
    pub fn new() -> Self {
        Self {
            undetectable: AtomicBool::new(true),
            disguise: Mutex::new(DisguiseMode::None),
            name_tick: AtomicU32::new(0),
            #[cfg(windows)]
            icons: Mutex::new(WinIcons { big: 0, small: 0 }),
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct PresenceView {
    pub undetectable: bool,
    pub disguise: String,
    pub label: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedPresence {
    undetectable: bool,
    disguise: String,
}

fn lock(mode: &Mutex<DisguiseMode>) -> std::sync::MutexGuard<'_, DisguiseMode> {
    mode.lock().unwrap_or_else(|err| err.into_inner())
}

fn snapshot(presence: &Presence) -> (bool, DisguiseMode) {
    let mode = *lock(&presence.disguise);
    (presence.undetectable.load(Ordering::Relaxed), mode)
}

fn view_of(presence: &Presence) -> PresenceView {
    let (undetectable, mode) = snapshot(presence);
    PresenceView {
        undetectable,
        disguise: mode.as_str().to_string(),
        label: visible_label(mode),
    }
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("presence.json"))
}

fn save(app: &AppHandle) {
    let Some(path) = config_path(app) else {
        return;
    };
    let presence = app.state::<Presence>();
    let (undetectable, mode) = snapshot(&presence);
    let saved = SavedPresence {
        undetectable,
        disguise: mode.as_str().to_string(),
    };
    if let Ok(text) = serde_json::to_string(&saved) {
        let _ = std::fs::write(path, text);
    }
}

/// Load the saved choice and push it to the OS. Missing file means
/// undetectable on and no disguise, which matches the windows that already
/// start content-protected.
pub fn restore(app: &AppHandle) {
    if let Some(path) = config_path(app) {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(saved) = serde_json::from_str::<SavedPresence>(&text) {
                let presence = app.state::<Presence>();
                presence
                    .undetectable
                    .store(saved.undetectable, Ordering::Relaxed);
                *lock(&presence.disguise) = parse_disguise(&saved.disguise);
            }
        }
    }
    apply(app);
}

pub fn start_sweep(app: AppHandle) {
    std::thread::Builder::new()
        .name("presence-sweep".into())
        .spawn(move || {
            while !SWEEP_STOP.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                if SWEEP_STOP.load(Ordering::Relaxed) {
                    break;
                }
                let (tx, rx) = mpsc::channel();
                let app_for_sweep = app.clone();
                if app
                    .run_on_main_thread(move || {
                        sweep(&app_for_sweep);
                        let _ = tx.send(());
                    })
                    .is_err()
                {
                    break;
                }
                // Don't queue a second pass if the main thread is busy, and
                // don't sit forever if the app is quitting.
                let _ = rx.recv_timeout(Duration::from_millis(250));
            }
        })
        .ok();
}

pub fn stop_sweep() {
    SWEEP_STOP.store(true, Ordering::Relaxed);
}

fn sweep(app: &AppHandle) {
    let presence = app.state::<Presence>();
    let (undetectable, mode) = snapshot(&presence);
    sync_titles(app, mode);
    if undetectable {
        mark_process_windows(true);
    }
    let tick = presence.name_tick.fetch_add(1, Ordering::Relaxed);
    if tick % 40 == 0 {
        apply_process_name(mode);
    }
}

fn apply(app: &AppHandle) {
    let presence = app.state::<Presence>();
    let (undetectable, mode) = snapshot(&presence);
    apply_identity(app, mode);
    sync_titles(app, mode);
    sync_tooltip(app, &visible_label(mode));
    for (_, window) in app.webview_windows() {
        let _ = window.set_content_protected(undetectable);
    }
    mark_process_windows(undetectable);
}

fn desired_title(mode: DisguiseMode, window: &str) -> String {
    if mode == DisguiseMode::None {
        match window {
            "overlay" => "Voice Input Overlay".to_string(),
            _ => REAL_NAME.to_string(),
        }
    } else {
        visible_label(mode)
    }
}

fn sync_titles(app: &AppHandle, mode: DisguiseMode) {
    for (label, window) in app.webview_windows() {
        let title = desired_title(mode, &label);
        if window.title().ok().as_deref() != Some(title.as_str()) {
            let _ = window.set_title(&title);
        }
    }
}

fn sync_tooltip(app: &AppHandle, label: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(label));
    }
}

fn apply_identity(app: &AppHandle, mode: DisguiseMode) {
    #[cfg(target_os = "macos")]
    apply_identity_macos(app, mode);
    #[cfg(windows)]
    apply_identity_windows(app, mode);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (app, mode);
    }
}

fn apply_process_name(mode: DisguiseMode) {
    // Re-apply only the process-list record. setProcessName and the Dock
    // icon stay on the explicit change, so this tick cannot flash the Dock.
    #[cfg(target_os = "macos")]
    {
        let _ = set_launch_services_display_name(&process_list_name(mode));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = mode;
    }
}

#[tauri::command]
pub fn presence_state(presence: State<'_, Presence>) -> PresenceView {
    view_of(&presence)
}

#[tauri::command]
pub fn set_undetectable(
    enabled: bool,
    app: AppHandle,
    presence: State<'_, Presence>,
) -> Result<PresenceView, String> {
    presence.undetectable.store(enabled, Ordering::Relaxed);
    save(&app);
    apply(&app);
    Ok(view_of(&presence))
}

#[tauri::command]
pub fn set_disguise(
    mode: String,
    app: AppHandle,
    presence: State<'_, Presence>,
) -> Result<PresenceView, String> {
    *lock(&presence.disguise) = parse_disguise(&mode);
    save(&app);
    apply(&app);
    Ok(view_of(&presence))
}

fn mark_process_windows(excluded: bool) {
    #[cfg(target_os = "macos")]
    mark_macos_windows(excluded);
    #[cfg(windows)]
    mark_windows(excluded);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = excluded;
    }
}

#[cfg(target_os = "macos")]
fn apply_identity_macos(app: &AppHandle, mode: DisguiseMode) {
    if mode == DisguiseMode::None {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        set_dock_icon(None);
        set_menu_bar_name(REAL_NAME);
        set_macos_process_name(REAL_NAME);
        return;
    }
    set_dock_icon(macos_icon_path(mode));
    let name = process_list_name(mode);
    set_macos_process_name(&name);
    set_menu_bar_name(name.trim());
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
}

#[cfg(target_os = "macos")]
fn macos_icon_path(mode: DisguiseMode) -> Option<&'static str> {
    let candidates: &[&str] = match mode {
        DisguiseMode::Terminal => &["/System/Applications/Utilities/Terminal.app"],
        DisguiseMode::Settings => &[
            "/System/Applications/System Settings.app",
            "/System/Applications/System Preferences.app",
        ],
        DisguiseMode::Activity => &["/System/Applications/Utilities/Activity Monitor.app"],
        DisguiseMode::None => return None,
    };
    candidates
        .iter()
        .copied()
        .find(|path| std::path::Path::new(path).exists())
}

#[cfg(target_os = "macos")]
fn set_dock_icon(path: Option<&str>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSWorkspace};
    use objc2_foundation::NSString;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // nil restores the bundle icon. A path is a system app on this machine;
    // its icon is not copied into our binary.
    let image = path.map(|path| NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path)));
    unsafe { app.setApplicationIconImage(image.as_deref()) };
}

#[cfg(target_os = "macos")]
fn set_menu_bar_name(name: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(menu) = app.mainMenu() else {
        return;
    };
    let Some(item) = menu.itemAtIndex(0) else {
        return;
    };
    let title = NSString::from_str(name);
    item.setTitle(&title);
    if let Some(submenu) = item.submenu() {
        submenu.setTitle(&title);
    }
}

#[cfg(target_os = "macos")]
fn set_macos_process_name(name: &str) {
    use objc2_foundation::{NSProcessInfo, NSString};

    NSProcessInfo::processInfo().setProcessName(&NSString::from_str(name));
    if !set_launch_services_display_name(name) {
        eprintln!("[shiro] process list name stayed unchanged ({})", name.trim());
    }
}

/// Rewrite this process's Launch Services display name. Activity Monitor's
/// Process Name column reads that record, not the executable filename.
/// Nothing on disk is written, so the signature is untouched.
#[cfg(target_os = "macos")]
fn set_launch_services_display_name(name: &str) -> bool {
    use std::os::raw::{c_char, c_int, c_void};

    type GetAsn = unsafe extern "C" fn() -> *const c_void;
    type SetItem = unsafe extern "C" fn(
        c_int,
        *const c_void,
        *const c_void,
        *const c_void,
        *mut *const c_void,
    ) -> c_int;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithBytes(
            alloc: *const c_void,
            bytes: *const u8,
            num_bytes: isize,
            encoding: u32,
            is_external: u8,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;

    unsafe fn symbol(handle: *mut c_void, name: &[u8]) -> *mut c_void {
        libc::dlsym(handle, name.as_ptr() as *const c_char)
    }

    unsafe {
        let mut get_asn = symbol(libc::RTLD_DEFAULT, b"_LSGetCurrentApplicationASN\0");
        let mut set_item = symbol(libc::RTLD_DEFAULT, b"_LSSetApplicationInformationItem\0");
        let mut key_slot = symbol(libc::RTLD_DEFAULT, b"_kLSDisplayNameKey\0");
        if get_asn.is_null() || set_item.is_null() || key_slot.is_null() {
            let handle = libc::dlopen(
                b"/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/LaunchServices\0"
                    .as_ptr() as *const c_char,
                libc::RTLD_LAZY,
            );
            if !handle.is_null() {
                if get_asn.is_null() {
                    get_asn = symbol(handle, b"_LSGetCurrentApplicationASN\0");
                }
                if set_item.is_null() {
                    set_item = symbol(handle, b"_LSSetApplicationInformationItem\0");
                }
                if key_slot.is_null() {
                    key_slot = symbol(handle, b"_kLSDisplayNameKey\0");
                }
            }
        }
        if get_asn.is_null() || set_item.is_null() || key_slot.is_null() {
            return false;
        }
        let get_asn: GetAsn = std::mem::transmute(get_asn);
        let set_item: SetItem = std::mem::transmute(set_item);
        let display_key = *(key_slot as *const *const c_void);
        if display_key.is_null() {
            return false;
        }
        let asn = get_asn();
        if asn.is_null() {
            return false;
        }
        let value = CFStringCreateWithBytes(
            std::ptr::null(),
            name.as_ptr(),
            name.len() as isize,
            UTF8,
            0,
        );
        if value.is_null() {
            return false;
        }
        let status = set_item(-1, asn, display_key, value, std::ptr::null_mut());
        CFRelease(value);
        status == 0
    }
}

#[cfg(target_os = "macos")]
fn mark_macos_windows(excluded: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSWindowSharingType};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let sharing = if excluded {
        NSWindowSharingType::None
    } else {
        NSWindowSharingType::ReadOnly
    };
    let app = NSApplication::sharedApplication(mtm);
    let windows = app.windows();
    for index in 0..windows.count() {
        // Set every time. An activation change can clear the WindowServer
        // sharing state while AppKit still reports the value we last wrote.
        windows.objectAtIndex(index).setSharingType(sharing);
    }
}

#[cfg(windows)]
fn apply_identity_windows(app: &AppHandle, mode: DisguiseMode) {
    set_app_user_model_id(mode);
    let presence = app.state::<Presence>();
    let mut icons = presence.icons.lock().unwrap_or_else(|err| err.into_inner());
    let (big, small) = match windows_icon_path(mode) {
        Some(path) => (extract_icon(path, true), extract_icon(path, false)),
        None => (0, 0),
    };
    let previous = (icons.big, icons.small);
    icons.big = big;
    icons.small = small;
    drop(icons);
    paint_window_icons(big, small);
    if previous.0 != 0 {
        unsafe { DestroyIcon(previous.0) };
    }
    if previous.1 != 0 {
        unsafe { DestroyIcon(previous.1) };
    }
}

#[cfg(windows)]
fn windows_icon_path(mode: DisguiseMode) -> Option<&'static str> {
    let candidates: &[&str] = match mode {
        DisguiseMode::Terminal => &[r"C:\Windows\System32\cmd.exe"],
        DisguiseMode::Settings => &[
            r"C:\Windows\ImmersiveControlPanel\SystemSettings.exe",
            r"C:\Windows\System32\SystemSettings.exe",
        ],
        DisguiseMode::Activity => &[r"C:\Windows\System32\Taskmgr.exe"],
        DisguiseMode::None => return None,
    };
    candidates
        .iter()
        .copied()
        .find(|path| std::path::Path::new(path).exists())
}

#[cfg(windows)]
fn set_app_user_model_id(mode: DisguiseMode) {
    // A private id so the taskbar button does not merge into the real
    // Command Prompt, Settings, or Task Manager button.
    let id = match mode {
        DisguiseMode::None => "com.adityaapkumar.shiro",
        DisguiseMode::Terminal => "com.adityaapkumar.shiro.terminal",
        DisguiseMode::Settings => "com.adityaapkumar.shiro.settings",
        DisguiseMode::Activity => "com.adityaapkumar.shiro.activity",
    };
    let wide = wide(id);
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(wide.as_ptr());
    }
}

#[cfg(windows)]
fn paint_window_icons(big: isize, small: isize) {
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    for_each_process_window(|hwnd| unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG, big);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small);
    });
}

#[cfg(windows)]
fn extract_icon(path: &str, large: bool) -> isize {
    const SHGFI_ICON: u32 = 0x100;
    const SHGFI_LARGEICON: u32 = 0x0;
    const SHGFI_SMALLICON: u32 = 0x1;
    let wide = wide(path);
    let mut info = unsafe { std::mem::zeroed::<ShFileInfoW>() };
    let flags = SHGFI_ICON | if large { SHGFI_LARGEICON } else { SHGFI_SMALLICON };
    let found = unsafe {
        SHGetFileInfoW(
            wide.as_ptr(),
            0,
            &mut info,
            std::mem::size_of::<ShFileInfoW>() as u32,
            flags,
        )
    };
    if found == 0 {
        0
    } else {
        info.h_icon
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn mark_windows(excluded: bool) {
    const WDA_NONE: u32 = 0x0000_0000;
    const WDA_MONITOR: u32 = 0x0000_0001;
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x0000_0011;
    for_each_process_window(|hwnd| unsafe {
        let mut current = 0u32;
        let _ = GetWindowDisplayAffinity(hwnd, &mut current);
        if excluded {
            if current == WDA_NONE
                && SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0
            {
                let _ = SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
            }
        } else if current != WDA_NONE {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_NONE);
        }
    });
}

#[cfg(windows)]
fn for_each_process_window<F: FnMut(isize)>(f: F) {
    let mut state = (unsafe { GetCurrentProcessId() }, f);
    unsafe {
        EnumWindows(enum_visit::<F>, &mut state as *mut (u32, F) as isize);
    }
}

#[cfg(windows)]
unsafe extern "system" fn enum_visit<F: FnMut(isize)>(hwnd: isize, lparam: isize) -> i32 {
    let state = &mut *(lparam as *mut (u32, F));
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == state.0 {
        (state.1)(hwnd);
    }
    1
}

#[cfg(windows)]
#[repr(C)]
struct ShFileInfoW {
    h_icon: isize,
    i_icon: i32,
    dw_attributes: u32,
    sz_display_name: [u16; 260],
    sz_type_name: [u16; 80],
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn SetWindowDisplayAffinity(hwnd: isize, affinity: u32) -> i32;
    fn GetWindowDisplayAffinity(hwnd: isize, affinity: *mut u32) -> i32;
    fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn DestroyIcon(icon: isize) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcessId() -> u32;
}

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn SHGetFileInfoW(
        path: *const u16,
        file_attributes: u32,
        info: *mut ShFileInfoW,
        info_size: u32,
        flags: u32,
    ) -> usize;
    fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_the_system_app_wording() {
        assert_eq!(disguise_label(DisguiseMode::None, false), "Voice Input");
        assert_eq!(disguise_label(DisguiseMode::Terminal, false), "Terminal ");
        assert_eq!(disguise_label(DisguiseMode::Settings, false), "System Settings ");
        assert_eq!(disguise_label(DisguiseMode::Activity, false), "Activity Monitor ");
        assert_eq!(disguise_label(DisguiseMode::Terminal, true), "Command Prompt ");
        assert_eq!(disguise_label(DisguiseMode::Settings, true), "Settings ");
        assert_eq!(disguise_label(DisguiseMode::Activity, true), "Task Manager ");
    }

    #[test]
    fn labels_are_names_not_paths() {
        for windows in [false, true] {
            for mode in [
                DisguiseMode::None,
                DisguiseMode::Terminal,
                DisguiseMode::Settings,
                DisguiseMode::Activity,
            ] {
                let name = disguise_label(mode, windows);
                assert!(!name.contains('/'), "{name}");
                assert!(!name.contains('\\'), "{name}");
                assert!(!name.is_empty());
            }
        }
    }

    #[test]
    fn unknown_disguise_is_the_real_app() {
        assert_eq!(parse_disguise("nope"), DisguiseMode::None);
        assert_eq!(parse_disguise("terminal"), DisguiseMode::Terminal);
    }
}
