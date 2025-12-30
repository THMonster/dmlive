use crate::{
    dmlerr,
    dmlive::{DMLContext, DMLMessage},
    ipcmanager::DMLStream,
    streamer::segment::{MediaSegment, SegmentStream},
    streamfinder,
};
use bytes::{Buf, Bytes, BytesMut};
use log::info;
use reqwest::{Client, Response};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    rc::Rc,
};
use tokio::sync::mpsc::{self, Receiver};
use tokio::{io::AsyncWriteExt, sync::mpsc::Sender};

fn get_head_sq_and_time(resp: &Response) -> anyhow::Result<(u64, u64)> {
    let sq: u64 = resp.headers().get("X-Head-Seqnum").ok_or_else(|| dmlerr!())?.to_str()?.parse()?;
    let ti: u64 = resp.headers().get("X-Head-Time-Sec").ok_or_else(|| dmlerr!())?.to_str()?.parse()?;
    Ok((sq, ti))
}

#[allow(unused)]
pub struct Youtube {
    room_url: String,
    url_v: RefCell<String>,
    url_a: RefCell<String>,
    sq: Cell<u64>,
    itvl: Cell<u64>,
    stream_ready: Cell<bool>,
    ctx: Rc<DMLContext>,
}

impl Youtube {
    pub fn new(stream_info: &HashMap<&str, String>, ctx: Rc<DMLContext>) -> Self {
        Youtube {
            url_v: RefCell::new(stream_info["url_v"].to_string()),
            url_a: RefCell::new(stream_info["url_a"].to_string()),
            sq: Cell::new(stream_info["sq"].parse().unwrap_or(1)),
            itvl: Cell::new(1000),
            stream_ready: Cell::new(false),
            room_url: stream_info["room_url"].to_string(),
            ctx,
        }
    }

    pub async fn strip_mp4_header(&self, resp: &mut Response) -> anyhow::Result<Bytes> {
        let mut buf = BytesMut::with_capacity(4000);
        while let Some(chunk) = resp.chunk().await? {
            buf.extend_from_slice(chunk.chunk());
            if buf.len() > 2000 {
                break;
            }
        }
        if buf.len() < 2000 {
            return Err(dmlerr!());
        }
        // if matroska, return
        if &buf[0..4] == b"\x1a\x45\xdf\xa3" {
            return Ok(buf.freeze());
        }
        while buf.len() > 8 {
            let len = buf.get_u32();
            if &buf[0..4] == b"emsg" {
                buf.advance(len as usize - 4);
                return Ok(buf.freeze());
            }
            buf.advance(len as usize - 4);
        }
        Err(dmlerr!())
    }

    pub async fn download_audio(
        &self, client: &Client, stream: &mut Box<dyn DMLStream>, seg: &MediaSegment,
    ) -> anyhow::Result<()> {
        if seg.skip != 0 {
            return Ok(());
        }
        let u = format!("{}sq/{}", self.url_a.borrow(), &seg.url);
        info!("a: {}", &u);
        let mut resp = client
            .get(u)
            .header("Connection", "keep-alive")
            .header("Referer", "https://www.youtube.com/")
            .send()
            .await?;
        if !seg.is_header && seg.skip == 0 {
            let d = self.strip_mp4_header(&mut resp).await?;
            stream.write_all(&d).await?;
        }
        while let Some(chunk) = resp.chunk().await? {
            stream.write_all(&chunk).await?;
        }
        Ok(())
    }

