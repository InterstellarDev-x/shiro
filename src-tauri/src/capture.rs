use image::{
    codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder},
    imageops::FilterType,
    ColorType, ImageEncoder,
};
use xcap::Monitor;

/// Captures the primary monitor and returns a resized PNG.
/// A full retina frame is large enough that some models ignore it.
pub fn capture_primary_screen() -> Result<Vec<u8>, String> {
    let mut monitors = Monitor::all().map_err(|e| format!("Failed to enumerate monitors: {e}"))?;
    if monitors.is_empty() {
        return Err("No monitors found".into());
    }
    let primary = monitors
        .iter()
        .position(|item| item.is_primary().unwrap_or(false))
        .unwrap_or(0);
    let monitor = monitors.swap_remove(primary);

    let image = monitor
        .capture_image()
        .map_err(|e| {
            #[cfg(target_os = "macos")]
            {
                let msg = e.to_string();
                if msg.contains("permission") || msg.contains("CGDisplay") || msg.contains("denied") {
                    return "Screen recording permission denied. Go to System Settings → Privacy & Security → Screen Recording and enable Shiro.".to_string();
                }
            }
            format!("Screen capture failed: {e}")
        })?;

    #[cfg(target_os = "macos")]
    if looks_blank(&image) {
        return Err("Screen capture returned a blank image. Go to System Settings → Privacy & Security → Screen Recording and enable Shiro.".into());
    }

    const MAX_EDGE: u32 = 1280;
    let (width, height) = (image.width(), image.height());
    let image = if width > MAX_EDGE || height > MAX_EDGE {
        let scale = MAX_EDGE as f32 / width.max(height) as f32;
        let next_width = ((width as f32) * scale).round().max(1.0) as u32;
        let next_height = ((height as f32) * scale).round().max(1.0) as u32;
        image::imageops::resize(&image, next_width, next_height, FilterType::Triangle)
    } else {
        image
    };

    let (width, height) = (image.width(), image.height());
    let rgba_bytes = image.into_raw();
    let mut png_bytes: Vec<u8> = Vec::new();
    PngEncoder::new_with_quality(&mut png_bytes, CompressionType::Fast, PngFilter::Sub)
        .write_image(&rgba_bytes, width, height, ColorType::Rgba8.into())
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    eprintln!("[shiro] screen capture {width}x{height}, {} bytes", png_bytes.len());

    Ok(png_bytes)
}

fn looks_blank(image: &image::RgbaImage) -> bool {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return true;
    }
    let step_x = (width / 24).max(1);
    let step_y = (height / 24).max(1);
    let mut seen = 0u32;
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            let pixel = image.get_pixel(x, y);
            if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                return false;
            }
            seen += 1;
            x += step_x;
        }
        y += step_y;
    }
    seen > 0
}
