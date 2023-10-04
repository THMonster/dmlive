use crate::{config::ConfigManager, dmlive::DMLMessage, ipcmanager::DMLStream, streamer::segment::SegmentStream};
use log::info;
use reqwest::Client;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    ops::Not,
    sync::Arc,
};
use tokio::{io::AsyncWriteExt, sync::mpsc::Receiver};

#[derive(Debug)]
pub struct M3U8 {
    sequence: u64,
    props: HashMap<String, Vec<String>>,
    hearder: String,
    clips: VecDeque<String>,
}

pub struct HLS {
    url: String,
    header: RefCell<Vec<u8>>,
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl HLS {
    pub fn new(
        url: String, cm: Arc<ConfigManager>, im: Arc<crate::ipcmanager::IPCManager>,
        mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        HLS {
            url,
            ipc_manager: im,
            cm,
            mtx,
            header: RefCell::new(Vec::new()),
        }
    }

    pub fn decode_m3u8(m3u8_text: &str) -> anyhow::Result<M3U8> {
        let mut lines = m3u8_text.lines();
        let mut sq = 0u64;
        let mut header = "".to_string();
        let mut m3u8_props = HashMap::new();
        let mut m3u8_clips = VecDeque::new();
        while let Some(line) = lines.next() {
            let line = line.trim();
            if line.starts_with("#") {
                if let Some((k, v)) = line.strip_prefix("#").unwrap().split_once(":") {
                    let k = k.trim();
                    let v = v.trim();
                    if k.eq("EXT-X-MEDIA-SEQUENCE") {
                        sq = v.parse().unwrap_or(0);
                    } else if k.eq("EXT-X-MAP") {
                        let (_, h) = v.split_once("=").unwrap_or(("", ""));
                        let h = h.trim().strip_prefix('"').and_then(|it| it.strip_suffix('"')).unwrap_or("").trim();
                        header.clear();
                        header.push_str(h);
                    } else {
                        m3u8_props
                            .entry(k.to_string())
                            .and_modify(|it: &mut Vec<String>| it.push(v.to_string()))
                            .or_insert(Vec::new());
                    }
                }
            } else {
                if line.is_empty().not() {
                    m3u8_clips.push_back(line.to_owned());
                }
            }
        }
        let m3u8 = M3U8 {
            sequence: sq,
            props: m3u8_props,
            clips: m3u8_clips,
            hearder: header,
        };
        // info!("hls: m3u8 data {:?}", &m3u8);
        Ok(m3u8)
    }

    fn parse_clip_url(&self, clip: &str) -> anyhow::Result<String> {
        let url = if clip.starts_with("http") {
            clip.to_string()
        } else {
            let url = url::Url::parse(&self.url)?;
            let url2 = url.join(&clip)?;
            if url2.as_str().contains("?") {
                url2.as_str().to_string()
            } else {
                format!("{}?{}", url2.as_str(), url.query().unwrap_or(""))
            }
        };
        Ok(url)
    }

    async fn download_header(&self, client: &Client, header_url: &str) -> anyhow::Result<()> {
        let url = self.parse_clip_url(&header_url)?;
        let mut resp = client.get(url).header("Connection", "keep-alive").send().await?;
        while let Some(chunk) = resp.chunk().await? {
            self.header.borrow_mut().write_all(&chunk).await?;
        }
        info!("hls: header length: {}", self.header.borrow().len());
        Ok(())
    }

    async fn download_task(
        &self, client: &Client, mut stream: Box<dyn DMLStream>, mut clip_rx: Receiver<String>,
    ) -> anyhow::Result<()> {
        let mut header_done = false;
        while let Some(clip) = clip_rx.recv().await {
            let url = self.parse_clip_url(&clip)?;
            // info!("hls: clip url: {}, m3u8 url: {}", &url, &self.url);
            info!("hls: clip: {}", &clip);
            let mut resp = client.get(url).header("Connection", "keep-alive").send().await?;
            if header_done.not() && self.header.borrow().is_empty().not() {
                stream.write_all(&self.header.borrow()).await?;
                header_done = true;
            }
            while let Some(chunk) = resp.chunk().await? {
                stream.write_all(&chunk).await?;
            }
        }
        Ok(())
    }

    async fn refresh_m3u8_task(
        &self, mut refresh_rx: Receiver<bool>, client: &Client, ss: &SegmentStream,
    ) -> anyhow::Result<()> {
        let mut header_done = false;
        while let Some(_) = refresh_rx.recv().await {
            let resp = client
                .get(&self.url)
                .header("Connection", "keep-alive")
                .header("X-Forwarded-For", "::1")
                .send()
                .await?;
            let m3u8_text = resp.text().await?;
            let m3u8 = Self::decode_m3u8(&m3u8_text)?;
            if header_done.not() && m3u8.hearder.is_empty().not() {
                self.download_header(client, &m3u8.hearder).await?;
                header_done = true;
            }
            ss.update_sequence(m3u8.sequence, m3u8.clips).await?;
        }
        Ok(())
    }

    pub async fn run(self: &Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .timeout(tokio::time::Duration::from_secs(15))
            .build()?;
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let (tx1, rx1) = tokio::sync::mpsc::channel(5);
        let seg_stream = SegmentStream::new(tx);
        // let  file = tokio::fs::File::create("/tmp/aaa.m4s").await?;
        // let file = Box::new(file);
        tokio::select! {
            it = self.refresh_m3u8_task(rx1, &client, &seg_stream) => { it?; },
            it = self.download_task(&client, self.ipc_manager.get_stream_socket().await?, rx) => { it?; },
            // it = self.download_task(&client, file, rx) => { it?; },
            it = seg_stream.run(tx1) => { it?; },
        }
        info!("hls streamer exit");
        Ok(())
    }
}
