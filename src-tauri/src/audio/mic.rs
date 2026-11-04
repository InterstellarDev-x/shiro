use super::{downmix, RawAudio};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);

    let thread = thread::Builder::new()
        .name("shiro-microphone".into())
        .spawn(move || run(tx, stop_thread, ready_tx))
        .map_err(|e| format!("Failed to start the microphone thread: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(MicStream {
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
            Err("Timed out opening the microphone".into())
        }
    }
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

fn open(tx: Sender<RawAudio>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("No microphone found")?;
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
        .map_err(|e| format!("Failed to open the microphone: {e}"))
}
