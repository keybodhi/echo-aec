use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleRate, Stream};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::aec::AecProcessor;

pub struct CaptureHandle {
    stream: Stream,
}

impl CaptureHandle {
    pub fn stop(self) {
        drop(self.stream);
    }
}

pub fn start_capture(device_id: &str, aec: Arc<Mutex<AecProcessor>>) -> Result<CaptureHandle> {
    let host = cpal::default_host();
    let device = host
        .output_devices()?
        .find(|d| d.name().unwrap_or_default() == device_id)
        .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))?;

    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut aec = aec.blocking_lock();
            aec.process_capture(data, sample_rate, channels);
        },
        |err| {
            tracing::error!("Capture stream error: {}", err);
        },
        None,
    )?;

    stream.play()?;

    Ok(CaptureHandle { stream })
}
