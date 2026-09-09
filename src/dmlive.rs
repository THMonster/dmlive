use crate::{
    config::ConfigManager, danmaku::Danmaku, ffmpeg::FfmpegControl, ipcmanager::IPCManager, mpv::MpvControl,
    streamer::Streamer, streamfinder::StreamFinder,
};
use async_channel::{Receiver, Sender};
use futures::StreamExt;
use log::info;
use std::rc::Rc;
use tokio::time::Duration;

pub struct DMLContext {
    pub im: Rc<IPCManager>,
    pub cm: Rc<ConfigManager>,
    pub mrx: Receiver<DMLMessage>,
    pub mtx: Sender<DMLMessage>,
}

#[allow(unused)]
pub enum DMLMessage {
    SetFontScale(f64),
    SetFontAlpha(f64),
    SetDMSpeed(u64),
    PlayVideo,
    SetVideoInfo((u64, u64, u64)),
    ToggleShowNick,
    FfmpegOutputReady,
    RequestRestart,
    RequestExit,
    StreamReady,
    SubtitleTracksReady {
        generation: u64,
        cid: String,
        tracks: Vec<crate::danmaku::SubtitleTrackFile>,
    },
}

#[allow(unused)]
pub struct DMLive {
    mc: Rc<MpvControl>,
    fc: Rc<FfmpegControl>,
    sf: Rc<StreamFinder>,
    st: Rc<Streamer>,
    dm: Rc<Danmaku>,
    ctx: Rc<DMLContext>,
}

impl DMLive {
    pub async fn new(ctx: Rc<DMLContext>) -> Self {
        let mc = Rc::new(MpvControl::new(ctx.clone()));
        let fc = Rc::new(FfmpegControl::new(ctx.clone()));
        let sf = Rc::new(StreamFinder::new(ctx.clone()));
        let st = Rc::new(Streamer::new(ctx.clone()));
        let dm = Rc::new(Danmaku::new(ctx.clone()));
        DMLive {
            mc,
            fc,
            sf,
            st,
            dm,
            ctx,
        }
    }

    pub async fn run(&self) {
        let signal_task = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        tokio::select! {
            _ = self.dispatch_task() => {},
            _ = self.mc.run() => {},
            _ = self.play() => {},
            _ = signal_task => {},
        }
        match self.ctx.im.stop().await {
            Ok(_) => {}
            Err(err) => info!("ipc manager stop error: {err}"),
        };
    }

