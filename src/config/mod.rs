pub mod config;

use clap::ArgMatches;
use reqwest::Url;
use std::{
    path::Path,
    sync::atomic::AtomicBool,
};
use tokio::sync::RwLock;
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
};

use self::config::{
    BVideoInfo,
    BVideoType,
    Config,
};

pub enum RunMode {
    Play,
    Record,
}
pub enum StreamType {
    FLV,
    HLS,
    DASH,
}
pub enum Site {
    BiliLive,
    BiliVideo,
    DouyuLive,
    HuyaLive,
    TwitchLive,
    YoutubeLive,
}

pub struct ConfigManager {
    pub plat: u8,
    pub bcookie: String,
    pub plive: bool,
    pub quiet: bool,
    pub wait_interval: u64,
    pub font_scale: RwLock<f64>,
    pub font_alpha: RwLock<f64>,
    pub danmaku_speed: RwLock<u64>,
    pub room_url: String,
    pub http_address: Option<String>,
    pub run_mode: RunMode,
    pub site: Site,
    pub stream_type: RwLock<StreamType>,
    pub bvideo_info: RwLock<BVideoInfo>,
    on_writing: AtomicBool,
}

impl ConfigManager {
    pub fn new(config_path: impl AsRef<Path>, ma: &ArgMatches) -> Self {
        let mut plat = if cfg!(target_os = "linux") { 0 } else { 1 };
        if ma.is_present("tcp") {
            plat = 1;
        }
        let mut bvinfo = BVideoInfo {
            base_url: "".into(),
            video_type: BVideoType::Video,
            current_page: 1,
            plist: Vec::new(),
        };
        let c = std::fs::read(config_path).unwrap();
        let c = String::from_utf8_lossy(&c);
        let c = config::load_config(&c).unwrap();
        let room_url = ma.value_of("url").unwrap();
        let site = if room_url.contains("live.bilibili.com/") {
            Site::BiliLive
        } else if room_url.contains("bilibili.com/") {
            let u = Url::parse(&room_url).unwrap();
            for q in u.query_pairs() {
                if q.0.eq("p") {
                    bvinfo.current_page = q.1.parse().unwrap();
                }
            }
            let vid = u.path_segments().unwrap().filter(|x| !x.is_empty()).last().unwrap().to_string();
            if vid.starts_with("BV") {
                bvinfo.video_type = BVideoType::Video;
                bvinfo.base_url.push_str(format!("https://www.bilibili.com/video/{}", vid).as_str());
            } else {
                bvinfo.video_type = BVideoType::Bangumi;
                bvinfo.base_url.push_str(format!("https://www.bilibili.com/bangumi/play/{}", vid).as_str());
            }
            Site::BiliVideo
        } else if room_url.contains("douyu.com/") {
            Site::DouyuLive
        } else if room_url.contains("huya.com/") {
            Site::HuyaLive
        } else if room_url.contains("twitch.tv/") {
            Site::TwitchLive
        } else if room_url.contains("youtube.com/") {
            Site::YoutubeLive
        } else {
            panic!("unknown url")
        };
        let run_mode = if ma.is_present("record") {
            RunMode::Record
        } else if ma.value_of("http-address").is_some() {
            RunMode::Record
        } else {
            RunMode::Play
        };
        Self {
            room_url: room_url.replace("dmlive://", "https://"),
            stream_type: RwLock::new(StreamType::FLV),
            run_mode,
            site,
            font_scale: RwLock::new(c.font_scale.unwrap_or(1.0)),
            font_alpha: RwLock::new(c.font_alpha.unwrap_or(0.0)),
            danmaku_speed: RwLock::new(c.danmaku_speed.unwrap_or(8000)),
            bvideo_info: RwLock::new(bvinfo),
            bcookie: c.bcookie.unwrap_or("".into()),
            http_address: match ma.value_of("http-address") {
                Some(it) => Some(it.into()),
                None => None,
            },
            plive: ma.is_present("plive"),
            quiet: ma.is_present("quiet"),
            wait_interval: ma.value_of("wait-interval").unwrap_or("0").parse().unwrap(),
            on_writing: AtomicBool::new(false),
            plat,
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
        if matches!(self.site, Site::BiliVideo) {
            *self.stream_type.write().await = StreamType::DASH;
        }
    }

    pub async fn write_config(&self) -> anyhow::Result<()> {
        if self.on_writing.load(std::sync::atomic::Ordering::SeqCst) == false {
            self.on_writing.store(true, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            let proj_dirs = directories::ProjectDirs::from("com", "THMonster", "dmlive").unwrap();
            let d = proj_dirs.config_dir();
            let _ = tokio::fs::create_dir_all(&d).await;
            let config_path = d.join("config.toml");
            if !config_path.exists() {
                let _ = tokio::fs::File::create(&config_path).await;
            }
            {
                let mut f = OpenOptions::new().write(true).truncate(true).open(config_path).await?;
                f.write_all(
                    toml::to_string_pretty(&Config {
                        bcookie: Some(self.bcookie.clone()),
                        danmaku_speed: Some(*self.danmaku_speed.read().await),
                        font_alpha: Some(*self.font_alpha.read().await),
                        font_scale: Some(*self.font_scale.read().await),
                    })
                    .unwrap()
                    .as_bytes(),
                )
                .await?;
                f.sync_all().await?;
            }
            self.on_writing.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }
}
