pub mod config;

use std::path::Path;
use tokio::sync::RwLock;

pub enum StreamType {
    FLV,
    HLS,
    DASH,
}

pub struct ConfigManager {
    pub toml_config: RwLock<config::Config>,
    pub room_url: String,
    pub stream_type: RwLock<StreamType>,
}

impl ConfigManager {
    pub fn new(config_path: impl AsRef<Path>, room_url: &str) -> Self {
        let c = std::fs::read(config_path).unwrap();
        let c = String::from_utf8_lossy(&c);
        let c = config::load_config(&c).unwrap();
        Self {
            toml_config: RwLock::new(c),
            room_url: room_url.into(),
            stream_type: RwLock::new(StreamType::FLV),
        }
    }

    pub async fn set_stream_type(&self, url: &str) {
        if url.contains(".m3u8") {
            *self.stream_type.write().await = StreamType::HLS;
        } else if url.contains(".flv") {
            *self.stream_type.write().await = StreamType::FLV;
        } else {
            *self.stream_type.write().await = StreamType::DASH;
        }
    }
}
