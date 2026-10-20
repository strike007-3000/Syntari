mod dsp;
#[cfg(target_os = "macos")]
pub mod macos_sc_tap;
#[cfg(target_os = "macos")]
#[path = "loopback_macos.rs"]
mod loopback;
#[cfg(target_os = "windows")]
#[path = "loopback_windows.rs"]
mod loopback;

use crate::ai::{self, engine::FoundryEngine, Transcript};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const WINDOW_SECS: u64 = 5;
const MAX_BUF: usize = 16_000 * 30; // circular cap: 30 s per track, RAM only

#[derive(Default)]
pub struct Capture {
    pub mic: Mutex<VecDeque<f32>>,
    pub sys: Mutex<VecDeque<f32>>,
    pub mic_lvl: AtomicU32,
    pub sys_lvl: AtomicU32,
}

#[derive(Serialize)]
pub struct Levels {
    pub mic: f32,
    pub system: f32,
}

impl Capture {
    pub fn levels(&self) -> Levels {
        Levels {
            mic: f32::from_bits(self.mic_lvl.load(Ordering::Relaxed)),
            system: f32::from_bits(self.sys_lvl.load(Ordering::Relaxed)),
        }
    }
}

fn open(dev: &cpal::Device, cfg: &cpal::SupportedStreamConfig, cap: Arc<Capture>, is_mic: bool) -> Result<cpal::Stream, String> {
    if cfg.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("unsupported sample format {:?}", cfg.sample_format()));
    }
    let ch = cfg.channels() as usize;
    let mut rs = dsp::Resampler::new(cfg.sample_rate().0 as f64);
    let mut out = Vec::new();
    let s = dev
        .build_input_stream(
            &cfg.config(),
            move |d: &[f32], _| {
                out.clear();
                rs.push(d.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32), &mut out);
                let (buf, lvl) = if is_mic { (&cap.mic, &cap.mic_lvl) } else { (&cap.sys, &cap.sys_lvl) };
                lvl.store(dsp::rms(&out).to_bits(), Ordering::Relaxed);
                let mut b = buf.lock().unwrap();
                b.extend(out.iter().copied());
                let over = b.len().saturating_sub(MAX_BUF);
                b.drain(..over);
            },
            |e| eprintln!("audio stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    s.play().map_err(|e| e.to_string())?;
    Ok(s)
}

pub struct Session {
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
}

/// Builds an in-memory 16-bit PCM WAV byte buffer (RAM-only, zero disk footprint)
fn create_wav_buffer(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let spec_channels: u16 = 1;
    let spec_bits: u16 = 16;
    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * (spec_channels as u32) * (spec_bits as u32 / 8);
    let block_align = spec_channels * (spec_bits / 8);
    let data_len = num_samples * (spec_bits as u32 / 8);
    let riff_len = 36 + data_len;

    let mut buf = Vec::with_capacity(44 + data_len as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&riff_len.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // subchunk1 size
    buf.extend_from_slice(&1u16.to_le_bytes());  // PCM
    buf.extend_from_slice(&spec_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&spec_bits.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let s_i16 = (clamped * 32767.0) as i16;
        buf.extend_from_slice(&s_i16.to_le_bytes());
    }

    buf
}

pub fn start(
    app: AppHandle,
    cap: Arc<Capture>,
    tr: Arc<Transcript>,
    engine: Arc<tokio::sync::Mutex<FoundryEngine>>,
    t0: Instant,
) -> Result<Session, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let stop2 = stop.clone();

    let handle = std::thread::spawn(move || {
        let host = cpal::default_host();
        let mic = host
            .default_input_device()
            .ok_or_else(|| "no microphone found / permission denied".to_string())
            .and_then(|d| {
                let c = d.default_input_config().map_err(|e| e.to_string())?;
                open(&d, &c, cap.clone(), true)
            });
        let _mic = match mic {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };

        // System Loopback: On macOS, prioritize native ScreenCaptureKit audio tap without virtual drivers.
        #[cfg(target_os = "macos")]
        let mut _sc_tap = match macos_sc_tap::ScreenCaptureAudioTap::start(cap.clone()) {
            Ok(tap) => Some(tap),
            Err(e) => {
                eprintln!("[Syntari SCKit] SCKit tap not available ({e}), attempting device fallback");
                let _ = app.emit("engine-error", format!("ScreenCaptureKit notice: {e}"));
                None
            }
        };

        // Fallback to loopback device (Windows WASAPI or fallback virtual device)
        #[cfg(not(target_os = "macos"))]
        let _sys = loopback::device(&host).and_then(|(d, c)| open(&d, &c, cap.clone(), false));
        #[cfg(target_os = "macos")]
        let _sys = if _sc_tap.is_none() {
            loopback::device(&host).and_then(|(d, c)| open(&d, &c, cap.clone(), false)).ok()
        } else {
            None
        };

        let _ = tx.send(Ok(()));

        let mut aec = dsp::Aec::new();
        let mut last = Instant::now();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        loop {
            let stopping = stop2.load(Ordering::Relaxed);
            if stopping || last.elapsed() >= Duration::from_secs(WINDOW_SECS) {
                last = Instant::now();
                let now_ms = t0.elapsed().as_millis() as u64;
                let mut mic: Vec<f32> = cap.mic.lock().unwrap().drain(..).collect();
                let mut sys: Vec<f32> = cap.sys.lock().unwrap().drain(..).collect();

                if !sys.is_empty() {
                    dsp::apply_ducking(&mut mic, &sys);
                    aec.process(&mut mic, &sys);
                }

                for (speaker, pcm) in [("Remote", &mut sys), ("You", &mut mic)] {
                    if !pcm.is_empty() && dsp::is_speech(pcm) {
                        let duration_secs = pcm.len() as f64 / 16_000.0;
                        let end_secs = ai::ms_to_secs(now_ms);
                        let start_secs = (end_secs - duration_secs).max(0.0);

                        let wav_bytes = create_wav_buffer(pcm, 16_000);
                        let engine_clone = engine.clone();
                        let speaker_str = speaker.to_string();

                        let transcription_res = rt.block_on(async {
                            let guard = engine_clone.lock().await;
                            if guard.is_ready() {
                                guard.transcribe_buffer(&wav_bytes, &speaker_str).await
                            } else {
                                Err("Engine not ready".to_string())
                            }
                        });

                        if let Ok(chunks) = transcription_res {
                            for mut c in chunks {
                                c.start += start_secs;
                                c.end += start_secs;
                                tr.lock().unwrap().push(c.clone());
                                let _ = app.emit("transcript-chunk", serde_json::json!({
                                    "id": fastrand::u32(..),
                                    "speaker": c.speaker,
                                    "text": c.text,
                                    "ts": (c.start * 1000.0) as u64,
                                    "start": c.start,
                                    "end": c.end
                                }));
                            }
                        }
                    }
                    pcm.fill(0.0); // RAM-only purge: zero out raw audio immediately
                }
            }
            if stopping {
                #[cfg(target_os = "macos")]
                if let Some(mut tap) = _sc_tap.take() {
                    let _ = tap.stop();
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    rx.recv().map_err(|e| e.to_string())??;
    Ok(Session { stop, handle })
}

impl Session {
    pub fn stop(self) -> Result<(), String> {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.join().map_err(|_| "audio thread panicked".to_string())
    }
}
