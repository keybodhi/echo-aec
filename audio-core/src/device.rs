use anyhow::Result;
use flexaudio::{devices, SourceKind};
use cpal::traits::{HostTrait, DeviceTrait};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub source_kind: String,
}

pub struct DeviceManager {
    mic_devices: Vec<AudioDevice>,
    loopback_devices: Vec<AudioDevice>,
    output_devices: Vec<AudioDevice>,
}

impl DeviceManager {
    pub fn new() -> Result<Self> {
        let all_devices = devices()?;
        
        let mut mic_devices = Vec::new();
        let mut loopback_devices = Vec::new();

        for dev in all_devices {
            let device = AudioDevice {
                id: dev.id.clone(),
                name: dev.name.clone(),
                source_kind: format!("{:?}", dev.source_kind),
            };

            match dev.source_kind {
                SourceKind::Mic => mic_devices.push(device),
                SourceKind::SystemLoopback => loopback_devices.push(device),
                _ => {}
            }
        }

        let host = cpal::default_host();
        let mut output_devices = Vec::new();
        for device in host.output_devices()? {
            if let Ok(name) = device.name() {
                output_devices.push(AudioDevice {
                    id: name.clone(),
                    name,
                    source_kind: "Output".to_string(),
                });
            }
        }

        Ok(Self {
            mic_devices,
            loopback_devices,
            output_devices,
        })
    }

    pub fn mic_devices(&self) -> &[AudioDevice] {
        &self.mic_devices
    }

    pub fn loopback_devices(&self) -> &[AudioDevice] {
        &self.loopback_devices
    }

    pub fn output_devices(&self) -> &[AudioDevice] {
        &self.output_devices
    }
}
