use std::{
    collections::{HashSet, LinkedList},
    convert::TryInto,
    sync::{atomic::AtomicBool, Arc},
};

use futures::pin_mut;
use log::info;
use reqwest::Client;
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
    sync::mpsc::Sender,
    time::timeout,
};

pub struct HLS {
    url: String,
    stream_socket: String,
    // stream_port: u16,
    loading: Arc<AtomicBool>,
}

impl HLS {
    pub fn new(url: String, extra: String, loading: Arc<AtomicBool>) -> Self {
        HLS {
            url,
            stream_socket: extra,
            // stream_port: 11111,
            loading,
        }
    }

    async fn decode_m3u8(m3u8: &str, old_urls: &mut HashSet<String>) -> Result<LinkedList<String>, Box<dyn std::error::Error>> {
        let lines: Vec<_> = m3u8.split("\n").collect();
        let mut sq = None;
        let mut urls = LinkedList::new();
        let mut i = 0;
        while i < lines.len() {
            if lines[i].starts_with("#EXT-X-MEDIA-SEQUENCE") {
                let re = regex::Regex::new(r#"#EXT-X-MEDIA-SEQUENCE: *([0-9]+)"#).unwrap();
                let t: u64 = re.captures(&lines[i]).ok_or("decode m3u8 err 1")?[1].parse()?;
                sq = Some(t);
            }
            if !lines[i].starts_with("#") {
                if !lines[i].trim().is_empty() {
                    urls.push_back(lines[i]);
                }
            }
            i += 1;
        }
        if sq.is_none() || urls.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "decode m3u8 failed",
            )));
        }
        let sq = sq.unwrap();
        let mut ret = LinkedList::new();
        if old_urls.is_empty() && sq != 0 {
            let u = urls.pop_back().unwrap();
            ret.push_front(u.to_owned());
            info!("{}, {:?}", m3u8, &ret);
        } else {
            while !urls.is_empty() {
                let u = urls.pop_back().unwrap();
                if old_urls.contains(u) {
                    old_urls.clear();
                    old_urls.insert(u.to_string());
                    break;
                } else {
                    ret.push_front(u.to_owned());
                }
            }
            while !urls.is_empty() {
                let u = urls.pop_back().unwrap();
                old_urls.insert(u.to_string());
            }
        }
        info!("hls: m3u8 sq: {}, new ts seg: {}", sq, ret.len());
        Ok(ret)
    }

    async fn download_m3u8(url: String, tx: Sender<String>, client: Arc<Client>, loading: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
        let mut old_urls = HashSet::new();
        let mut interval: u64 = 1500;
        let mut m3u8_retry = 3u8;
        let mut first = true;
        loop {
            let now = std::time::Instant::now();
            let mut urls = match client.get(&url).header("Connection", "keep-alive").send().await {
                Ok(it) => {
                    m3u8_retry = 3;
                    let resp = it.text().await?;
                    if first {
                        loading.store(false, std::sync::atomic::Ordering::SeqCst);
                        first = false;
                    }
                    Self::decode_m3u8(&resp, &mut old_urls).await?
                }
                Err(e) => {
                    if m3u8_retry > 0 {
                        m3u8_retry = m3u8_retry.saturating_sub(1);
                        continue;
                    } else {
                        return Err(e.into());
                    }
                }
            };
            info!("hls: interval: {}", interval);
            // info!("hls: {:?}", &url_fifo);

            let urls_len = urls.len();
            if urls_len > 1 {
                interval = interval.saturating_sub(100);
                if interval < 500 {
                    interval = 500;
                }
            } else if urls_len < 1 {
                interval += 100;
            }

            // old_urls.clear();
            while !urls.is_empty() {
                let u = urls.pop_front().unwrap();
                tx.send(u.clone()).await?;
                old_urls.insert(u);
            }

            if true {
                let elapsed: u64 = now.elapsed().as_millis().try_into()?;
                if elapsed < interval {
                    let sleep_time = interval - elapsed;
                    // info!("v sleep: {}", &sleep_time);
                    tokio::time::sleep(tokio::time::Duration::from_millis(sleep_time)).await;
                }
            }
        }
    }

    async fn download(&self, mut stream: UnixStream) -> Result<(), Box<dyn std::error::Error>> {
        // let mut sq = 0;
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(30);
        let client = reqwest::Client::builder().user_agent(crate::utils::gen_ua()).timeout(tokio::time::Duration::from_secs(15)).build()?;
        let client = Arc::new(client);
        let client1 = client.clone();
        let url = self.url.clone();
        let loading = self.loading.clone();
        let m3u8_task = async move {
            match Self::download_m3u8(url, tx, client1, loading).await {
                Ok(_) => {}
                Err(err) => {
                    info!("hls download m3u8 error: {:?}", err);
                }
            }
        };
        let ts_task = async move {
            while let Some(u) = rx.recv().await {
                let mut resp = client.get(u).header("Connection", "keep-alive").send().await?;
                while let Some(chunk) = resp.chunk().await? {
                    stream.write_all(&chunk).await?;
                }
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        pin_mut!(m3u8_task);
        pin_mut!(ts_task);
        let _ = futures::future::select(m3u8_task, ts_task).await;
        Ok(())
    }

    pub async fn run(&self, arc_self: Arc<HLS>) -> Result<(), Box<dyn std::error::Error>> {
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
                info!("hls download error: {:?}", err);
            }
        };
        info!("hls streamer exit");
        Ok(())
    }
}
