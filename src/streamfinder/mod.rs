pub mod bilibili;
pub mod douyu;
pub mod huya;
pub mod twitch;
pub mod youtube;

use crate::{config::ConfigManager, dmlive::DMLMessage};
use anyhow::*;
use log::info;
use std::sync::Arc;

pub struct StreamFinder {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl StreamFinder {
    pub fn new(
        cm: Arc<ConfigManager>,
        im: Arc<crate::ipcmanager::IPCManager>,
        mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        Self {
            ipc_manager: im,
            cm,
            mtx,
        }
    }

    pub async fn run(self: &Arc<Self>) -> Result<(String, Vec<String>)> {
        for _ in 0..20 {
            if self.cm.room_url.contains("live.bilibili.com") {
                let b = bilibili::Bilibili::new();
                match b.get_live(&self.cm.room_url).await {
                    Ok(u) => {
                        return Ok((u["title"].to_string(), vec![u["url"].to_string()]));
                    }
                    Err(e) => {
                        info!("{}", e);
                    }
                };
            } else if self.cm.room_url.contains("bilibili.com/") {
                let b = bilibili::Bilibili::new();
                match b.get_video(&self.cm.room_url, "").await {
                    Ok(mut u) => {
                        return Ok((u.remove(0), u));
                    }
                    Err(e) => {
                        info!("{}", e);
                    }
                };
            } else if self.cm.room_url.contains("douyu.com") {
                let b = douyu::Douyu::new();
                match b.get_live(&self.cm.room_url).await {
                    Ok(u) => {
                        return Ok((u["title"].to_string(), vec![u["url"].to_string()]));
                    }
                    Err(e) => {
                        info!("{}", e);
                    }
                };
            } else if self.cm.room_url.contains("huya.com") {
                let b = huya::Huya::new();
                match b.get_live(&self.cm.room_url).await {
                    Ok(u) => {
                        return Ok((u["title"].to_string(), vec![u["url"].to_string()]));
                    }
                    Err(e) => {
                        info!("{}", e);
                    }
                };
            } else if self.cm.room_url.contains("youtube.com/") {
                let b = youtube::Youtube::new();
                match b.get_live(&self.cm.room_url).await {
                    Ok(u) => {
                        let a: Vec<String> = u["url"].split("\n").map(|x| x.to_string()).collect();
                        return Ok((u["title"].to_string(), a));
                    }
                    Err(e) => {
                        info!("{}", e);
                    }
                };
            } else if self.cm.room_url.contains("twitch.tv/") {
                let b = twitch::Twitch::new();
                match b.get_live(&self.cm.room_url).await {
                    Ok(u) => {
                        return Ok((u["title"].to_string(), vec![u["url"].to_string()]));
                    }
                    Err(e) => {
                        info!("{}", e);
                    }
                };
            }
            println!("real url not found, retry...");
            tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
        }
        Err(anyhow!("max retry, quit"))
    }
}
