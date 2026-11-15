//! Crash-safe recording (opt-in): both tracks go to disk as raw 16 kHz mono 16-bit PCM, written where they belong on
//! the meeting timeline, so a crash loses at most the window being captured. The files let a meeting be transcribed again.
use super::{create_wav_buffer, dsp, echo, BLEED_RMS};
use crate::ai::engine::{FoundryEngine, TranscriptChunk};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const RATE: usize = 16_000;

pub fn dir(app_data: &Path, session: i64) -> PathBuf {
    app_data.join("recordings").join(session.to_string())
}

/// Bytes on disk for a meeting's recording (0 when there is none).
pub fn size(dir: &Path) -> u64 {
    ["me.pcm", "them.pcm"].iter().filter_map(|f| std::fs::metadata(dir.join(f)).ok()).map(|m| m.len()).sum()
}

/// Samples already saved (the longer track).
pub fn end_samples(dir: &Path) -> usize {
    (size_of(dir, "me.pcm").max(size_of(dir, "them.pcm")) / 2) as usize
}

fn size_of(dir: &Path, f: &str) -> u64 {
    std::fs::metadata(dir.join(f)).map(|m| m.len()).unwrap_or(0)
}

/// Saved recordings under the app data folder: (meeting id, bytes, seconds of audio, days since last written).
pub fn list(app_data: &Path) -> Vec<(i64, u64, u64, u64)> {
    let now = std::time::SystemTime::now();
    std::fs::read_dir(app_data.join("recordings")).into_iter().flatten().flatten().filter_map(|e| {
        let id = e.file_name().to_string_lossy().parse::<i64>().ok()?;
        let d = e.path();
        let age = ["me.pcm", "them.pcm"].iter().filter_map(|f| std::fs::metadata(d.join(f)).and_then(|m| m.modified()).ok())
            .max().and_then(|t| now.duration_since(t).ok()).map_or(0, |a| a.as_secs() / 86_400);
        Some((id, size(&d), end_samples(&d) as u64 / RATE as u64, age))
    }).collect()
}

pub struct Recorder {
    me: File,
    them: File,
}

fn open(path: PathBuf) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)
}

impl Recorder {
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self { me: open(dir.join("me.pcm"))?, them: open(dir.join("them.pcm"))? })
    }

    /// Writes `pcm` at sample `at` of the track. Failures are logged: a full disk must not stop the meeting.
    pub fn write(&mut self, remote: bool, at: usize, pcm: &[f32]) {
        let f = if remote { &mut self.them } else { &mut self.me };
        let bytes: Vec<u8> = pcm.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        if let Err(e) = f.seek(SeekFrom::Start(at as u64 * 2)).and_then(|_| f.write_all(&bytes)).and_then(|_| f.flush()) {
            eprintln!("[Recording] write failed: {e}");
        }
    }
}

fn read(path: PathBuf) -> Vec<f32> {
    let mut b = Vec::new();
    if let Ok(mut f) = File::open(path) { let _ = f.read_to_end(&mut b); }
    b.chunks_exact(2).map(|p| i16::from_le_bytes([p[0], p[1]]) as f32 / 32768.0).collect()
}

/// Transcribes a saved recording again, window by window, the way the live capture does.
pub async fn retranscribe(dir: &Path, engine: &FoundryEngine) -> Result<Vec<TranscriptChunk>, String> {
    if !engine.is_ready() { return Err("Engine not ready".into()) }
    let (mic, sys) = (read(dir.join("me.pcm")), read(dir.join("them.pcm")));
    let total = mic.len().max(sys.len());
    if total == 0 { return Err("No recording found for this meeting".into()) }
    let win = RATE * 12;
    let (mut aec, mut leak, mut out) = (dsp::Aec::new(), echo::Leak::default(), Vec::<TranscriptChunk>::new());
    let slice = |v: &[f32], a: usize, b: usize| -> Vec<f32> { v.get(a.min(v.len())..b.min(v.len())).unwrap_or(&[]).to_vec() };
    for begin in (0..total).step_by(win) {
        let end = begin + win;
        let (mut m, s) = (slice(&mic, begin, end), slice(&sys, begin, end));
        if dsp::rms(&s) > 0.003 && dsp::rms(&m) > 0.0006 { leak.add(dsp::envelope_corr(&s, &m)); }
        if !s.is_empty() { dsp::apply_ducking(&mut m, &s); aec.process(&mut m, &s); }
        let sys_active = dsp::rms(&s) > 0.01;
        let keep_mic = !(sys_active && dsp::rms(&m) < BLEED_RMS);
        for (speaker, pcm) in [("Remote", &s), ("You", &m)] {
            if (speaker == "You" && !keep_mic) || !dsp::is_speech(pcm) { continue }
            let (start, stop) = (begin as f64 / RATE as f64, (begin + pcm.len()) as f64 / RATE as f64);
            for mut c in engine.transcribe_buffer(&create_wav_buffer(pcm, RATE as u32), speaker).await? {
                if speaker == "You" {
                    let recent = out.iter().filter(|r| r.speaker == "Remote" && r.end >= start - 20.0).map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ");
                    if echo::is_echo(&c.text, &recent, leak.leaking()) { continue }
                }
                c.start = start;
                c.end = stop;
                out.push(c);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_land_on_the_timeline_and_read_back() {
        let d = std::env::temp_dir().join(format!("syntari_rec_test_{}", fastrand::u32(..)));
        let mut r = Recorder::open(&d).unwrap();
        r.write(false, 100, &[0.5, -0.5]);
        drop(r);
        let v = read(d.join("me.pcm"));
        assert_eq!(v.len(), 102);
        assert_eq!(v[99], 0.0);
        assert!((v[100] - 0.5).abs() < 1e-3 && (v[101] + 0.5).abs() < 1e-3);
        assert_eq!(size(&d), 204);
        let _ = std::fs::remove_dir_all(d);
    }
}