    async fn dispatch_task(&self) {
        let mut tasks = futures::stream::FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(_) = tasks.next() => {},
                msg = self.ctx.mrx.recv() => {
                    match msg {
                        Ok(it) => { tasks.push(self.dispatch(it)) },
                        Err(_) => { return; },
                    }
                }
            }
        }
    }

    async fn dispatch(&self, msg: DMLMessage) {
        match msg {
            DMLMessage::SetFontScale(fs) => {
                self.dm.set_font_size(fs).await;
            }
            DMLMessage::SetFontAlpha(fa) => {
                self.dm.set_font_alpha(fa).await;
            }
            DMLMessage::SetDMSpeed(sp) => {
                self.dm.set_speed(sp).await;
            }
            DMLMessage::ToggleShowNick => {
                self.dm.toggle_show_nick().await;
            }
            DMLMessage::RequestRestart => {
                let _ = self.fc.quit().await;
            }
            DMLMessage::RequestExit => {
                // self.quit().await;
            }
            DMLMessage::SetVideoInfo((w, h, pts)) => {
                info!("video info: w {w} h {h} pts {pts}");
                // danmaku task
                if matches!(self.ctx.cm.site, crate::config::Site::BiliVideo) {
                    if let Some((generation, bvid, cid)) = self.dm.bili_video_info() {
                        let ratio = if w == 0 { 1.0 } else { 16.0 * h as f64 / w as f64 / 9.0 };
                        match self.dm.build_bilivideo_tracks(generation, &bvid, &cid, ratio).await {
                            Ok(tracks) => {
                                let _ = self
                                    .ctx
                                    .mtx
                                    .send(DMLMessage::SubtitleTracksReady {
                                        generation,
                                        cid,
                                        tracks,
                                    })
                                    .await;
                            }
                            Err(error) => log::warn!("failed to build video subtitle tracks: {error}"),
                        }
                    }
                } else {
                    self.dm.set_ratio_scale((16.0 / 9.0) / (w as f64 / h as f64));
                    // let _ = self.dm.run(16.0 * h as f64 / w as f64 / 9.0, pts).await;
                }
            }
            DMLMessage::StreamReady => {
                info!("stream ready");
                let _ = self.dm.run().await;
            }
            DMLMessage::PlayVideo => {
                let _ = self.play_video().await.map_err(|e| info!("play video error: {}", e));
            }
            DMLMessage::FfmpegOutputReady => {
                info!("ffmpeg output ready");
                tokio::time::sleep(Duration::from_millis(200)).await;
                match self.ctx.cm.run_mode {
                    crate::config::RunMode::Play => {
                        let _ = self.mc.reload_video().await;
                    }
                    crate::config::RunMode::Record => {
                        if self.ctx.cm.http_address.is_none() {
                            let _ = self.fc.write_record_task().await;
                        }
                    }
                }
            }
            DMLMessage::SubtitleTracksReady {
                generation,
                cid,
                tracks,
            } => {
                if self.dm.is_current_bili_video(generation, &cid) {
                    if let Err(error) = self.mc.load_subtitle_tracks(generation, &tracks).await {
                        log::warn!("failed to load video subtitle tracks: {error}");
                    }
                } else {
                    info!("discarding stale subtitle tracks for cid {cid}");
                }
            }
        }
    }

    pub async fn play(&self) -> anyhow::Result<()> {
        loop {
            match self.ctx.cm.run_mode {
                crate::config::RunMode::Play => {
                    if matches!(self.ctx.cm.site, crate::config::Site::BiliVideo) {
                        self.play_video().await?;
                        tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
                    } else {
                        self.play_live().await?;
                    }
                }
                crate::config::RunMode::Record => match self.ctx.cm.record_mode {
                    crate::config::RecordMode::All => {
                        self.play_live().await?;
                        if matches!(self.ctx.cm.site, crate::config::Site::BiliVideo) {
                            return Err(anyhow::anyhow!("recording finished"));
                        }
                    }
                    crate::config::RecordMode::Danmaku => {
                        self.download_danmaku().await?;
                        return Err(anyhow::anyhow!("recording finished"));
                    }
                },
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
        // Ok(())
    }

    pub async fn play_live(&self) -> anyhow::Result<()> {
        let mut stream_info = self.sf.run().await?;
        self.ctx.cm.set_stream_type(&stream_info);
        *self.ctx.cm.title.borrow_mut() = stream_info.remove("title").unwrap();
        self.dm.set_bili_video_cid(stream_info.get("bili_cid").unwrap_or(&"".to_string())).await;
        let ff_task = async {
            self.fc.run(&stream_info).await?;
            anyhow::Ok(())
        };
        let streamer_task = async {
            let _ = self.st.run(&stream_info).await.map_err(|e| info!("streamer error: {}", e));
            self.fc.quit().await?;
            anyhow::Ok(())
        };
        if matches!(self.ctx.cm.site, crate::config::Site::BiliVideo) {
            ff_task.await?;
        } else {
            let (_ff_res, _st_res) = tokio::join!(ff_task, streamer_task);
        }
        Ok(())
    }

    pub async fn play_video(&self) -> anyhow::Result<()> {
        let mut stream_info = self.sf.run().await?;
        self.ctx.cm.set_stream_type(&stream_info);
        *self.ctx.cm.title.borrow_mut() = stream_info.remove("title").unwrap();
        self.dm.set_bili_video_info(
            stream_info.get("bili_bvid").map(String::as_str).unwrap_or(""),
            stream_info.get("bili_cid").map(String::as_str).unwrap_or(""),
        );
        self.mc.reload_edl_video(&stream_info).await?;
        Ok(())
    }

    pub async fn download_danmaku(&self) -> anyhow::Result<()> {
        let mut stream_info = self.sf.run().await?;
        *self.ctx.cm.title.borrow_mut() = stream_info.remove("title").unwrap();
        self.dm.set_bili_video_cid(stream_info.get("bili_cid").unwrap_or(&"".to_string())).await;
        let ff_task = async {
            self.fc.write_danmaku_only_task().await?;
            anyhow::Ok(())
        };
        let danmaku_task = async {
            match self.ctx.cm.site {
                crate::config::Site::BiliVideo => {
                    let _ = self.dm.run_bilivideo(1.0).await;
                }
                crate::config::Site::BahaVideo => {
                    let _ = self.dm.run_baha().await;
                }
                _ => todo!(),
            }
            anyhow::Ok(())
        };
        let (_ff_res, _st_res) = tokio::join!(ff_task, danmaku_task);
        Ok(())
    }
}
