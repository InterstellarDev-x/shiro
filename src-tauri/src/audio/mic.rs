use super::{downmix, RawAudio};
#[cfg(not(target_os = "macos"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(not(target_os = "macos"))]
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The microphone stream stays on the thread that opened it. `cpal::Stream` is not `Send`.
pub struct MicStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for MicStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

pub fn start(tx: Sender<RawAudio>) -> Result<MicStream, String> {
    ensure_microphone_permission()?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);

    let thread = thread::Builder::new()
        .name("shiro-microphone".into())
        .spawn(move || run(tx, stop_thread, ready_tx))
        .map_err(|e| format!("Failed to start the microphone thread: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(())) => Ok(MicStream {
            stop,
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            stop.store(true, Ordering::SeqCst);
            let _ = thread.join();
            Err(error)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(
            "The microphone thread stopped before it opened.".into(),
        ),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            stop.store(true, Ordering::SeqCst);
            // CoreAudio can stay blocked after the wait. Join it in the background
            // so the overlay can show this error instead of spinning.
            thread::spawn(move || {
                let _ = thread.join();
            });
            Err("The microphone did not open. Enable Voice Input in System Settings → Privacy & Security → Microphone, then try again.".into())
        }
    }
}

pub fn log_microphone_access() {
    #[cfg(target_os = "macos")]
    {
        use objc2_av_foundation::AVMediaTypeAudio;
        let Some(media) = (unsafe { AVMediaTypeAudio }) else {
            return;
        };
        eprintln!(
            "[shiro] microphone authorization status {:?}",
            microphone_authorization(media)
        );
    }
}

#[cfg(target_os = "macos")]
fn microphone_authorization(
    media: &objc2_av_foundation::AVMediaType,
) -> objc2_av_foundation::AVAuthorizationStatus {
    use objc2_av_foundation::AVCaptureDevice;
    unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) }
}

