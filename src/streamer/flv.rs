use futures::pin_mut;
use log::info;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
    time::timeout,
};

pub struct FLV {
    url: String,
    stream_socket: String,
    referer: String,
    loading: Arc<AtomicBool>,
}

impl FLV {
    pub fn new(url: String, extra: String, loading: Arc<AtomicBool>) -> Self {
        let e: Vec<&str> = extra.split("\n").collect();
        FLV {
            url,
            stream_socket: e[0].to_string(),
            referer: e[1].to_string(),
            // stream_port: 11111,
            loading,
        }
    }

    async fn download(&self, mut stream: UnixStream) -> Result<(), Box<dyn std::error::Error>> {
        // let mut sq = 0;
        let client = reqwest::Client::builder().user_agent(crate::utils::gen_ua()).connect_timeout(tokio::time::Duration::from_secs(10)).build()?;
        let url = self.url.clone();
        let room_url = self.referer.clone();
        let loading = self.loading.clone();
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
                    println!("connection too slow");
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
        };
        let ts_task = async move {
            let mut resp = client.get(url).header("Referer", room_url).send().await?;
            loading.store(false, std::sync::atomic::Ordering::SeqCst);
            while let Some(chunk) = resp.chunk().await? {
                stream.write_all(&chunk).await?;
                fd1.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        pin_mut!(watchdog_task);
        pin_mut!(ts_task);
        let _ = futures::future::select(watchdog_task, ts_task).await;
        Ok(())
    }

    pub async fn run(&self, arc_self: Arc<FLV>) -> Result<(), Box<dyn std::error::Error>> {
        let mut listener = None;
        let _ = timeout(
            tokio::time::Duration::from_secs(10),
            tokio::fs::remove_file(&self.stream_socket),
        )
        .await?;
        for _ in 0..15 {
            match UnixListener::bind(&self.stream_socket) {
                Ok(it) => {
                    listener = Some(it);
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    continue;
                }
            };
        }
        let (stream, _) = timeout(
            tokio::time::Duration::from_secs(10),
            listener.ok_or("unix socket bind failed")?.accept(),
        )
        .await??;
        let self1 = arc_self.clone();
        match self1.download(stream).await {
            Ok(it) => it,
            Err(err) => {
                info!("flv download error: {:?}", err);
            }
        };
        info!("flv streamer exit");
        Ok(())
    }
}
