mod mic;
mod resample;
mod system;

use crossbeam_channel::Sender;

pub use mic::MicStream;
pub use resample::{decode_interleaved, downmix, LinearResampler};
pub use system::SystemCapture;

/// Mono PCM at the device's native rate. The listen session resamples to 16 kHz.
pub struct RawAudio {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

pub fn start_microphone(tx: Sender<RawAudio>) -> Result<MicStream, String> {
    mic::start(tx)
}

pub fn start_system_audio(tx: Sender<RawAudio>) -> Result<SystemCapture, String> {
    system::start(tx)
}

pub fn log_microphone_access() {
    mic::log_microphone_access();
}
