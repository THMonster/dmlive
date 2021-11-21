use std::{
    convert::TryInto,
    sync::{atomic::AtomicBool, Arc},
};

use futures::pin_mut;
use log::info;
use reqwest::Response;
use tokio::{io::AsyncWriteExt, net::TcpStream};

async fn get_head_sq(resp: &Response) -> Result<usize, Box<dyn std::error::Error>> {
    let sq: usize = resp.headers().get("X-Head-Seqnum").ok_or("no x-head-seqnum")?.to_str()?.parse()?;
    Ok(sq)
}

pub async fn download_video(url_v: String, stream_port: u16, mut sq: usize, loading: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    let mut first = true;
    let mut interval: u64 = 1000;
    let mut tcp_stream = None;
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:93.0) Gecko/20100101 Firefox/93.0")
        .timeout(tokio::time::Duration::from_secs(15))
        .build()?;
    for _ in 0..15 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        match TcpStream::connect(format!("127.0.0.1:{}", &stream_port)).await {
            Ok(it) => {
                tcp_stream = Some(it);
                break;
            }
            Err(_) => {
                continue;
            }
        };
    }
    if tcp_stream.is_none() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "cannot connect to ffmpeg tcp server",
        )));
    }
    loop {
        let u = format!("{}sq/{}", &url_v, &sq);
        // println!("v: {}", &sq);
        let now = std::time::Instant::now();
        let mut resp = client.get(&u).header("Connection", "keep-alive").header("Referer", "https://www.youtube.com/").send().await?;
        let head_sq = get_head_sq(&resp).await?;

        if first == true {
            first = false;
            loading.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        if resp.status() != 200 {
            println!("video stream error: {:?}", &resp.status());
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
            tcp_stream.as_mut().unwrap().write_all(&chunk).await?;
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

pub async fn download_audio(url_a: String, stream_port: u16, mut sq: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut interval: u64 = 1000;
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:93.0) Gecko/20100101 Firefox/93.0")
        .timeout(tokio::time::Duration::from_secs(15))
        .build()?;
    let mut tcp_stream = None;
    for _ in 0..15 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        match TcpStream::connect(format!("127.0.0.1:{}", &stream_port)).await {
            Ok(it) => {
                tcp_stream = Some(it);
                break;
            }
            Err(_) => {
                continue;
            }
        };
    }
    if tcp_stream.is_none() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "cannot connect to ffmpeg tcp server",
        )));
    }
    loop {
        let u = format!("{}sq/{}", &url_a, &sq);
        // println!("a: {}", &sq);
        let now = std::time::Instant::now();
        let mut resp = client.get(u).header("Connection", "keep-alive").header("Referer", "https://www.youtube.com/").send().await?;
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
            tcp_stream.as_mut().unwrap().write_all(&chunk).await?;
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
pub struct Youtube {
    url_v: String,
    url_a: String,
    sq: usize,
    stream_port: u16,
    loading: Arc<AtomicBool>,
}

impl Youtube {
    pub fn new(url: String, extra: String, loading: Arc<AtomicBool>) -> Self {
        let u: Vec<&str> = url.split("\n").collect();
        Youtube {
            url_v: u[0].to_string(),
            url_a: u[1].to_string(),
            sq: u[2].parse().unwrap_or(1),
            stream_port: extra.parse().unwrap(),
            loading,
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

    pub async fn run(&self) {
        let url_v = self.url_v.clone();
        let url_a = self.url_a.clone();
        let loading = self.loading.clone();
        let sq = match self.get_dash_sq().await {
            Some(it) => it,
            None => {
                println!("youtube streamer get sq error");
                return;
            }
        };
        let vtask = async move {
            match download_video(url_v, self.stream_port, sq, loading).await {
                Ok(_) => {}
                Err(err) => {
                    info!("youtube download video: {:?}", err);
                }
            }
        };
        let atask = async move {
            match download_audio(url_a, self.stream_port + 1, sq).await {
                Ok(_) => {}
                Err(err) => {
                    info!("youtube download audio: {:?}", err);
                }
            }
        };
        pin_mut!(vtask);
        pin_mut!(atask);
        let _ = futures::future::select(vtask, atask).await;
        info!("youtube streamer exit");
    }
}
