use super::RawAudio;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct SystemCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for SystemCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

pub fn start(tx: Sender<RawAudio>) -> Result<SystemCapture, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);

    let thread = thread::Builder::new()
        .name("shiro-system-audio".into())
        .spawn(move || capture_loop(tx, stop_thread, ready_tx))
        .map_err(|e| format!("Failed to start system-audio thread: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(())) => Ok(SystemCapture {
            stop,
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            stop.store(true, Ordering::SeqCst);
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            let _ = thread.join();
            Err("Timed out starting system audio capture".into())
        }
    }
}

#[cfg(target_os = "macos")]
fn capture_loop(
    tx: Sender<RawAudio>,
    stop: Arc<AtomicBool>,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    use screencapturekit::prelude::*;
    use std::sync::atomic::AtomicBool as StdAtomic;

    struct Forward {
        tx: Sender<RawAudio>,
        logged: StdAtomic,
    }

    impl SCStreamOutputTrait for Forward {
        fn did_output_sample_buffer(
            &self,
            sample: CMSampleBuffer,
            output_type: SCStreamOutputType,
        ) {
            if output_type != SCStreamOutputType::Audio {
                return;
            }
            let Some((samples, description)) = decode_sample(&sample) else {
                return;
            };
            if !self.logged.swap(true, Ordering::Relaxed) {
                eprintln!("[shiro] system audio format: {description}");
            }
            let _ = self.tx.try_send(RawAudio {
                sample_rate: 48_000,
                samples,
            });
        }
    }

    let setup = (|| {
        // Allowing the prompt often comes back as "not granted" for a moment.
        // Try the capture anyway, and retry, before telling the user it failed.
        request_screen_recording();
        let content = shareable_content()?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or("No display found for system audio")?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| format!("System audio filter failed: {e}"))?;
        let config = SCStreamConfiguration::new()
            .with_width(64)
            .with_height(64)
            .with_captures_audio(true)
            .with_sample_rate(48_000)
            .with_channel_count(2);
        let mut stream = SCStream::new(&filter, &config)
            .map_err(|e| format!("System audio stream failed: {e}"))?;
        stream
            .add_output_handler(
                Forward {
                    tx,
                    logged: StdAtomic::new(false),
                },
                SCStreamOutputType::Audio,
            )
            .map_err(|e| format!("System audio handler failed: {e}"))?;
        stream
            .start_capture()
            .map_err(|e| format!("System audio start failed: {e}"))?;
        Ok(stream)
    })();

    let stream = match setup {
        Ok(stream) => {
            let _ = ready.send(Ok(()));
            stream
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    eprintln!("[shiro] system audio capture started");

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(50));
    }
    let _ = stream.stop_capture();
}

#[cfg(target_os = "macos")]
fn screen_recording_allowed() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> u8;
    }
    unsafe { CGPreflightScreenCaptureAccess() == 1 }
}

