use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use std::sync::Arc;
use parking_lot::Mutex;
use ringbuf::traits::{Consumer, Producer, SplitProducer, SplitConsumer};
use ringbuf::heap::HeapRb;

use super::aec::AecProcessor;

pub struct AudioStreams {
    _mic_capture: Stream,
    _virtual_mic_output: Stream,
}

impl AudioStreams {
    pub fn stop(self) {
        drop(self._mic_capture);
        drop(self._virtual_mic_output);
    }
}

pub fn start_all_streams(
    real_mic_name: &str,
    virtual_mic_name: &str,
    aec: AecProcessor,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioStreams> {
    let host = cpal::default_host();

    let real_mic = host.input_devices()?
        .find(|d| d.name().unwrap_or_default() == real_mic_name)
        .ok_or_else(|| anyhow::anyhow!("Real mic not found: {}", real_mic_name))?;

    let virtual_mic = host.output_devices()?
        .find(|d| d.name().unwrap_or_default() == virtual_mic_name)
        .ok_or_else(|| anyhow::anyhow!("Virtual mic not found: {}", virtual_mic_name))?;

    let mic_config = real_mic.default_input_config()?;

    let buf_size = (sample_rate * channels as u32) as usize * 2;

    let (mic_to_vmic_producer, mic_to_vmic_consumer) = HeapRb::<f32>::new(buf_size).split();
    let mic_to_vmic_producer = Arc::new(Mutex::new(mic_to_vmic_producer));
    let mic_to_vmic_consumer = Arc::new(Mutex::new(mic_to_vmic_consumer));

    let aec_for_mic = aec.clone();
    let mic_prod = mic_to_vmic_producer.clone();
    let mic_capture = real_mic.build_input_stream(
        &mic_config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut processed = data.to_vec();
            let _ = aec_for_mic.process_capture(&mut processed);
            let mut prod = mic_prod.lock();
            for &sample in &processed {
                let _ = prod.try_push(sample);
            }
        },
        |err| tracing::error!("Mic capture error: {}", err),
        None,
    )?;

    let vmic_cons = mic_to_vmic_consumer.clone();
    let vmic_config = virtual_mic.default_output_config()?;
    let virtual_mic_output = virtual_mic.build_output_stream(
        &vmic_config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut cons = vmic_cons.lock();
            for sample in data.iter_mut() {
                *sample = cons.try_pop().unwrap_or(0.0);
            }
        },
        |err| tracing::error!("Virtual mic output error: {}", err),
        None,
    )?;

    mic_capture.play()?;
    virtual_mic_output.play()?;

    Ok(AudioStreams {
        _mic_capture: mic_capture,
        _virtual_mic_output: virtual_mic_output,
    })
}
