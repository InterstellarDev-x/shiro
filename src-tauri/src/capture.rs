use image::{ImageEncoder, codecs::png::PngEncoder, ColorType};
use xcap::Monitor;

/// Captures the primary monitor and returns raw PNG bytes (in memory, not written to disk).
pub fn capture_primary_screen() -> Result<Vec<u8>, String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to enumerate monitors: {e}"))?;
    let monitor = monitors.into_iter().next().ok_or("No monitors found")?;

    let image = monitor
        .capture_image()
        .map_err(|e| {
            // On macOS, a permission denial typically results in a capture error.
            // Give the user a clear, actionable message.
            #[cfg(target_os = "macos")]
            {
                let msg = e.to_string();
                if msg.contains("permission") || msg.contains("CGDisplay") || msg.contains("denied") {
                    return "Screen recording permission denied. Go to System Settings → Privacy & Security → Screen Recording and enable Shiro.".to_string();
                }
            }
            format!("Screen capture failed: {e}")
        })?;

    // On macOS, xcap may return a fully black image when permission is denied
    // without raising an error. Detect that and surface a useful message.
    #[cfg(target_os = "macos")]
    {
        let is_blank = image.pixels().all(|p| p.0[0] == 0 && p.0[1] == 0 && p.0[2] == 0);
        if is_blank {
            return Err("Screen capture returned a blank image. Go to System Settings → Privacy & Security → Screen Recording and enable Shiro.".to_string());
        }
    }

    let (width, height) = (image.width(), image.height());
    let rgba_bytes = image.into_raw();

    let mut png_bytes: Vec<u8> = Vec::new();
    PngEncoder::new(&mut png_bytes)
        .write_image(&rgba_bytes, width, height, ColorType::Rgba8.into())
        .map_err(|e| format!("PNG encoding failed: {e}"))?;

    Ok(png_bytes)
}
