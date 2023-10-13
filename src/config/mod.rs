pub mod config;

use self::config::{BVideoInfo, BVideoType, Config};
use crate::utils::is_android;
use crate::Args;
use reqwest::Url;
use std::cell::{Cell, RefCell};
use std::path::Path;
use tokio::{fs::OpenOptions, io::AsyncWriteExt};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    LinuxTcp,
    Android,
}
pub enum RunMode {
    Play,
    Record,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    FLV,
    HLS(usize),
    DASH,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Site {
    BiliLive,
    BiliVideo,
    DouyuLive,
    HuyaLive,
    TwitchLive,
    YoutubeLive,
}

pub struct ConfigManager {
    pub plat: Platform,
    pub bcookie: String,
    pub cookies_from_browser: String,
    pub plive: bool,
    pub quiet: bool,
    pub wait_interval: u64,
    pub font_scale: Cell<f64>,
    pub font_alpha: Cell<f64>,
    pub danmaku_speed: Cell<u64>,
    pub display_fps: Cell<(u64, u64)>,
    pub room_url: String,
    pub http_address: Option<String>,
    pub run_mode: RunMode,
    pub site: Site,
    pub stream_type: Cell<StreamType>,
    pub bvideo_info: RefCell<BVideoInfo>,
    pub title: RefCell<String>,
    on_writing: Cell<bool>,
}

impl ConfigManager {
    pub fn new(config_path: impl AsRef<Path>, args: &Args) -> Self {
        let mut plat = Platform::Linux;
        if args.tcp {
            plat = Platform::LinuxTcp;
        }
        let mut bvinfo = BVideoInfo {
            base_url: "".into(),
            video_type: BVideoType::Video,
            current_page: 0,
            plist: Vec::new(),
        };
        let c = std::fs::read(config_path).unwrap();
        let c = String::from_utf8_lossy(&c);
        let c = config::load_config(&c).unwrap();
        let room_url = args.url.clone();
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
            if vid.starts_with("BV") || vid.starts_with("av") {
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
        let run_mode = if args.record || args.http_address.is_some() {
            RunMode::Record
        } else {
            RunMode::Play
        };
        Self {
            room_url: room_url.replace("dmlive://", "https://"),
            stream_type: Cell::new(StreamType::FLV),
            run_mode,
            site,
            font_scale: Cell::new(c.font_scale.unwrap_or(1.0)),
            font_alpha: Cell::new(c.font_alpha.unwrap_or(0.0)),
            danmaku_speed: Cell::new(c.danmaku_speed.unwrap_or(8000)),
            bvideo_info: RefCell::new(bvinfo),
            bcookie: c.bcookie.unwrap_or_else(|| "".into()),
            http_address: args.http_address.as_ref().map(|it| it.into()),
            plive: args.plive,
            quiet: args.quiet,
            wait_interval: args.wait_interval.unwrap_or(0),
            on_writing: Cell::new(false),
            plat,
            cookies_from_browser: c.cookies_from_browser.unwrap_or_else(|| "".into()),
            display_fps: Cell::new((60, 0)),
            title: RefCell::new("".to_string()),
        }
    }

    pub async fn init(&mut self) -> anyhow::Result<()> {
        if is_android().await {
            self.plat = Platform::Android;
        }
        Ok(())
    }

    pub fn set_stream_type(&self, url: &str) {
        if url.contains(".m3u8") {
            if self.site == Site::BiliLive {
                self.stream_type.set(StreamType::HLS(1)); // for m4s inside
            } else {
                self.stream_type.set(StreamType::HLS(0)); // for ts inside
            }
        } else if url.contains(".flv") {
            self.stream_type.set(StreamType::FLV);
        } else {
            self.stream_type.set(StreamType::DASH);
        }
        if matches!(self.site, Site::BiliVideo) {
            self.stream_type.set(StreamType::DASH);
        }
    }

    pub async fn write_config(&self) -> anyhow::Result<()> {
        if !self.on_writing.get() {
            self.on_writing.set(true);
            // tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            let proj_dirs = directories::ProjectDirs::from("com", "THMonster", "dmlive").unwrap();
            let d = proj_dirs.config_dir();
            let _ = tokio::fs::create_dir_all(&d).await;
            let config_path = d.join("config.toml");
            if !config_path.exists() {
                let _ = tokio::fs::File::create(&config_path).await;
            }
            let mut f = OpenOptions::new().write(true).truncate(true).open(config_path).await?;
            f.write_all(
                toml::to_string_pretty(&Config {
                    bcookie: Some(self.bcookie.clone()),
                    cookies_from_browser: Some(self.cookies_from_browser.clone()),
                    danmaku_speed: Some(self.danmaku_speed.get()),
                    font_alpha: Some(self.font_alpha.get()),
                    font_scale: Some(self.font_scale.get()),
                })
                .unwrap()
                .as_bytes(),
            )
            .await?;
            f.sync_all().await?;
            self.on_writing.set(false);
        }
        Ok(())
    }
}
