use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub bcookie: Option<String>,
    pub danmaku_speed: Option<u16>,
    pub font_alpha: Option<f64>,
    pub font_scale: Option<f64>,
}

pub fn load_config(j: &str) -> Result<Config, std::io::Error> {
    let c: Config = toml::from_str(&j).unwrap();
    Ok(c)
}
