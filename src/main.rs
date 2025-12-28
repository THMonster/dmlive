// mod config;
// mod danmaku;
// mod dmlive;
// mod ffmpeg;
// mod ipcmanager;
// mod mpv;
// mod streamer;
// mod streamfinder;
// mod utils;

use clap::Parser;
use dmlive::config::{Args, ConfigManager};
use log::*;
use std::rc::Rc;
use tokio::runtime::Builder;

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
        let mut im = dmlive::ipcmanager::IPCManager::new(cm.clone());
        im.run().await.unwrap();
        let im = Rc::new(im);
        let (mtx, mrx) = async_channel::unbounded();
        let ctx = dmlive::dmlive::DMLContext { im, cm, mrx, mtx };
        let dml = dmlive::dmlive::DMLive::new(Rc::new(ctx)).await;
        dml.run().await;
    })
}
