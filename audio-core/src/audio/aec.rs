use anyhow::Result;
use std::sync::Arc;
use webrtc_audio_processing::Processor;
use webrtc_audio_processing_config::{Config, EchoCanceller};

pub const SAMPLE_RATE: u32 = 48000;
pub const CHANNELS: u16 = 1;
pub const FRAME_SIZE: usize = 480;

pub struct AecProcessor {
    inner: Arc<Processor>,
}

impl AecProcessor {
    pub fn new() -> Result<Self> {
        Self::with_delay(100) // Default 100ms delay estimation
    }

    pub fn with_delay(delay_ms: u16) -> Result<Self> {
        let processor = Processor::new(SAMPLE_RATE)?;

        let mut config = Config::default();
        
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: Some(delay_ms),
        });
        
        config.noise_suppression = None;  // Disable NS to preserve near-end signal

        processor.set_config(config);

        Ok(Self {
            inner: Arc::new(processor),
        })
    }

    pub fn process_capture_frame(&self, channels: &mut Vec<Vec<f32>>) -> Result<()> {
        self.inner.process_capture_frame(channels)?;
        Ok(())
    }

    pub fn process_render_frame(&self, channels: &mut Vec<Vec<f32>>) -> Result<()> {
        self.inner.process_render_frame(channels)?;
        Ok(())
    }
}

impl Clone for AecProcessor {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