#[cfg(target_os = "macos")]
fn ensure_microphone_permission() -> Result<(), String> {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    use std::time::Instant;

    let media = unsafe { AVMediaTypeAudio }.ok_or("Could not load the microphone permission API")?;
    let status = microphone_authorization(media);
    eprintln!("[shiro] microphone authorization status {status:?}");

    if status == AVAuthorizationStatus::Authorized {
        return Ok(());
    }
    if status == AVAuthorizationStatus::Denied || status == AVAuthorizationStatus::Restricted {
        return Err(
            "Microphone access is off. Turn it on for Voice Input in System Settings → Privacy & Security → Microphone, then click Listen again."
                .into(),
        );
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let handler = RcBlock::new(move |granted: Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe {
        AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler);
    }

    // The prompt callback can arrive before Core Audio has restarted. Opening the
    // microphone in that gap returns error 1937010544 ('stop'). Wait until the
    // status itself says Authorized.
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if let Ok(granted) = rx.try_recv() {
            eprintln!("[shiro] microphone prompt returned {granted}");
            if !granted {
                return Err(
                    "Microphone access was denied. Turn it on for Voice Input in System Settings → Privacy & Security → Microphone."
                        .into(),
                );
            }
        }
        let status = microphone_authorization(media);
        if status == AVAuthorizationStatus::Authorized {
            eprintln!("[shiro] microphone authorization status {status:?}");
            thread::sleep(Duration::from_millis(500));
            return Ok(());
        }
        if status == AVAuthorizationStatus::Denied || status == AVAuthorizationStatus::Restricted {
            return Err(
                "Microphone access is off. Turn it on for Voice Input in System Settings → Privacy & Security → Microphone, then click Listen again."
                    .into(),
            );
        }
        if Instant::now() >= deadline {
            return Err(
                "Still waiting on the microphone prompt. Allow Voice Input in System Settings → Privacy & Security → Microphone, then click Listen again."
                    .into(),
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(not(target_os = "macos"))]
fn ensure_microphone_permission() -> Result<(), String> {
    Ok(())
}

fn run(
    tx: Sender<RawAudio>,
    stop: Arc<AtomicBool>,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let stream = match open(tx) {
        Ok(stream) => {
            let _ = ready.send(Ok(()));
            stream
        }
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(50));
    }
    drop(stream);
}

#[cfg(target_os = "macos")]
struct EngineInput {
    engine: objc2::rc::Retained<objc2::runtime::AnyObject>,
    _tap: block2::RcBlock<dyn Fn(*mut objc2::runtime::AnyObject, *mut objc2::runtime::AnyObject)>,
}

#[cfg(target_os = "macos")]
impl Drop for EngineInput {
    fn drop(&mut self) {
        objc2::rc::autoreleasepool(|_| unsafe {
            let input: *mut objc2::runtime::AnyObject = objc2::msg_send![&*self.engine, inputNode];
            if !input.is_null() {
                let _: () = objc2::msg_send![input, removeTapOnBus: 0isize];
            }
            let _: () = objc2::msg_send![&*self.engine, stop];
        });
    }
}

/// The HAL audio unit keeps returning 1937010544 ('stop') after microphone
/// permission is granted. AVAudioEngine uses the same permission macOS already
/// recorded for Voice Input.
#[cfg(target_os = "macos")]
fn open(tx: Sender<RawAudio>) -> Result<EngineInput, String> {
    use block2::RcBlock;
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;

    autoreleasepool(|_| {
        let class = AnyClass::get(c"AVAudioEngine").ok_or("AVAudioEngine is unavailable")?;
        let engine: Retained<AnyObject> = unsafe { msg_send![class, new] };
        let input: Retained<AnyObject> = unsafe { msg_send![&*engine, inputNode] };
        eprintln!("[shiro] opening microphone with AVAudioEngine");

        let logged = AtomicBool::new(false);
        let block = RcBlock::new(move |buffer: *mut AnyObject, _when: *mut AnyObject| {
            if buffer.is_null() {
                return;
            }
            let (sample_rate, channels, frames, data) = unsafe {
                let format: *mut AnyObject = msg_send![buffer, format];
                if format.is_null() {
                    return;
                }
                let channels: u32 = msg_send![format, channelCount];
                let sample_rate: f64 = msg_send![format, sampleRate];
                let frames: u32 = msg_send![buffer, frameLength];
                let data: *mut *mut f32 = msg_send![buffer, floatChannelData];
                (sample_rate, channels, frames, data)
            };
            if data.is_null() || frames == 0 || channels == 0 || sample_rate <= 0.0 {
                return;
            }
            let channels = channels as usize;
            let frames = frames as usize;
            let sample_rate = sample_rate.round() as u32;
            if !logged.swap(true, Ordering::Relaxed) {
                eprintln!("[shiro] microphone delivering {sample_rate} Hz, {channels} ch");
            }
            let mut interleaved = Vec::with_capacity(frames * channels);
            for frame in 0..frames {
                for channel in 0..channels {
                    unsafe {
                        let ptr = *data.add(channel);
                        if ptr.is_null() {
                            return;
                        }
                        interleaved.push(*ptr.add(frame));
                    }
                }
            }
            let samples = downmix(&interleaved, channels);
            let _ = tx.try_send(RawAudio {
                sample_rate,
                samples,
            });
        });

        unsafe {
            let _: () = msg_send![
                &*input,
                installTapOnBus: 0isize,
                bufferSize: 1024u32,
                format: None::<&AnyObject>,
                block: &*block
            ];
            let _: () = msg_send![&*engine, prepare];
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let started: bool = msg_send![&*engine, startAndReturnError: &mut error];
            if !started {
                return Err(format!(
                    "Could not start the microphone: {}",
                    av_error(error)
                ));
            }
        }

        let format: *mut AnyObject = unsafe { msg_send![&*input, outputFormatForBus: 0isize] };
        if !format.is_null() {
            let rate: f64 = unsafe { msg_send![format, sampleRate] };
            let channels: u32 = unsafe { msg_send![format, channelCount] };
            eprintln!(
                "[shiro] microphone open at {} Hz, {channels} ch",
                rate.round() as u32
            );
        }

        Ok(EngineInput {
            engine,
            _tap: block,
        })
    })
}

#[cfg(target_os = "macos")]
unsafe fn av_error(error: *mut objc2::runtime::AnyObject) -> String {
    if error.is_null() {
        return "AVAudioEngine failed to start".into();
    }
    let description: *mut objc2::runtime::AnyObject = objc2::msg_send![error, localizedDescription];
    if description.is_null() {
        return "AVAudioEngine failed to start".into();
    }
    let text: *const i8 = objc2::msg_send![description, UTF8String];
    if text.is_null() {
        return "AVAudioEngine failed to start".into();
    }
    std::ffi::CStr::from_ptr(text).to_string_lossy().into_owned()
}

#[cfg(not(target_os = "macos"))]
fn open(tx: Sender<RawAudio>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("No microphone found")?;
    let device_name = device.name().unwrap_or_else(|_| "microphone".into());
    eprintln!("[shiro] opening input device {device_name}");
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Microphone configuration failed: {e}"))?;

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let stream = match sample_format {
        SampleFormat::F32 => build(&device, &config, tx, sample_rate, channels, |sample: f32| sample)?,
        SampleFormat::I16 => build(&device, &config, tx, sample_rate, channels, |sample: i16| {
            sample as f32 / i16::MAX as f32
        })?,
        SampleFormat::U16 => build(&device, &config, tx, sample_rate, channels, |sample: u16| {
            (sample as f32 / u16::MAX as f32) * 2.0 - 1.0
        })?,
        other => return Err(format!("Unsupported microphone sample format: {other:?}")),
    };

    stream
        .play()
        .map_err(|e| format!("Failed to start the microphone: {e}"))?;
    eprintln!("[shiro] microphone open at {sample_rate} Hz, {channels} ch");
    Ok(stream)
}

#[cfg(not(target_os = "macos"))]
fn microphone_open_error(message: &str) -> String {
    if message.contains("no longer available") {
        "macOS has not granted the microphone to Voice Input yet. Quit Voice Input, start it again, click Listen, and choose Allow. Voice Input is added to System Settings → Privacy & Security → Microphone only after that prompt.".into()
    } else {
        format!("Failed to open the microphone: {message}")
    }
}

#[cfg(not(target_os = "macos"))]
fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    tx: Sender<RawAudio>,
    sample_rate: u32,
    channels: usize,
    convert: fn(T) -> f32,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + Send + 'static,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let floats: Vec<f32> = data.iter().copied().map(convert).collect();
                let samples = downmix(&floats, channels.max(1));
                let _ = tx.try_send(RawAudio {
                    sample_rate,
                    samples,
                });
            },
            |error| eprintln!("[shiro] microphone error: {error}"),
            None,
        )
        .map_err(|e| microphone_open_error(&e.to_string()))
}
