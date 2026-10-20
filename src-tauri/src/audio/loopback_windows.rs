//! Windows: WASAPI loopback. cpal opens an *output* device as an input
//! stream with AUDCLNT_STREAMFLAGS_LOOPBACK.
pub fn device(host: &cpal::Host) -> Result<(cpal::Device, cpal::SupportedStreamConfig), String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let d = host.default_output_device().ok_or("no output device for loopback")?;
    let c = d.default_output_config().map_err(|e| e.to_string())?;
    Ok((d, c))
}
