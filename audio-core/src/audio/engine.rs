use anyhow::Result;
use flexaudio::{open, StreamConfig, SourceKind};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;

use super::aec::{AecProcessor, FRAME_SIZE};

const MAX_BUFFER_SIZE: usize = 96000;

pub struct AudioEngine {
    is_running: bool,
    running_flag: Arc<AtomicBool>,
    output_thread: Option<std::thread::JoinHandle<()>>,
    mic_thread: Option<std::thread::JoinHandle<()>>,
    loopback_thread: Option<std::thread::JoinHandle<()>>,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            is_running: false,
            running_flag: Arc::new(AtomicBool::new(false)),
            output_thread: None,
            mic_thread: None,
            loopback_thread: None,
        }
    }

    pub fn start(
        &mut self,
        mic_device: &str,
        loopback_device: &str,
        virtual_mic_device: &str,
    ) -> Result<()> {
        if self.is_running {
            self.stop();
        }

        let aec = AecProcessor::new()?;
        let running_flag = self.running_flag.clone();
        running_flag.store(true, Ordering::SeqCst);

        let audio_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let audio_buffer_clone = audio_buffer.clone();

        // Loopback thread: capture system audio -> process_render_frame directly
        // WebRTC AEC manages render buffer internally, no manual offset needed
        let aec_for_loopback = aec.clone();
        let loopback_flag = running_flag.clone();
        let loopback_device_id = loopback_device.to_string();
        let loopback_thread = std::thread::spawn(move || {
            let mut stream = match open(StreamConfig {
                kind: SourceKind::SystemLoopback,
                device_id: Some(loopback_device_id),
                ..Default::default()
            }) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to open loopback: {}", e);
                    return;
                }
            };

            if let Err(e) = stream.start() {
                tracing::error!("Failed to start loopback: {}", e);
                return;
            }

            tracing::info!("Loopback capture started");

            while loopback_flag.load(Ordering::SeqCst) {
                if let Some(chunk) = stream.poll_chunk() {
                    let samples = chunk.data;
                    let mono = stereo_to_mono(&samples);

                    // Feed render frames to AEC in 10ms chunks
                    for frame in mono.chunks(FRAME_SIZE) {
                        if frame.len() == FRAME_SIZE {
                            let mut ref_frame: Vec<Vec<f32>> = vec![frame.to_vec()];
                            let _ = aec_for_loopback.process_render_frame(&mut ref_frame);
                        }
                    }
                }
            }

            let _ = stream.stop();
            tracing::info!("Loopback capture stopped");
        });

        // Mic thread: capture mic -> process_capture_frame -> output buffer
        let aec_for_mic = aec.clone();
        let mic_flag = running_flag.clone();
        let mic_device_id = mic_device.to_string();
        let mic_thread = std::thread::spawn(move || {
            let mut stream = match open(StreamConfig {
                kind: SourceKind::Mic,
                device_id: Some(mic_device_id),
                ..Default::default()
            }) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to open mic: {}", e);
                    return;
                }
            };

            if let Err(e) = stream.start() {
                tracing::error!("Failed to start mic: {}", e);
                return;
            }

            tracing::info!("Mic capture started");

            while mic_flag.load(Ordering::SeqCst) {
                if let Some(chunk) = stream.poll_chunk() {
                    let samples = chunk.data;
                    let mono = stereo_to_mono(&samples);

                    // Process capture frames in 10ms chunks
                    for frame in mono.chunks(FRAME_SIZE) {
                        if frame.len() == FRAME_SIZE {
                            let mut mic_frame: Vec<Vec<f32>> = vec![frame.to_vec()];
                            if aec_for_mic.process_capture_frame(&mut mic_frame).is_ok() {
                                let mut buf = audio_buffer_clone.lock();
                                buf.extend_from_slice(&mic_frame[0]);

                                if buf.len() > MAX_BUFFER_SIZE {
                                    let excess = buf.len() - MAX_BUFFER_SIZE;
                                    buf.drain(..excess);
                                }
                            }
                        }
                    }
                }
            }

            let _ = stream.stop();
            tracing::info!("Mic capture stopped");
        });

        // Output thread: write processed audio to virtual mic
        let output_flag = running_flag.clone();
        let output_device_name = virtual_mic_device.to_string();
        let output_thread = std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.output_devices()
                .ok()
                .and_then(|devices| devices.into_iter().find(|d| d.name().unwrap_or_default() == output_device_name))
            {
                Some(d) => d,
                None => {
                    tracing::error!("Output device not found: {}", output_device_name);
                    return;
                }
            };

            let config = match device.default_output_config() {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to get output config: {}", e);
                    return;
                }
            };

            let channels = config.channels() as usize;

            let stream = match device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = audio_buffer.lock();
                    let frames_needed = data.len() / channels;

                    if buf.is_empty() {
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                        return;
                    }

                    let frames_available = buf.len();

                    for i in 0..frames_needed {
                        if i < frames_available {
                            let sample = buf[i];
                            for ch in 0..channels {
                                data[i * channels + ch] = sample;
                            }
                        } else {
                            for ch in 0..channels {
                                data[i * channels + ch] = 0.0;
                            }
                        }
                    }

                    let consumed = frames_needed.min(frames_available);
                    if consumed > 0 {
                        buf.drain(..consumed);
                    }
                },
                |err| tracing::error!("Output stream error: {}", err),
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to build output stream: {}", e);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                tracing::error!("Failed to play output stream: {}", e);
                return;
            }

            tracing::info!("Virtual mic output started");

            while output_flag.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            drop(stream);
            tracing::info!("Virtual mic output stopped");
        });

        self.output_thread = Some(output_thread);
        self.mic_thread = Some(mic_thread);
        self.loopback_thread = Some(loopback_thread);
        self.is_running = true;

        tracing::info!("Audio engine started");
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running_flag.store(false, Ordering::SeqCst);

        if let Some(thread) = self.output_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.mic_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.loopback_thread.take() {
            let _ = thread.join();
        }

        self.is_running = false;
        tracing::info!("Audio engine stopped");
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

fn stereo_to_mono(data: &[f32]) -> Vec<f32> {
    if data.len() < 2 {
        return data.to_vec();
    }
    data.chunks_exact(2)
        .map(|c| (c[0] + c[1]) * 0.5)
        .collect()
}
