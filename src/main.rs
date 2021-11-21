mod config;
mod danmaku;
mod dmlive;
mod ffmpeg;
mod ipcmanager;
mod mpv;
mod streamer;
mod streamfinder;
mod utils;

use std::sync::Arc;

use clap::Arg;
use log::*;
use tokio::{
    runtime::Builder,
    task,
};

fn main() {
    let ma = clap::App::new("dmlive")
        .arg(Arg::with_name("url").short("u").long("url").value_name("STRING").required(true).takes_value(true))
        .arg(Arg::with_name("log-level").long("log-level").required(false).takes_value(true))
        .arg(Arg::with_name("http-address").long("http-address").required(false).takes_value(true))
        .arg(Arg::with_name("record").short("r").long("record").required(false))
        .arg(Arg::with_name("quiet").short("q").long("quiet").required(false))
        .arg(Arg::with_name("plive").long("plive").hidden(true))
        .arg(
            Arg::with_name("wait-interval")
                .short("w")
                .long("wait-interval")
                .value_name("SECOND")
                .required(false)
                .takes_value(true),
        )
        .version(clap::crate_version!())
        .get_matches();
    let log_level = match ma.value_of("log-level").unwrap_or("3").parse().unwrap_or(3) {
        1 => LevelFilter::Debug,
        2 => LevelFilter::Info,
        3 => LevelFilter::Warn,
        4 => LevelFilter::Error,
        _ => LevelFilter::Info,
    };
    env_logger::Builder::new().filter(None, log_level).init();

    Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
        let local = task::LocalSet::new();
        local
            .run_until(async move {
                let proj_dirs = directories::ProjectDirs::from("com", "THMonster", "dmlive").unwrap();
                let d = proj_dirs.config_dir();
                let _ = tokio::fs::create_dir_all(&d).await;
                let config_path = d.join("config.toml");
                if !config_path.exists() {
                    let _ = tokio::fs::File::create(&config_path).await;
                }
                let cm = Arc::new(crate::config::ConfigManager::new(config_path, &ma));
                let dml = dmlive::DMLive::new(cm).await;
                let dml = Arc::new(dml);
                dml.run().await;
            })
            .await;
    })
}
