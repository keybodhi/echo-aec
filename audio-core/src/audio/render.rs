use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleRate, Stream};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::aec::AecProcessor;

pub struct RenderHandle {
    stream: Stream,
}

impl RenderHandle {
    pub fn stop(self) {
        drop(self.stream);
    }
}

pub fn start_render(device_id: &str, aec: Arc<Mutex<AecProcessor>>) -> Result<RenderHandle> {
    let host = cpal::default_host();
    let device = host
        .input_devices()?
        .find(|d| d.name().unwrap_or_default() == device_id)
        .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))?;

    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;

    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut aec = aec.blocking_lock();
            aec.process_render(data, sample_rate, channels);
        },
        |err| {
            tracing::error!("Render stream error: {}", err);
        },
        None,
    )?;

    Ok(RenderHandle { stream })
}
