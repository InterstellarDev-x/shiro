use crate::audio::{start_microphone, start_system_audio, LinearResampler, MicStream, RawAudio, SystemCapture};
use crate::stt::{AudioSource, SpeechConfig, SpeechEngine, TranscriptEvent, TARGET_RATE};
use crossbeam_channel::Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle as TaskHandle;

pub struct ListenSession {
    stop_flag: Arc<AtomicBool>,
    stop_tx: watch::Sender<bool>,
    mic: Option<MicStream>,
    system: Option<SystemCapture>,
    threads: Vec<JoinHandle<()>>,
    tasks: Vec<TaskHandle<()>>,
}

impl ListenSession {
    pub fn push_task(&mut self, task: TaskHandle<()>) {
        self.tasks.push(task);
    }

    pub async fn shutdown(mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.stop_tx.send(true);
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.mic.take();
        self.system.take();
        let threads = std::mem::take(&mut self.threads);
        let _ = tokio::task::spawn_blocking(move || {
            for thread in threads {
                let _ = thread.join();
            }
        })
        .await;
    }
}

pub async fn start_session(
    engine: Arc<dyn SpeechEngine>,
    config: SpeechConfig,
    events: mpsc::Sender<TranscriptEvent>,
) -> Result<ListenSession, String> {
    let (stop_tx, stop_rx) = watch::channel(false);
    let stop_flag = Arc::new(AtomicBool::new(false));

    let _ = events
        .send(TranscriptEvent::Status {
            message: "Allow microphone access if macOS asks.".into(),
        })
        .await;

    let (me_tx, me_rx) = mpsc::channel(32);
    let (them_tx, them_rx) = mpsc::channel(32);
    let (mic_raw_tx, mic_raw_rx) = crossbeam_channel::bounded::<RawAudio>(64);
    let (sys_raw_tx, sys_raw_rx) = crossbeam_channel::bounded::<RawAudio>(64);

    let mic = tokio::task::spawn_blocking(move || start_microphone(mic_raw_tx))
        .await
        .map_err(|error| format!("Microphone task failed: {error}"))??;
    let system = match tokio::task::spawn_blocking(move || start_system_audio(sys_raw_tx)).await {
        Ok(Ok(system)) => Some(system),
        Ok(Err(error)) => {
            eprintln!("[shiro] system audio unavailable: {error}");
            let _ = events
                .send(TranscriptEvent::Error {
                    message: error,
                })
                .await;
            None
        }
        Err(error) => {
            let message = format!("System audio task failed: {error}");
            eprintln!("[shiro] {message}");
            let _ = events
                .send(TranscriptEvent::Error { message })
                .await;
            None
        }
    };

    let mut threads = Vec::new();
    threads.push(spawn_bridge(mic_raw_rx, me_tx, stop_flag.clone()));

    let mut tasks = Vec::new();
    tasks.push(spawn_engine(
        engine.clone(),
        AudioSource::Me,
        config.clone(),
        me_rx,
        events.clone(),
        stop_rx.clone(),
    ));
    if system.is_some() {
        threads.push(spawn_bridge(sys_raw_rx, them_tx, stop_flag.clone()));
        tasks.push(spawn_engine(
            engine,
            AudioSource::Them,
            config,
            them_rx,
            events.clone(),
            stop_rx,
        ));
    }

    let _ = events
        .send(TranscriptEvent::Status {
            message: "Listening".into(),
        })
        .await;

    Ok(ListenSession {
        stop_flag,
        stop_tx,
        mic: Some(mic),
        system,
        threads,
        tasks,
    })
}

fn spawn_bridge(
    raw_rx: Receiver<RawAudio>,
    pcm_tx: mpsc::Sender<Vec<f32>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("shiro-resample".into())
        .spawn(move || {
            let mut resampler: Option<LinearResampler> = None;
            let mut rate = 0u32;
            let mut heard = false;
            while !stop.load(Ordering::Relaxed) {
                match raw_rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(chunk) => {
                        if !heard && chunk.samples.iter().any(|sample| sample.abs() > 0.02) {
                            heard = true;
                            eprintln!("[shiro] audio is arriving");
                        }
                        if resampler.is_none() || rate != chunk.sample_rate {
                            rate = chunk.sample_rate;
                            resampler = Some(LinearResampler::new(rate, TARGET_RATE));
                        }
                        let Some(resampler) = resampler.as_mut() else {
                            continue;
                        };
                        let pcm = resampler.push(&chunk.samples);
                        if pcm.is_empty() {
                            continue;
                        }
                        if pcm_tx.blocking_send(pcm).is_err() {
                            break;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("resample thread must start")
}

fn spawn_engine(
    engine: Arc<dyn SpeechEngine>,
    source: AudioSource,
    config: SpeechConfig,
    audio: mpsc::Receiver<Vec<f32>>,
    events: mpsc::Sender<TranscriptEvent>,
    stop: watch::Receiver<bool>,
) -> TaskHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = engine.run_source(source, config, audio, events.clone(), stop).await {
            eprintln!("[shiro] {source:?} transcription failed: {error}");
            let _ = events
                .send(TranscriptEvent::Error {
                    message: error,
                })
                .await;
        }
    })
}
