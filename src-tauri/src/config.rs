use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedConfig {
    pub mic_device: String,
    pub loopback_device: String,
    pub virtual_mic_device: String,
}

impl SavedConfig {
    fn config_path() -> PathBuf {
        let mut path = std::env::current_exe()
            .map(|p| p.parent().map(|p| p.to_path_buf()).unwrap_or_default())
            .unwrap_or_default();
        path.push("config.json");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        let content = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, content)
    }
}
