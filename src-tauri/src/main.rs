// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    relaunch_in_app_bundle();
    shiro_lib::run()
}

/// The dev process is a child of Terminal, which already has microphone access.
/// macOS therefore never asks Shiro, and Core Audio answers with error 'stop'
/// (1937010544). Launch Services starts the signed bundle as its own app, so the
/// permission prompt names Shiro and the microphone can start.
#[cfg(target_os = "macos")]
fn relaunch_in_app_bundle() {
    use std::process::Command;

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    if exe.to_string_lossy().contains(".app/Contents/MacOS/") {
        if let Some(debug_dir) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).and_then(|p| p.parent()) {
            let _ = std::fs::write(debug_dir.join("shiro.pid"), std::process::id().to_string());
        }
        exit_with_dev_parent();
        eprintln!(
            "[shiro] running as Shiro.app (pid {}) so macOS can ask Shiro for the microphone",
            std::process::id()
        );
        return;
    }
    let Some(parent) = exe.parent() else {
        return;
    };

    let app = parent.join("Shiro.app");
    let contents = app.join("Contents");
    let macos_dir = contents.join("MacOS");
    let bundled = macos_dir.join("shiro");
    let stamp_path = parent.join("shiro.codesign-stamp");
    // Re-signing on every launch changes the app identity, so a Screen Recording
    // toggle granted to the previous copy never applies to the new one.
    let stamp = exe_stamp(&exe);
    let already_signed = stamp
        .as_deref()
        .is_some_and(|stamp| std::fs::read_to_string(&stamp_path).ok().as_deref() == Some(stamp))
        && bundled.is_file()
        && contents.join("Info.plist").is_file();
    if already_signed {
        eprintln!("[shiro] using the already signed Shiro.app");
    } else {
        if let Err(error) = std::fs::create_dir_all(&macos_dir) {
            eprintln!("[shiro] could not create Shiro.app: {error}");
            return;
        }
        if let Err(error) = std::fs::copy(&exe, &bundled) {
            eprintln!("[shiro] could not copy into Shiro.app: {error}");
            return;
        }
        if let Err(error) = std::fs::write(contents.join("Info.plist"), include_str!("../Info.plist"))
        {
            eprintln!("[shiro] could not write Shiro.app Info.plist: {error}");
            return;
        }

        let signed = Command::new("codesign")
            .args([
                "--force",
                "--sign",
                "-",
                "--identifier",
                "com.adityaapkumar.shiro",
            ])
            .arg(&app)
            .output();
        match signed {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                eprintln!(
                    "[shiro] could not sign Shiro.app: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                return;
            }
            Err(error) => {
                eprintln!("[shiro] could not run codesign: {error}");
                return;
            }
        }
        if let Some(stamp) = &stamp {
            let _ = std::fs::write(&stamp_path, stamp);
        }
        eprintln!("[shiro] signed a new Shiro.app, so macOS may ask for Screen Recording again");
    }

    let log_path = parent.join("shiro-app.log");
    let _ = std::fs::write(&log_path, "");
    let pid_path = parent.join("shiro.pid");
    let _ = std::fs::remove_file(&pid_path);

    let mut command = Command::new("/usr/bin/open");
    command.arg("-n").arg("-W");
    command.arg("--stdout").arg(&log_path);
    command.arg("--stderr").arg(&log_path);
    for (key, value) in std::env::vars() {
        if key == "SHIRO_RESPONSIBLE" || key == "_" || value.contains('\0') {
            continue;
        }
        let mut chars = key.chars();
        let Some(first) = chars.next() else { continue };
        if !(first.is_ascii_alphabetic() || first == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        command.arg("--env").arg(format!("{key}={value}"));
    }
    command.arg("--env").arg("SHIRO_RESPONSIBLE=1");
    command.arg("--env").arg(format!("SHIRO_DEV_PARENT={}", std::process::id()));
    command.arg(&app);
    command.arg("--args");
    for arg in std::env::args_os().skip(1) {
        command.arg(arg);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("[shiro] could not launch Shiro.app: {error}");
            return;
        }
    };
    eprintln!("[shiro] launched Shiro.app so macOS can ask Shiro for the microphone");
    let follow_log = log_path.clone();
    std::thread::spawn(move || {
        let mut offset = 0usize;
        loop {
            if let Ok(data) = std::fs::read(&follow_log) {
                if data.len() > offset {
                    eprint!("{}", String::from_utf8_lossy(&data[offset..]));
                    offset = data.len();
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });

    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = text.trim().parse::<i32>() {
                APP_PID.store(pid, std::sync::atomic::Ordering::SeqCst);
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    unsafe {
        libc::signal(libc::SIGTERM, forward_signal as libc::sighandler_t);
        libc::signal(libc::SIGINT, forward_signal as libc::sighandler_t);
    }
    let status = child.wait();
    let code = status.ok().and_then(|status| status.code()).unwrap_or(1);
    std::process::exit(code);
}

/// `tauri dev` kills the launcher with SIGKILL when it rebuilds, so the signal
/// forwarding below never runs. Without this check the old Shiro.app kept running
/// with no Dock icon, and its windows sat on screen catching clicks.
#[cfg(target_os = "macos")]
fn exit_with_dev_parent() {
    let Some(parent) = std::env::var("SHIRO_DEV_PARENT")
        .ok()
        .and_then(|pid| pid.parse::<i32>().ok())
        .filter(|pid| *pid > 1)
    else {
        return;
    };
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let alive = unsafe { libc::kill(parent, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !alive {
            eprintln!("[shiro] the dev launcher exited, so this copy is quitting too");
            std::process::exit(0);
        }
    });
}

#[cfg(target_os = "macos")]
fn exe_stamp(exe: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(exe).ok()?;
    let modified = meta.modified().ok()?;
    let nanos = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    Some(format!("{}:{nanos}", meta.len()))
}

#[cfg(target_os = "macos")]
static APP_PID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[cfg(target_os = "macos")]
extern "C" fn forward_signal(sig: i32) {
    let pid = APP_PID.load(std::sync::atomic::Ordering::SeqCst);
    if pid > 0 {
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

#[cfg(not(target_os = "macos"))]
fn relaunch_in_app_bundle() {}
