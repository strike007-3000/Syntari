//! macOS: CoreAudio exposes no loopback input by default. Until a
//! ScreenCaptureKit tap is implemented, use a virtual loopback input device
//! (BlackHole / Loopback / Soundflower) if the user has routed system audio to it.
pub fn device(host: &cpal::Host) -> Result<(cpal::Device, cpal::SupportedStreamConfig), String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let d = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .find(|d| {
            d.name().map(|n| {
                let n = n.to_lowercase();
                n.contains("blackhole") || n.contains("loopback") || n.contains("soundflower")
            }).unwrap_or(false)
        })
        .ok_or("no loopback device found (ScreenCaptureKit tap not implemented)")?;
    let c = d.default_input_config().map_err(|e| e.to_string())?;
    Ok((d, c))
}
