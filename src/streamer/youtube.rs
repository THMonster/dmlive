use std::{convert::TryInto, sync::Arc};

use log::info;
use reqwest::Response;
use tokio::io::AsyncWriteExt;

use crate::{config::ConfigManager, dmlive::DMLMessage};

async fn get_head_sq(resp: &Response) -> Result<usize, Box<dyn std::error::Error>> {
    let sq: usize = resp.headers().get("X-Head-Seqnum").ok_or("no x-head-seqnum")?.to_str()?.parse()?;
    Ok(sq)
}

pub struct Youtube {
    url_v: String,
    url_a: String,
    sq: usize,
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl Youtube {
    pub fn new(
        url_v: String,
        url_a: String,
        sq: usize,
        cm: Arc<ConfigManager>,
        im: Arc<crate::ipcmanager::IPCManager>,
        mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        Youtube {
            url_v,
            url_a,
            sq,
            ipc_manager: im,
            cm,
            mtx,
        }
    }

    pub async fn download_audio(&self, mut sq: usize) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval: u64 = 1000;
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:93.0) Gecko/20100101 Firefox/93.0")
            .timeout(tokio::time::Duration::from_secs(15))
            .build()?;
        let mut tcp_stream = self.ipc_manager.get_audio_socket().await?;
        loop {
            let u = format!("{}sq/{}", &self.url_a, &sq);
            // println!("a: {}", &sq);
            let now = std::time::Instant::now();
            let mut resp = client
                .get(u)
                .header("Connection", "keep-alive")
                .header("Referer", "https://www.youtube.com/")
                .send()
                .await?;
            let head_sq = get_head_sq(&resp).await?;

            if resp.status() != 200 {
                println!("audio stream error: {:?}", &resp.status());
                return Ok(());
            }
            if (head_sq - sq) > 1 {
                interval = interval.saturating_sub(100);
            } else if (head_sq - sq) < 1 {
                info!("a: {}, {}", &sq, &interval);
                info!("a: {:?}", resp.headers());
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                interval += 100;
                continue;
            }
            while let Some(chunk) = resp.chunk().await? {
                tcp_stream.write_all(&chunk).await?;
            }
            if (head_sq - sq) <= 1 {
                let elapsed: u64 = now.elapsed().as_millis().try_into()?;
                if elapsed < interval {
                    let sleep_time = interval - elapsed;
                    // info!("a sleep: {}", &sleep_time);
                    tokio::time::sleep(tokio::time::Duration::from_millis(sleep_time)).await;
                }
            }
            sq += 1;
        }
    }
    pub async fn download_video(&self, mut sq: usize) -> Result<(), Box<dyn std::error::Error>> {
        let mut first = true;
        let mut interval: u64 = 1000;
        let mut tcp_stream = self.ipc_manager.get_video_socket().await?;
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:93.0) Gecko/20100101 Firefox/93.0")
            .timeout(tokio::time::Duration::from_secs(15))
            .build()?;
        loop {
            let u = format!("{}sq/{}", &self.url_v, &sq);
            // println!("v: {}", &sq);
            let now = std::time::Instant::now();
            let mut resp = client
                .get(&u)
                .header("Connection", "keep-alive")
                .header("Referer", "https://www.youtube.com/")
                .send()
                .await?;
            let head_sq = get_head_sq(&resp).await?;

            if first == true {
                first = false;
                self.mtx.send(DMLMessage::StreamStarted).await?;
            }

            if resp.status() != 200 {
                info!("video stream error: {:?}", &resp.status());
                return Ok(());
            }
            if (head_sq - sq) > 1 {
                interval = interval.saturating_sub(100);
            } else if (head_sq - sq) < 1 {
                info!("v: {}, {}", &sq, &interval);
                info!("v: {:?}", resp.headers());
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                interval += 100;
                continue;
            }
            while let Some(chunk) = resp.chunk().await? {
                tcp_stream.write_all(&chunk).await?;
            }
            if (head_sq - sq) <= 1 {
                let elapsed: u64 = now.elapsed().as_millis().try_into()?;
                if elapsed < interval {
                    let sleep_time = interval - elapsed;
                    // info!("v sleep: {}", &sleep_time);
                    tokio::time::sleep(tokio::time::Duration::from_millis(sleep_time)).await;
                }
            }
            sq += 1;
        }
    }
    async fn get_dash_sq(&self) -> Option<usize> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:93.0) Gecko/20100101 Firefox/93.0")
            .timeout(tokio::time::Duration::from_secs(7))
            .build()
            .unwrap();

        let u = format!("{}sq/{}", &self.url_v, &self.sq);
        let resp = match client
            .get(&u)
            .header("Connection", "keep-alive")
            .header(
                "User-Agent",
                "Mozilla/5.0 (X11; Linux x86_64; rv:93.0) Gecko/20100101 Firefox/93.0",
            )
            .header("Referer", "https://www.youtube.com/")
            .send()
            .await
        {
            Ok(it) => it,
            Err(_) => return None,
        };
        info!("get sq: {:?}", resp.headers());
        match get_head_sq(&resp).await {
            Ok(it) => Some(it),
            Err(_) => None,
        }
    }

    pub async fn run(self: &Arc<Self>) {
        let sq = match self.get_dash_sq().await {
            Some(it) => it,
            None => {
                println!("youtube streamer get sq error");
                return;
            }
        };
        let s1 = self.clone();
        let vtask = async move {
            match s1.download_video(sq).await {
                Ok(_) => {}
                Err(err) => {
                    info!("youtube download video: {:?}", err);
                }
            }
        };
        let s2 = self.clone();
        let atask = async move {
            match s2.download_audio(sq).await {
                Ok(_) => {}
                Err(err) => {
                    info!("youtube download audio: {:?}", err);
                }
            }
        };
        let _ = futures::future::select(Box::pin(vtask), Box::pin(atask)).await;
        info!("youtube streamer exit");
    }
}
