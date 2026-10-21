use crate::audio::{dsp, Capture, MAX_BUF};
use screencapturekit::{
    sc_content_filter::{InitParams, SCContentFilter},
    sc_error_handler::StreamErrorHandler,
    sc_shareable_content::SCShareableContent,
    sc_stream::SCStream,
    sc_stream_configuration::SCStreamConfiguration,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;

struct ErrorHandler;
impl StreamErrorHandler for ErrorHandler {
    fn on_error(&self) {
        eprintln!("[Syntari SCKit Error] Stream delegate reported stream didStopWithError!");
    }
}

use screencapturekit_sys::{
    cm_sample_buffer_ref::CMSampleBufferRef,
    os_types::rc::Id,
    stream::UnsafeSCStream,
    stream_output_handler::UnsafeSCStreamOutput,
};

struct AudioOutputHandler {
    tx: std::sync::mpsc::SyncSender<Vec<f32>>,
}

impl UnsafeSCStreamOutput for AudioOutputHandler {
    fn did_output_sample_buffer(&self, sample_buffer_ref: Id<CMSampleBufferRef>, of_type: u8) {
        // of_type 1 is Audio (0 is Screen)
        if of_type == 1 {
            if let Some(desc) = sample_buffer_ref.get_format_description() {
                static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !ONCE.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    let asbd = unsafe { screencapturekit_sys::cm_format_description_ref::CMAudioFormatDescriptionGetStreamBasicDescription(&*desc) };
                    if !asbd.is_null() {
                        let fmt = unsafe { *asbd };
                        eprintln!("[SCKit Format] rate: {}, format_id: {}, flags: 0x{:X}, bits: {}, ch: {}, bytes_per_frame: {}",
                            fmt.sample_rate, fmt.format_id, fmt.format_flags, fmt.bits_per_channel, fmt.channels_per_frame, fmt.bytes_per_frame);
                    }
                }
            }

            let audio_buffers = sample_buffer_ref.get_av_audio_buffer_list();
            if audio_buffers.is_empty() {
                return;
            }

            // Extract PCM samples from audio buffers
            // Note: If non-interleaved stereo, audio_buffers has 2 buffers (one per channel).
            let (raw_samples, num_channels) = if audio_buffers.len() >= 2 {
                let left_len = audio_buffers[0].data.len() / 4;
                let right_len = audio_buffers[1].data.len() / 4;
                let count = left_len.min(right_len);
                let mut mono = Vec::with_capacity(count);
                for i in 0..count {
                    let l_bytes: [u8; 4] = audio_buffers[0].data[i * 4..(i + 1) * 4].try_into().unwrap();
                    let r_bytes: [u8; 4] = audio_buffers[1].data[i * 4..(i + 1) * 4].try_into().unwrap();
                    let l = f32::from_ne_bytes(l_bytes);
                    let r = f32::from_ne_bytes(r_bytes);
                    mono.push((l + r) * 0.5);
                }
                (mono, 1)
            } else {
                let buffer = &audio_buffers[0];
                let num_bytes = buffer.data.len();
                let float_count = num_bytes / 4;
                if float_count == 0 {
                    return;
                }
                let mut samples = Vec::with_capacity(float_count);
                for chunk in buffer.data.chunks_exact(4) {
                    let sample = f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    samples.push(sample);
                }
                let ch = buffer.number_channels;
                (samples, ch)
            };

            let mono_samples: Vec<f32> = if num_channels == 2 || (num_channels != 1 && raw_samples.len() % 2 == 0) {
                raw_samples
                    .chunks_exact(2)
                    .map(|pair| (pair[0] + pair[1]) * 0.5)
                    .collect()
            } else {
                raw_samples
            };

            let mono_rms = dsp::rms(&mono_samples);
            if mono_rms > 0.0001 {
                eprintln!("[SCKit Tap] len: {}, mono_rms: {:.5}", mono_samples.len(), mono_rms);
            }

            // Non-blocking handoff from real-time CoreMedia thread to avoid priority inversion
            if let Err(e) = self.tx.try_send(mono_samples) {
                eprintln!("[SCKit Tap] Channel send warning: {e}");
            }
        }
    }
}

#[repr(transparent)]
struct SCStreamInternal {
    unsafe_ref: Id<UnsafeSCStream>,
}

pub struct ScreenCaptureAudioTap {
    stream: Id<UnsafeSCStream>,
}

impl ScreenCaptureAudioTap {
    pub fn start(cap: Arc<Capture>) -> Result<Self, String> {
        let content = SCShareableContent::try_current()
            .map_err(|e| format!("Failed to get ScreenCapture content: {e}"))?;

        let display = content
            .displays
            .first()
            .ok_or_else(|| "No display found for ScreenCaptureKit".to_string())?
            .clone();

        eprintln!("[SCKit Tap] Target display_id: {}, size: {}x{}", display.display_id, display.width, display.height);

        // Use the simplest Display filter — captures all system audio.
        // excludes_current_process_audio: false in config handles self-exclusion.
        // More complex filters (ExcludingApplications) can cause SCKit to deliver
        // zero-filled audio buffers instead of refusing the stream outright.
        let filter = SCContentFilter::new(InitParams::Display(display));

        let config = SCStreamConfiguration {
            width: 100,
            height: 100,
            captures_audio: true,
            sample_rate: 48_000,
            channel_count: 2,
            excludes_current_process_audio: false,
            minimum_frame_interval: screencapturekit::sc_types::base::CMTime {
                value: 1,
                timescale: 10,
                flags: screencapturekit::sc_types::base::CMTIME_FLAGS_VALID,
                epoch: 0,
            },
            ..Default::default()
        };

        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(256);
        let cap_clone = Arc::clone(&cap);
        std::thread::Builder::new()
            .name("sc-audio-resampler".into())
            .spawn(move || {
                let mut resampler = dsp::Resampler::new(48_000.0);
                let mut out = Vec::new();
                while let Ok(samples) = rx.recv() {
                    out.clear();
                    resampler.push(samples.into_iter(), &mut out);
                    if !out.is_empty() {
                        let rms_lvl = dsp::rms(&out);
                        cap_clone.sys_lvl.store(rms_lvl.to_bits(), Ordering::Relaxed);

                        let mut b = cap_clone.sys.lock().unwrap();
                        b.extend(out.iter().copied());
                        let cur_len = b.len();
                        let over = cur_len.saturating_sub(MAX_BUF);
                        b.drain(..over);
                        if rms_lvl > 0.001 && cur_len % 16_000 < out.len() {
                            eprintln!("[SCKit Tap] buf: {} samples, rms: {:.5}", cur_len, rms_lvl);
                        }
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn sc audio thread: {e}"))?;

        let sc_stream = SCStream::new(filter, config, ErrorHandler);
        // Transmute safe wrapper to extract the underlying Id<UnsafeSCStream>
        let internal: SCStreamInternal = unsafe { std::mem::transmute(sc_stream) };
        let stream = internal.unsafe_ref;

        let handler = AudioOutputHandler { tx };
        // Bypass screencapturekit 0.2's StreamOutputWrapper which panics on audio buffers
        // calling sys_ref.get_frame_info().status() on nil attachments.
        stream.add_stream_output(handler, 1); // 1 = Audio

        stream
            .start_capture()
            .map_err(|e| format!("Failed to start ScreenCaptureKit audio capture: {e:?}"))?;

        eprintln!("[Syntari SCKit] Native ScreenCaptureKit audio tap active.");
        Ok(Self { stream })
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.stream
            .stop_capture()
            .map_err(|e| format!("Failed to stop ScreenCaptureKit capture: {e:?}"))
    }
}
