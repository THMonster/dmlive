use crate::{
    config::{ConfigManager, Site},
    dmlive::DMLMessage,
    ipcmanager::IPCManager,
};
use log::{info, warn};
use std::{cell::Cell, rc::Rc};
use tokio::io::AsyncWriteExt;

#[allow(unused)]
pub struct FLV {
    url: String,
    ipc_manager: Rc<IPCManager>,
    cm: Rc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl FLV {
    pub fn new(url: String, cm: Rc<ConfigManager>, im: Rc<IPCManager>, mtx: async_channel::Sender<DMLMessage>) -> Self {
        FLV {
            url,
            ipc_manager: im,
            cm,
            mtx,
        }
    }

    async fn download(&self) -> anyhow::Result<()> {
        let mut stream = self.ipc_manager.get_video_socket().await?;
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let url = self.url.clone();
        let room_url = self.cm.room_url.clone();
        let watch_dog = Cell::new(0);
        let watchdog_task = async {
            loop {
                watch_dog.set(watch_dog.get() + 1);
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                if watch_dog.get() > 10 {
                    warn!("connection too slow");
                    break;
                }
            }
        };
        let dl_task = async {
            let mut resp = client.get(url).header("Referer", room_url);
            if self.cm.plive && matches!(self.cm.site, Site::BiliLive) {
                resp = resp.header("Cookie", self.cm.bcookie.as_str());
            }
            let mut resp = resp.send().await?;
            while let Some(chunk) = resp.chunk().await? {
                stream.write_all(&chunk).await?;
                watch_dog.set(0);
            }
            info!("flv downloader exit normally");
            anyhow::Ok(())
        };
        tokio::select! {
            it = dl_task => { it?; }
            _ = watchdog_task => {}
        }
        Ok(())
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        self.download().await?;
        info!("flv streamer exit");
        Ok(())
    }
}
