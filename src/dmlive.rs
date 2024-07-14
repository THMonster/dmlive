use crate::{
    config::ConfigManager, danmaku::Danmaku, ffmpeg::FfmpegControl, ipcmanager::IPCManager, mpv::MpvControl,
    streamer::Streamer, streamfinder::StreamFinder,
};
use async_channel::{Receiver, Sender};
use futures::StreamExt;
use log::info;
use std::rc::Rc;
use tokio::time::Duration;

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
}

#[allow(unused)]
pub struct DMLive {
    ipc_manager: Rc<IPCManager>,
    cm: Rc<ConfigManager>,
    mc: Rc<MpvControl>,
    fc: Rc<FfmpegControl>,
    sf: Rc<StreamFinder>,
    st: Rc<Streamer>,
    dm: Rc<Danmaku>,
    mrx: Receiver<DMLMessage>,
    mtx: Sender<DMLMessage>,
}

impl DMLive {
    pub async fn new(cm: Rc<ConfigManager>, im: Rc<IPCManager>) -> Self {
        let (mtx, mrx) = async_channel::unbounded();
        let mc = Rc::new(MpvControl::new(cm.clone(), im.clone(), mtx.clone()));
        let fc = Rc::new(FfmpegControl::new(cm.clone(), im.clone(), mtx.clone()));
        let sf = Rc::new(StreamFinder::new(cm.clone(), im.clone(), mtx.clone()));
        let st = Rc::new(Streamer::new(cm.clone(), im.clone(), mtx.clone()));
        let dm = Rc::new(Danmaku::new(cm.clone(), im.clone(), mtx.clone()));
        DMLive {
            ipc_manager: im,
            cm,
            mrx,
            mtx,
            mc,
            fc,
            sf,
            st,
            dm,
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
        match self.ipc_manager.stop().await {
            Ok(_) => {}
            Err(err) => info!("ipc manager stop error: {}", err),
        };
    }

    async fn dispatch_task(&self) {
        let mut tasks = futures::stream::FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(_) = tasks.next() => {},
                msg = self.mrx.recv() => {
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
                info!("video info: w {} h {} pts {}", w, h, pts);
                // danmaku task
                if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                    let _ = self.dm.run_bilivideo(16.0 * h as f64 / w as f64 / 9.0).await;
                } else {
                    self.dm.set_ratio_scale(16.0 * h as f64 / w as f64 / 9.0);
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
                match self.cm.run_mode {
                    crate::config::RunMode::Play => {
                        let _ = self.mc.reload_video().await;
                    }
                    crate::config::RunMode::Record => {
                        if self.cm.http_address.is_none() {
                            let _ = self.fc.write_record_task().await;
                        }
                    }
                }
            }
        }
    }

    pub async fn play(&self) -> anyhow::Result<()> {
        loop {
            match self.cm.run_mode {
                crate::config::RunMode::Play => {
                    if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                        self.play_video().await?;
                        tokio::time::sleep(Duration::from_secs(u64::MAX)).await;
                    } else {
                        self.play_live().await?;
                    }
                }
                crate::config::RunMode::Record => {
                    self.play_live().await?;
                    if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                        return Err(anyhow::anyhow!("recording finished"));
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
        // Ok(())
    }

    pub async fn play_live(&self) -> anyhow::Result<()> {
        let (title, urls) = self.sf.run().await?;
        self.cm.set_stream_type(&urls[0]);
        self.cm.title.borrow_mut().clear();
        self.cm.title.borrow_mut().push_str(&title);
        self.dm.set_bili_video_cid(&urls[0]).await;
        let ff_task = async {
            self.fc.run(&urls).await?;
            anyhow::Ok(())
        };
        let streamer_task = async {
            let _ = self.st.run(&urls).await.map_err(|e| info!("streamer error: {}", e));
            self.fc.quit().await?;
            anyhow::Ok(())
        };
        if matches!(self.cm.site, crate::config::Site::BiliVideo) {
            ff_task.await?;
        } else {
            let (_ff_res, _st_res) = tokio::join!(ff_task, streamer_task);
        }
        Ok(())
    }

    pub async fn play_video(&self) -> anyhow::Result<()> {
        let (title, urls) = self.sf.run().await?;
        self.cm.set_stream_type(&urls[0]);
        self.cm.title.borrow_mut().clear();
        self.cm.title.borrow_mut().push_str(&title);
        self.dm.set_bili_video_cid(&urls[0]).await;
        self.mc.reload_edl_video(&urls).await?;
        Ok(())
    }
}
