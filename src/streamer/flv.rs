use crate::{config::ConfigManager, dmlive::DMLMessage, ipcmanager::DMLStream};
use futures::pin_mut;
use log::{info, warn};
use std::sync::{atomic::AtomicBool, Arc};
use tokio::io::AsyncWriteExt;

pub struct FLV {
    url: String,
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl FLV {
    pub fn new(
        url: String,
        cm: Arc<ConfigManager>,
        im: Arc<crate::ipcmanager::IPCManager>,
        mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        FLV {
            url,
            ipc_manager: im,
            cm,
            mtx,
        }
    }

    async fn download(&self, mut stream: Box<dyn DMLStream>) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let url = self.url.clone();
        let room_url = self.cm.room_url.clone();
        let feed_dog = Arc::new(AtomicBool::new(false));
        let fd1 = feed_dog.clone();
        let watchdog_task = async move {
            let mut cnt = 0u8;
            loop {
                if feed_dog.load(std::sync::atomic::Ordering::SeqCst) == false {
                    cnt += 1;
                } else {
                    cnt = 0;
                    feed_dog.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                if cnt > 10 {
                    warn!("connection too slow");
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
        };
        let ts_task = async move {
            let mut resp = client.get(url).header("Referer", room_url).send().await?;
            self.mtx.send(DMLMessage::StreamStarted).await?;
            while let Some(chunk) = resp.chunk().await? {
                match stream.write_all(&chunk).await {
                    Ok(it) => it,
                    Err(err) => {
                        info!("flv download error: {}", err);
                        return Ok(());
                    }
                };
                fd1.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        let _ = futures::future::select(Box::pin(watchdog_task), Box::pin(ts_task)).await;
        Ok(())
    }

    pub async fn run(self: &Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        match self.download(self.ipc_manager.get_stream_socket().await?).await {
            Ok(it) => it,
            Err(err) => {
                info!("flv download error: {:?}", err);
            }
        };
        info!("flv streamer exit");
        Ok(())
    }
}
