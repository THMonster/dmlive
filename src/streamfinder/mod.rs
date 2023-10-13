pub mod bilibili;
pub mod douyu;
pub mod huya;
pub mod twitch;
pub mod youtube;

use crate::ipcmanager::IPCManager;
use crate::{config::ConfigManager, dmlive::DMLMessage};
use anyhow::anyhow;
use anyhow::Result;
use log::info;
use log::warn;
use std::rc::Rc;

#[allow(unused)]
pub struct StreamFinder {
    ipc_manager: Rc<IPCManager>,
    cm: Rc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl StreamFinder {
    pub fn new(cm: Rc<ConfigManager>, im: Rc<IPCManager>, mtx: async_channel::Sender<DMLMessage>) -> Self {
        Self {
            ipc_manager: im,
            cm,
            mtx,
        }
    }

    // pub async fn run_bilivideo(&self, page: usize) -> Result<(String, Vec<String>)> {
    //     let b = bilibili::Bilibili::new(self.cm.clone());
    //     let mut u = b.get_video(page).await?;
    //     Ok((u.remove(0), u))
    // }

    pub async fn run(&self) -> Result<(String, Vec<String>)> {
        loop {
            for _ in 0..20 {
                match self.cm.site {
                    crate::config::Site::BiliLive => {
                        let b = bilibili::Bilibili::new(self.cm.clone());
                        match b.get_live(&self.cm.room_url).await {
                            Ok(u) => {
                                return Ok((u["title"].to_string(), vec![u["url"].to_string()]));
                            }
                            Err(e) => {
                                info!("{}", e);
                            }
                        };
                    }
                    crate::config::Site::BiliVideo => {
                        let b = bilibili::Bilibili::new(self.cm.clone());
                        let p = self.cm.bvideo_info.borrow().current_page;
                        match b.get_video(p).await {
                            Ok(mut u) => {
                                return Ok((u.remove(0), u));
                            }
                            Err(e) => {
                                info!("{}", e);
                            }
                        };
                    }
                    crate::config::Site::DouyuLive => {
                        let b = douyu::Douyu::new();
                        match b.get_live(&self.cm.room_url).await {
                            Ok(u) => {
                                return Ok((u["title"].to_string(), vec![u["url"].to_string()]));
                            }
                            Err(e) => {
                                info!("{}", e);
                            }
                        };
                    }
                    crate::config::Site::HuyaLive => {
                        let b = huya::Huya::new();
                        match b.get_live(&self.cm.room_url).await {
                            Ok(u) => {
                                return Ok((u["title"].to_string(), vec![u["url"].to_string()]));
                            }
                            Err(e) => {
                                info!("{}", e);
                            }
                        };
                    }
                    crate::config::Site::TwitchLive => {
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
                    crate::config::Site::YoutubeLive => {
                        let b = youtube::Youtube::new();
                        match b.get_live(&self.cm.room_url).await {
                            Ok(u) => {
                                let a: Vec<String> = u["url"].split('\n').map(|x| x.to_string()).collect();
                                return Ok((u["title"].to_string(), a));
                            }
                            Err(e) => {
                                info!("{}", e);
                            }
                        };
                    }
                }
                warn!("real url not found, retry...");
                tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
            }
            if self.cm.wait_interval == 0 {
                break;
            } else {
                warn!("waiting for {} seconds...", self.cm.wait_interval);
                tokio::time::sleep(tokio::time::Duration::from_secs(self.cm.wait_interval)).await;
            }
        }
        Err(anyhow!("max retry, quit"))
    }
}
