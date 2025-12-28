pub mod baha;
pub mod bilibili;
pub mod douyu;
pub mod huya;
pub mod twitch;
pub mod youtube;

use crate::dmlive::DMLContext;
use anyhow::Result;
use anyhow::anyhow;
use log::info;
use log::warn;
use std::collections::HashMap;
use std::rc::Rc;

#[allow(unused)]
pub struct StreamFinder {
    ctx: Rc<DMLContext>,
}

impl StreamFinder {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Self { ctx }
    }

    // pub async fn run_bilivideo(&self, page: usize) -> Result<(String, Vec<String>)> {
    //     let b = bilibili::Bilibili::new(self.cm.clone());
    //     let mut u = b.get_video(page).await?;
    //     Ok((u.remove(0), u))
    // }

    pub async fn run(&self) -> Result<HashMap<&str, String>> {
        loop {
            for _ in 0..20 {
                let stream_info = match self.ctx.cm.site {
                    crate::config::Site::BiliLive => {
                        let b = bilibili::Bilibili::new(self.ctx.clone());
                        b.get_live(&self.ctx.cm.room_url).await
                    }
                    crate::config::Site::BiliVideo => {
                        let b = bilibili::Bilibili::new(self.ctx.clone());
                        let p = self.ctx.cm.bvideo_info.borrow().current_page;
                        b.get_video(p).await
                    }
                    crate::config::Site::DouyuLive => {
                        let b = douyu::Douyu::new();
                        b.get_live(&self.ctx.cm.room_url).await
                    }
                    crate::config::Site::HuyaLive => {
                        let b = huya::Huya::new();
                        b.get_live(&self.ctx.cm.room_url).await
                    }
                    crate::config::Site::TwitchLive => {
                        let b = twitch::Twitch::new();
                        b.get_live(&self.ctx.cm.room_url).await
                    }
                    crate::config::Site::YoutubeLive => {
                        let b = youtube::Youtube::new();
                        b.get_live(&self.ctx.cm.room_url).await
                    }
                    crate::config::Site::BahaVideo => {
                        let b = baha::Baha::new(self.ctx.clone());
                        b.get_video().await
                    }
                };
                match stream_info {
                    Ok(it) => {
                        return Ok(it);
                    }
                    Err(e) => {
                        info!("{}", e);
                    }
                }
                warn!("real url not found, retry...");
                tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
            }
            if self.ctx.cm.wait_interval == 0 {
                break;
            } else {
                warn!("waiting for {} seconds...", self.ctx.cm.wait_interval);
                tokio::time::sleep(tokio::time::Duration::from_secs(self.ctx.cm.wait_interval)).await;
            }
        }
        Err(anyhow!("max retry, quit"))
    }
}
