pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Stateful box-average downsampler to 16 kHz.
pub struct Resampler {
    ratio: f64,
    pos: f64,
    acc: f32,
    n: u32,
}

impl Resampler {
    pub fn new(src_rate: f64) -> Self {
        Self { ratio: src_rate / 16_000.0, pos: 0.0, acc: 0.0, n: 0 }
    }
    pub fn push(&mut self, mono: impl Iterator<Item = f32>, out: &mut Vec<f32>) {
        for s in mono {
            self.acc += s;
            self.n += 1;
            self.pos += 1.0;
            if self.pos >= self.ratio {
                self.pos -= self.ratio;
                out.push(self.acc / self.n as f32);
                self.acc = 0.0;
                self.n = 0;
            }
        }
    }
}

/// Energy VAD gate: true if the window contains speech-level energy.
pub fn is_speech(pcm: &[f32]) -> bool {
    const FRAME: usize = 320; // 20 ms @ 16 kHz
    let active = pcm.chunks(FRAME).filter(|f| rms(f) > 0.003).count();
    active * 10 >= pcm.len() / FRAME.max(1) // >=10% active frames
}

/// Returns the sample index of the start of the first silence gap >= `min_silence_ms`.
/// Use this to find natural sentence boundaries in accumulated audio.
pub fn find_silence_gap(pcm: &[f32], min_silence_ms: u32) -> Option<usize> {
    const FRAME: usize = 320; // 20 ms @ 16 kHz
    let min_frames = (min_silence_ms / 20).max(1) as usize;
    let mut run = 0usize;
    for (i, chunk) in pcm.chunks(FRAME).enumerate() {
        if rms(chunk) < 0.003 {
            run += 1;
            if run >= min_frames {
                return Some((i + 1 - run) * FRAME);
            }
        } else {
            run = 0;
        }
    }
    None
}

/// NLMS adaptive echo canceller: subtracts the filtered loopback reference
/// from the mic signal. Covers ~128 ms of echo path (2048 taps @ 16 kHz).
pub struct Aec {
    w: Vec<f32>,
    hist: Vec<f32>,
}

const TAPS: usize = 2048;

impl Aec {
    pub fn new() -> Self {
        Self { w: vec![0.0; TAPS], hist: vec![0.0; TAPS] }
    }
    pub fn process(&mut self, mic: &mut [f32], reference: &[f32]) {
        const MU: f32 = 0.5;
        for (i, m) in mic.iter_mut().enumerate() {
            self.hist.rotate_right(1);
            self.hist[0] = reference.get(i).copied().unwrap_or(0.0);
            let y: f32 = self.w.iter().zip(&self.hist).map(|(a, b)| a * b).sum();
            let e = *m - y;
            let p: f32 = self.hist.iter().map(|v| v * v).sum::<f32>() + 1e-6;
            let g = MU * e / p;
            for (w, h) in self.w.iter_mut().zip(&self.hist) {
                *w += g * h;
            }
            *m = e;
        }
    }
}

/// Frame-slice dynamic ducking: processes audio in 320-sample chunks (20 ms @ 16 kHz).
/// If a reference frame RMS exceeds -24 dBFS (~0.0631), attenuates corresponding mic frame by -18 dB (~0.1259).
pub fn apply_ducking(mic: &mut [f32], reference: &[f32]) {
    const FRAME_SIZE: usize = 320;
    const DUCK_THRESHOLD_RMS: f32 = 0.0631;
    const ATTENUATION_FACTOR: f32 = 0.1259;

    for (mic_chunk, ref_chunk) in mic.chunks_mut(FRAME_SIZE).zip(reference.chunks(FRAME_SIZE)) {
        if rms(ref_chunk) > DUCK_THRESHOLD_RMS {
            for s in mic_chunk.iter_mut() {
                *s *= ATTENUATION_FACTOR;
            }
        }
    }
}

