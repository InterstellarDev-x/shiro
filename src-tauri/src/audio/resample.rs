/// Streaming linear resampler. Speech models tolerate it, and it has no fixed chunk size.
pub struct LinearResampler {
    step: f64,
    pos: f64,
    pending: Vec<f32>,
}

impl LinearResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        let from = from_rate.max(1) as f64;
        let to = to_rate.max(1) as f64;
        Self {
            step: from / to,
            pos: 0.0,
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        self.pending.extend_from_slice(input);
        let mut out = Vec::new();
        while self.pos + 1.0 < self.pending.len() as f64 {
            let index = self.pos.floor() as usize;
            let frac = (self.pos - index as f64) as f32;
            let sample = self.pending[index] * (1.0 - frac) + self.pending[index + 1] * frac;
            out.push(sample);
            self.pos += self.step;
        }
        let drop_count = self.pos.floor() as usize;
        if drop_count > 0 && drop_count <= self.pending.len() {
            self.pending.drain(..drop_count);
            self.pos -= drop_count as f64;
        }
        out
    }
}

pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Decode interleaved PCM. `bytes_per_sample` is 4 for f32 or 2 for i16.
pub fn decode_interleaved(bytes: &[u8], channels: usize, frames: usize) -> Vec<f32> {
    if frames == 0 || channels == 0 || bytes.is_empty() {
        return Vec::new();
    }
    let bytes_per_sample = bytes.len() / (frames.saturating_mul(channels).max(1));
    let samples: Vec<f32> = match bytes_per_sample {
        4 => bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
        2 => bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_ne_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32)
            .collect(),
        _ => return Vec::new(),
    };
    downmix(&samples, channels)
}
