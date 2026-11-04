mod deepgram;

pub use deepgram::DeepgramEngine;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

pub const TARGET_RATE: u32 = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    Me,
    Them,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptEvent {
    Partial { source: AudioSource, text: String },
    Final { source: AudioSource, text: String },
    SpeechStarted { source: AudioSource },
    Status { message: String },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct SpeechConfig {
    pub api_key: String,
    pub model: String,
    pub language: String,
}

/// One live transcription implementation. `speech-base` ships the empty engine.
/// Provider branches replace it with Deepgram or Phonon.
#[async_trait]
pub trait SpeechEngine: Send + Sync {
    fn id(&self) -> &'static str;

    async fn run_source(
        &self,
        source: AudioSource,
        config: SpeechConfig,
        audio: mpsc::Receiver<Vec<f32>>,
        events: mpsc::Sender<TranscriptEvent>,
        stop: watch::Receiver<bool>,
    ) -> Result<(), String>;
}

#[allow(dead_code)]
pub struct MissingEngine;

#[async_trait]
impl SpeechEngine for MissingEngine {
    fn id(&self) -> &'static str {
        "none"
    }

    async fn run_source(
        &self,
        _source: AudioSource,
        _config: SpeechConfig,
        _audio: mpsc::Receiver<Vec<f32>>,
        _events: mpsc::Sender<TranscriptEvent>,
        _stop: watch::Receiver<bool>,
    ) -> Result<(), String> {
        Err(
            "This build has no speech engine. Switch to the speech-deepgram or speech-phonon branch."
                .into(),
        )
    }
}
