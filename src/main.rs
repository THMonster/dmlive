mod config;
mod danmaku;
mod dmlive;
mod ffmpeg;
mod ipcmanager;
mod mpv;
mod streamer;
mod streamfinder;
mod utils;

use crate::config::ConfigManager;
use clap::Parser;
use log::*;
use std::rc::Rc;
use tokio::runtime::Builder;

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
    log_level: u8,

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

fn main() {
    let args = Args::parse();
    let log_level = match args.log_level {
        1 => LevelFilter::Debug,
        2 => LevelFilter::Info,
        3 => LevelFilter::Warn,
        4 => LevelFilter::Error,
        _ => LevelFilter::Info,
    };
    // rustls::crypto::ring::default_provider().install_default().expect("Failed to install default rustls crypto provider");
    env_logger::Builder::new().filter(None, log_level).init();

    Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
        let proj_dirs = directories::ProjectDirs::from("com", "THMonster", "dmlive").unwrap();
        let d = proj_dirs.config_dir();
        let _ = tokio::fs::create_dir_all(&d).await;
        let config_path = d.join("config.toml");
        if !config_path.exists() {
            let _ = tokio::fs::File::create(&config_path).await;
        }
        let mut cm = ConfigManager::new(config_path, &args);
        cm.init().await.unwrap();
        let cm = Rc::new(cm);
        let mut im = ipcmanager::IPCManager::new(cm.clone());
        im.run().await.unwrap();
        let im = Rc::new(im);
        let dml = dmlive::DMLive::new(cm, im).await;
        dml.run().await;
    })
}