    pub async fn download_video(
        &self, client: &Client, stream: &mut Box<dyn DMLStream>, seg: &MediaSegment,
    ) -> anyhow::Result<()> {
        if seg.skip == 2 {
            return Ok(());
        }
        let u = format!("{}sq/{}", self.url_v.borrow(), &seg.url);
        info!("v: {}", &u);
        let mut resp = client
            .get(u)
            .header("Connection", "keep-alive")
            .header("Referer", "https://www.youtube.com/")
            .send()
            .await?;
        if seg.skip == 1 {
            let (sq, ti) = get_head_sq_and_time(&resp)?;
            self.sq.set(sq);
            let itvl = (ti as f64 / (sq + 1) as f64).round() as u64;
            if itvl != 0 {
                self.itvl.set(itvl * 1000);
            }
        }
        if !seg.is_header && seg.skip == 0 {
            let d = self.strip_mp4_header(&mut resp).await?;
            stream.write_all(&d).await?;
        }
        while let Some(chunk) = resp.chunk().await? {
            if seg.skip == 0 {
                if !self.stream_ready.get() {
                    self.stream_ready.set(true);
                    let _ = self.ctx.mtx.send(DMLMessage::StreamReady).await;
                }
                stream.write_all(&chunk).await?;
            }
        }
        Ok(())
    }

    pub async fn refresh_manifest_task(&self, client: &Client) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(20000));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            let info = streamfinder::youtube::get_live_info(client, self.room_url.as_str()).await?;
            let mut info = streamfinder::youtube::Youtube::decode_mpd(client, &info.5).await?;
            *self.url_v.borrow_mut() = info.remove("url_v").unwrap();
            *self.url_a.borrow_mut() = info.remove("url_a").unwrap();
        }
    }

    pub async fn refresh_seq_task(&self, ss: &SegmentStream) -> anyhow::Result<()> {
        let mut rx = ss.refresh_rx.borrow_mut();
        let mut sq = self.sq.get().saturating_sub(1);
        let mut state = 0;
        while let Some(_) = rx.recv().await {
            let mut clips = VecDeque::new();
            let skip = if state == 0 {
                state = 1;
                1
            } else if state == 1 {
                sq = self.sq.get();
                state = 2;
                0
            } else if state == 2 {
                state = 3;
                0
            } else {
                sq += 1;
                0
            };
            let c = MediaSegment {
                skip,
                props: HashMap::new(),
                url: sq.to_string(),
                is_header: if state == 2 { true } else { false },
            };
            clips.push_back(c);
            ss.update_sequence(sq, clips, self.itvl.get()).await?;
        }
        Ok(())
    }

    pub async fn video_task(&self, client: &Client, mut rx: Receiver<MediaSegment>) -> anyhow::Result<()> {
        let mut video_stream = self.ctx.im.get_video_socket().await?;
        while let Some(clip) = rx.recv().await {
            self.download_video(&client, &mut video_stream, &clip).await?;
        }
        Ok(())
    }

    pub async fn audio_task(&self, client: &Client, mut rx: Receiver<MediaSegment>) -> anyhow::Result<()> {
        let mut audio_stream = self.ctx.im.get_audio_socket().await?;
        while let Some(clip) = rx.recv().await {
            self.download_audio(&client, &mut audio_stream, &clip).await?;
        }
        Ok(())
    }

    pub async fn dispatch_task(
        &self, ss: &SegmentStream, tx_v: Sender<MediaSegment>, tx_a: Sender<MediaSegment>,
    ) -> anyhow::Result<()> {
        let mut rx = ss.clip_rx.borrow_mut();
        while let Some(clip) = rx.recv().await {
            info!("youtube: clip {}", &clip.url);
            tx_v.send(clip.clone()).await?;
            tx_a.send(clip).await?;
        }
        Ok(())
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .timeout(tokio::time::Duration::from_secs(15))
            .build()?;
        let seg_stream = SegmentStream::new();
        let (tx_v, rx_v) = mpsc::channel(100);
        let (tx_a, rx_a) = mpsc::channel(100);
        tokio::select! {
            it = self.refresh_manifest_task(&client) => { it?; },
            it = self.refresh_seq_task(&seg_stream) => { it?; },
            it = self.dispatch_task(&seg_stream, tx_v, tx_a) => { it?; },
            it = self.video_task(&client, rx_v) => { it?; },
            it = self.audio_task(&client, rx_a) => { it?; },
            it = seg_stream.run() => { it?; },
        }
        info!("youtube streamer exit");
        Ok(())
    }
}