#[cfg(target_os = "macos")]
fn request_screen_recording() {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGRequestScreenCaptureAccess() -> u8;
    }

    if screen_recording_allowed() {
        eprintln!("[shiro] screen recording already allowed");
        return;
    }
    eprintln!("[shiro] requesting screen recording permission");
    let granted = unsafe { CGRequestScreenCaptureAccess() == 1 };
    eprintln!("[shiro] screen recording request returned {granted}");
    for _ in 0..20 {
        if screen_recording_allowed() {
            eprintln!("[shiro] screen recording allowed");
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(target_os = "macos")]
fn shareable_content() -> Result<screencapturekit::shareable_content::SCShareableContent, String> {
    use screencapturekit::prelude::SCShareableContent;

    let mut last = String::new();
    for attempt in 1..=8 {
        match SCShareableContent::get() {
            Ok(content) => return Ok(content),
            Err(error) => {
                last = error.to_string();
                eprintln!("[shiro] screen content attempt {attempt}: {last}");
                let denied = last.to_lowercase().contains("declined") || last.to_lowercase().contains("tcc");
                if !denied {
                    break;
                }
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
    Err(format!(
        "The other person's audio needs Screen Recording. Turn on Shiro in System Settings → Privacy & Security → Screen Recording. Your microphone stays on. ({last})"
    ))
}

#[cfg(target_os = "macos")]
fn decode_sample(sample: &screencapturekit::cm::CMSampleBuffer) -> Option<(Vec<f32>, String)> {
    use super::decode_interleaved;

    let list = sample.audio_buffer_list().ok()?;
    let frames = sample.num_samples().max(0) as usize;
    let buffers = list.num_buffers();
    if buffers == 0 || frames == 0 {
        return None;
    }
    if buffers == 1 {
        let buffer = list.get(0)?;
        let channels = buffer.number_channels.max(1) as usize;
        let mono = decode_interleaved(buffer.data(), channels, frames);
        if mono.is_empty() {
            return None;
        }
        return Some((
            mono,
            format!("interleaved channels={channels} frames={frames} bytes={}", buffer.data().len()),
        ));
    }

    let mut mixed = vec![0.0f32; frames];
    let mut used = 0usize;
    for index in 0..buffers {
        let buffer = list.get(index)?;
        let decoded = decode_interleaved(buffer.data(), 1, frames);
        if decoded.len() != frames {
            continue;
        }
        for (slot, sample) in mixed.iter_mut().zip(decoded) {
            *slot += sample;
        }
        used += 1;
    }
    if used == 0 {
        return None;
    }
    for sample in &mut mixed {
        *sample /= used as f32;
    }
    Some((mixed, format!("planar buffers={buffers} frames={frames}")))
}

#[cfg(windows)]
fn capture_loop(
    tx: Sender<RawAudio>,
    stop: Arc<AtomicBool>,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

    let setup = (|| {
        wasapi::initialize_mta().map_err(|e| format!("Windows audio init failed: {e}"))?;
        let enumerator =
            DeviceEnumerator::new().map_err(|e| format!("Windows audio devices failed: {e}"))?;
        let device = enumerator
            .get_default_device(&Direction::Render)
            .map_err(|e| format!("No default playback device for loopback: {e}"))?;
        let mut client = device
            .get_iaudioclient()
            .map_err(|e| format!("Windows audio client failed: {e}"))?;
        let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
        let mode = StreamMode::PollingShared {
            autoconvert: true,
            buffer_duration_hns: 200_000,
        };
        // Render device + capture direction is how WASAPI selects loopback.
        client
            .initialize_client(&format, &Direction::Capture, &mode)
            .map_err(|e| format!("Windows loopback init failed: {e}"))?;
        let capture = client
            .get_audiocaptureclient()
            .map_err(|e| format!("Windows loopback reader failed: {e}"))?;
        client
            .start_stream()
            .map_err(|e| format!("Windows loopback start failed: {e}"))?;
        Ok((client, capture))
    })();

    let (mut client, capture) = match setup {
        Ok(parts) => {
            let _ = ready.send(Ok(()));
            parts
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    eprintln!("[shiro] system audio loopback started");

    while !stop.load(Ordering::SeqCst) {
        let frames = match capture.get_next_packet_size() {
            Ok(Some(frames)) => frames,
            Ok(None) => 0,
            Err(error) => {
                eprintln!("[shiro] loopback packet size failed: {error}");
                break;
            }
        };
        if frames == 0 {
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        let mut bytes = vec![0u8; frames as usize * 8];
        match capture.read_from_device(&mut bytes) {
            Ok((read, _)) if read > 0 => {
                let samples = super::decode_interleaved(&bytes[..read as usize * 8], 2, read as usize);
                if !samples.is_empty() {
                    let _ = tx.try_send(RawAudio {
                        sample_rate: 48_000,
                        samples,
                    });
                }
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("[shiro] loopback read failed: {error}");
                break;
            }
        }
    }

    let _ = client.stop_stream();
}

#[cfg(not(any(target_os = "macos", windows)))]
fn capture_loop(
    _tx: Sender<RawAudio>,
    _stop: Arc<AtomicBool>,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let _ = ready.send(Err(
        "System audio capture is implemented for macOS and Windows.".into(),
    ));
}
