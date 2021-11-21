pub mod config;

use std::path::Path;

use tokio::sync::RwLock;

pub struct ConfigManager {
    pub toml_config: RwLock<config::Config>,
    pub room_url: String,
}

impl ConfigManager {
    pub fn new(config_path: impl AsRef<Path>, room_url: &str) -> Self {
        let c = std::fs::read(config_path).unwrap();
        let c = String::from_utf8_lossy(&c);
        let c = config::load_config(&c).unwrap();
        Self {
            toml_config: RwLock::new(c),
            room_url: room_url.into(),
        }
    }
}
