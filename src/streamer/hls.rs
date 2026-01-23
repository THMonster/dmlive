use super::segment::MediaSegment;
use crate::{
    dmlive::{DMLContext, DMLMessage},
    streamer::segment::SegmentStream,
};
use log::info;
use reqwest::Client;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    ops::Not,
    rc::Rc,
};
use tokio::io::AsyncWriteExt;

#[allow(unused)]
#[derive(Debug)]
pub struct M3U8 {
    sequence: u64,
    target_duration: u64,
    props: HashMap<String, Vec<String>>,
    clips: VecDeque<MediaSegment>,
    streams: VecDeque<(isize, String)>,
}

// #[allow(unused)]
pub struct HLS {
    url: RefCell<String>,
    header_done: Cell<bool>,
    watch_dog: Cell<bool>,
    stream_ready: Cell<bool>,
    ctx: Rc<DMLContext>,
}

impl HLS {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        let url = ctx.cm.stream_info.borrow()["url"].to_string();
        HLS {
            url: RefCell::new(url),
            watch_dog: Cell::new(false),
            header_done: Cell::new(false),
            stream_ready: Cell::new(false),
            ctx,
        }
    }

    pub fn decode_m3u8(m3u8_text: &str) -> anyhow::Result<M3U8> {
        let mut lines = m3u8_text.lines();
        let mut sq = 0u64;
        let mut td = 5u64;
        let mut header = "".to_string();
        let mut m3u8_props = HashMap::new();
        let mut m3u8_clips = VecDeque::new();
        let mut m3u8_streams = VecDeque::new();
        let mut extinf = "".to_string();
        let mut ext_stream_inf = "".to_string();
        while let Some(line) = lines.next() {
            info!("{}", &line);
            let line = line.trim();
            if line.starts_with("#") {
                if let Some((k, v)) = line.strip_prefix("#").unwrap().split_once(":") {
                    let k = k.trim();
                    let v = v.trim();
                    if k.eq("EXT-X-MEDIA-SEQUENCE") {
                        sq = v.parse().unwrap_or(0);
                    } else if k.eq("EXT-X-TARGETDURATION") {
                        td = v.parse().unwrap_or(5);
                    } else if k.eq("EXT-X-MAP") {
                        let (_, h) = v.split_once("=").unwrap_or(("", ""));
                        let h = h.trim().strip_prefix('"').and_then(|it| it.strip_suffix('"')).unwrap_or("").trim();
                        header.clear();
                        header.push_str(h);
                    } else if k.eq("EXTINF") {
                        extinf.clear();
                        extinf.push_str(v);
                    } else if k.eq("EXT-X-STREAM-INF") {
                        ext_stream_inf.clear();
                        ext_stream_inf.push_str(v);
                    } else {
                        m3u8_props
                            .entry(k.to_string())
                            .and_modify(|it: &mut Vec<String>| it.push(v.to_string()))
                            .or_insert(vec![v.to_string()]);
                    }
                }
            } else {
                if line.is_empty().not() {
                    if ext_stream_inf.is_empty() {
                        let seg = MediaSegment {
                            skip: if extinf.contains("Amazon") { 1 } else { 0 },
                            props: HashMap::new(),
                            url: line.to_owned(),
                            is_header: false,
                        };
                        m3u8_clips.push_back(seg);
                        extinf.clear()
                    } else {
                        let bw = ext_stream_inf
                            .split(',')
                            .find_map(|a| {
                                if a.trim().starts_with("BANDWIDTH") {
                                    return a.split("=").find_map(|x| x.trim().parse::<isize>().ok());
                                }
                                None
                            })
                            .unwrap_or(1);
                        m3u8_streams.push_back((bw, line.to_owned()));
                        td = 1;
                        sq = 0;
                    }
                }
            }
        }
        if header.is_empty().not() {
            let seg = MediaSegment {
                skip: 1,
                props: HashMap::new(),
                url: header,
                is_header: true,
            };
            m3u8_clips.push_front(seg);
        }
        let m3u8 = M3U8 {
            sequence: sq,
            target_duration: td,
            props: m3u8_props,
            clips: m3u8_clips,
            streams: m3u8_streams,
        };
        // info!("m3u8: {:?}", &m3u8);
        Ok(m3u8)
    }

    fn parse_clip_url(&self, clip: &str) -> anyhow::Result<String> {
        let url = if clip.starts_with("http") {
            clip.to_string()
        } else {
            let url = url::Url::parse(self.url.borrow().as_str())?;
            let url2 = url.join(&clip)?;
            if url2.as_str().contains("?") {
                url2.as_str().to_string()
            } else {
                format!("{}?{}", url2.as_str(), url.query().unwrap_or(""))
            }
        };
        Ok(url)
    }

    async fn download_task(&self, client: &Client, ss: &SegmentStream) -> anyhow::Result<()> {
        let mut stream = self.ctx.im.get_video_socket().await?;
        let mut rx = ss.clip_rx.borrow_mut();
        while let Some(mut clip) = rx.recv().await {
            // info!("hls: clip: {}", &clip);
            if self.header_done.get().not() && clip.is_header {
                clip.skip = 0;
                self.header_done.set(true);
            } else if clip.skip == 2 {
                continue;
            }
            let url = self.parse_clip_url(&clip.url)?;
            let mut resp = client.get(url).header("Connection", "keep-alive").send().await?;
            info!("hls resp: {resp:?}");
            while let Some(chunk) = resp.chunk().await? {
                if clip.skip == 0 {
                    if !self.stream_ready.get() {
                        self.stream_ready.set(true);
                        let _ = self.ctx.mtx.send(DMLMessage::StreamReady).await;
                    }
                    stream.write_all(&chunk).await?;
                }
            }
            self.watch_dog.set(true);
        }
        Ok(())
    }

    async fn refresh_m3u8_task(&self, client: &Client, ss: &SegmentStream) -> anyhow::Result<()> {
        let mut rx = ss.refresh_rx.borrow_mut();
        while let Some(_) = rx.recv().await {
            let resp = client
                .get(self.url.borrow().as_str())
                .timeout(tokio::time::Duration::from_millis(ss.refresh_itvl.get()))
                .header("Connection", "keep-alive")
                .send()
                .await;
            let resp = match resp {
                Ok(it) => it,
                Err(e) => {
                    info!("{}", e);
                    continue;
                }
            };
            let m3u8_text = match resp.text().await {
                Ok(it) => it,
                Err(e) => {
                    info!("{}", e);
                    continue;
                }
            };
            let m3u8 = Self::decode_m3u8(&m3u8_text)?;
            if m3u8.streams.is_empty().not() {
                let (_, s) = m3u8.streams.iter().max_by(|a, b| a.0.cmp(&b.0)).unwrap();
                let s = self.parse_clip_url(s)?;
                *self.url.borrow_mut() = s;
            }
            ss.update_sequence(m3u8.sequence, m3u8.clips, m3u8.target_duration * 1000).await?;
        }
        Ok(())
    }

    async fn watch_dog_task(&self) -> anyhow::Result<()> {
        let mut cnt = 0;
        let max_waiting = match self.ctx.cm.site {
            crate::config::Site::TwitchLive => 30,
            _ => 10,
        };
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            if self.watch_dog.get().not() {
                cnt += 1;
            } else {
                cnt = 0;
            }
            if cnt > max_waiting {
                info!("watch dog failed!");
                return Err(anyhow::anyhow!("watch dog failed!"));
            }
            self.watch_dog.set(false);
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .timeout(tokio::time::Duration::from_secs(30))
            .build()?;
        let seg_stream = SegmentStream::new();
        tokio::select! {
            it = self.refresh_m3u8_task(&client, &seg_stream) => { it?; },
            it = self.download_task(&client, &seg_stream) => { it?; },
            it = self.watch_dog_task() => { it?; },
            it = seg_stream.run() => { it?; },
        }
        info!("hls streamer exit");
        Ok(())
    }
}
