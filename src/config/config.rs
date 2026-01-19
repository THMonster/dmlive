use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub bcookie: Option<String>,
    pub cookies_from_browser: Option<String>,
    pub danmaku_speed: Option<u64>,
    pub font_alpha: Option<f64>,
    pub font_scale: Option<f64>,
}

pub fn load_config(j: &str) -> Result<Config, std::io::Error> {
    let c: Config = toml::from_str(j).unwrap();
    Ok(c)
}

pub enum BVideoType {
    Video,
    Bangumi
}

pub struct BVideoInfo {
    pub base_url: String,
    pub video_type: BVideoType,
    pub current_page: usize,
    pub current_cid: String,
    pub plist: Vec<String>,
}
