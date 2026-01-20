pub mod config;

use crate::utils::is_android;
use clap::Parser;
use config::{BVideoInfo, BVideoType, Config};
use reqwest::Url;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use tokio::{fs::OpenOptions, io::AsyncWriteExt};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Set the http url
    #[clap(short = 'u', long, value_parser, value_name = "URL")]
    url: String,

    #[clap(short = 'r', long, action)]
    record: bool,

    #[clap(long = "download-dm", action)]
    download_dm: bool,

    #[clap(short = 'w', long = "wait-interval", value_parser)]
    wait_interval: Option<u64>,

    #[clap(long = "log-level", default_value_t = 3, value_parser)]
    pub log_level: u8,

    /// Serve as a http server
    #[clap(long = "http-address", value_parser)]
    http_address: Option<String>,

    /// Do not print danmaku
    #[clap(short = 'q', long, action)]
    quiet: bool,

    #[clap(long, action)]
    tcp: bool,

    #[clap(long, action)]
    plive: bool,
    // /// Use the Cookies that extracted from browser, could be "chrome" "chromium" or "firefox"
    // #[clap(long = "cookies-from-browser", value_parser)]
    // cookies_from_browser: Option<String>,
}

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

pub enum RecordMode {
    All,
    Danmaku,
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
    BahaVideo,
    DouyuLive,
    HuyaLive,
    TwitchLive,
    YoutubeLive,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SiteType {
    Live,
    Video,
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
    pub room_id: String,
    pub http_address: Option<String>,
    pub run_mode: RunMode,
    pub record_mode: RecordMode,
    pub site: Site,
    pub site_type: SiteType,
    pub stream_type: Cell<StreamType>,
    pub bvideo_info: RefCell<BVideoInfo>,
    pub title: RefCell<String>,
    pub stream_info: RefCell<HashMap<&'static str, String>>,
    on_writing: Cell<bool>,
}

impl ConfigManager {
    pub fn new(config_path: impl AsRef<Path>, args: &Args) -> Self {
        let mut plat = Platform::Linux;

        if args.tcp {
            plat = Platform::LinuxTcp;
        }

        let bvinfo = BVideoInfo {
            base_url: "".to_string(),
            video_type: BVideoType::Video,
            current_page: 0,
            current_cid: "".to_string(),
            plist: Vec::new(),
        };

        let c = std::fs::read(config_path).unwrap();
        let c = String::from_utf8_lossy(&c);
        let c = config::load_config(&c).unwrap();

        let run_mode = if args.record || args.http_address.is_some() || args.download_dm {
            RunMode::Record
        } else {
            RunMode::Play
        };
        let record_mode = if args.download_dm {
            RecordMode::Danmaku
        } else {
            RecordMode::All
        };

        Self {
            room_url: args.url.replace("dmlive://", "https://"),
            room_id: "".to_string(),
            stream_type: Cell::new(StreamType::FLV),
            run_mode,
            record_mode,
            site: Site::BiliLive,
            site_type: SiteType::Live,
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
            stream_info: RefCell::new(HashMap::new()),
        }
    }

    pub async fn init(&mut self) -> () {
        if is_android().await {
            self.plat = Platform::Android;
        }
        self.parse_url();
    }

    pub fn set_stream_type(&self, stream_info: &HashMap<&str, String>) {
        if stream_info["url"].contains(".m3u8") {
            if self.site == Site::BiliLive {
                self.stream_type.set(StreamType::HLS(1)); // for m4s inside
            } else {
                self.stream_type.set(StreamType::HLS(0)); // for ts inside
            }
        } else if stream_info["url"].contains(".flv") {
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

    pub fn parse_url(&mut self) -> () {
        let u = Url::parse(&self.room_url).unwrap();
        self.room_id = u.path_segments().unwrap().filter(|x| !x.is_empty()).last().unwrap().to_string();
        if self.room_url.contains("live.bilibili.com/") {
            self.site = Site::BiliLive;
            self.room_url = format!("https://live.bilibili.com/{}", self.room_id);
        } else if self.room_url.contains("bilibili.com/") {
            for q in u.query_pairs() {
                if q.0.eq("p") {
                    self.bvideo_info.borrow_mut().current_page = q.1.parse().unwrap();
                }
            }
            if self.room_id.starts_with("BV") || self.room_id.starts_with("av") {
                self.bvideo_info.borrow_mut().video_type = BVideoType::Video;
                self.bvideo_info.borrow_mut().base_url = format!("https://www.bilibili.com/video/{}", self.room_id);
            } else {
                self.bvideo_info.borrow_mut().video_type = BVideoType::Bangumi;
                self.bvideo_info.borrow_mut().base_url =
                    format!("https://www.bilibili.com/bangumi/play/{}", self.room_id);
            }
            self.site_type = SiteType::Video;
            self.site = Site::BiliVideo;
        } else if self.room_url.contains("ani.gamer.com.tw/") {
            for q in u.query_pairs() {
                if q.0.eq("p") {
                    self.bvideo_info.borrow_mut().current_page = q.1.parse().unwrap();
                }
            }
            self.site_type = SiteType::Video;
            self.site = Site::BahaVideo;
        } else if self.room_url.contains("douyu.com/") {
            self.site = Site::DouyuLive;
            self.room_url = format!("https://www.douyu.com/{}", self.room_id);
        } else if self.room_url.contains("huya.com/") {
            self.site = Site::HuyaLive;
            self.room_url = format!("https://www.huya.com/{}", self.room_id);
        } else if self.room_url.contains("twitch.tv/") {
            self.site = Site::TwitchLive;
            self.room_url = format!("https://www.twtich.tv/{}", self.room_id);
        } else if self.room_url.contains("youtube.com/") {
            self.site = Site::YoutubeLive;
            if self.room_url.contains("youtube.com/@") {
                self.room_id = u
                    .path_segments()
                    .unwrap()
                    .filter(|x| !x.is_empty())
                    .last()
                    .unwrap()
                    .strip_prefix("@")
                    .unwrap()
                    .to_string();
                self.room_url = format!("https://www.youtube.com/@{}/live", self.room_id);
            } else {
                self.room_id = u.query_pairs().find(|q| q.0.eq("v")).unwrap().1.to_string();
                self.room_url = format!("https://www.youtube.com/watch?v={}", self.room_id);
            };
        } else {
            panic!("unknown url")
        };
    }
}
