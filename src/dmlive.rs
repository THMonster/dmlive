use crate::{
    config::ConfigManager, danmaku::Danmaku, ffmpeg::FfmpegControl, ipcmanager::IPCManager, mpv::MpvControl,
    streamer::Streamer, streamfinder::StreamFinder,
};
use async_channel::Receiver;
use log::{info, warn};
use std::{ops::Deref, sync::Arc};
use tokio::sync::RwLock;

pub enum DMLMessage {
    SetFontScale(f64),
    SetFontAlpha(f64),
    SetDMSpeed(u64),
    GoToBVPage(usize),
    SetVideoInfo((u64, u64, u64)),
    ToggleShowNick,
    RequestRestart,
    RequestExit,
}

enum DMLState {
    Running,
    Exiting,
}

pub struct DMLive {
    ipc_manager: Arc<IPCManager>,
    cm: Arc<ConfigManager>,
    mc: Arc<MpvControl>,
    fc: Arc<FfmpegControl>,
    sf: Arc<StreamFinder>,
    st: Arc<Streamer>,
    dm: Arc<Danmaku>,
    mrx: Receiver<DMLMessage>,
    state: RwLock<DMLState>,
}

impl DMLive {
    pub async fn new(cm: Arc<ConfigManager>) -> Self {
        let mut im = IPCManager::new(cm.clone());
        im.run().await.unwrap();
        let im = Arc::new(im);
        let (mtx, mrx) = async_channel::unbounded();
        let mc = Arc::new(MpvControl::new(cm.clone(), im.clone(), mtx.clone()));
        let fc = Arc::new(FfmpegControl::new(cm.clone(), im.clone(), mtx.clone()));
        let sf = Arc::new(StreamFinder::new(cm.clone(), im.clone(), mtx.clone()));
        let st = Arc::new(Streamer::new(cm.clone(), im.clone(), mtx.clone()));
        let dm = Arc::new(Danmaku::new(cm.clone(), im.clone(), mtx));
        DMLive {
            ipc_manager: im,
            cm,
            mrx,
            mc,
            fc,
            sf,
            st,
            dm,
            state: RwLock::new(DMLState::Running),
        }
    }

    pub async fn run(self: &Arc<Self>) {
        let s1 = self.clone();
        tokio::task::spawn_local(async move {
            s1.dispatch().await;
        });
        let s2 = self.clone();
        tokio::task::spawn_local(async move {
            s2.restart().await;
        });
        let s3 = self.clone();
        tokio::task::spawn_local(async move {
            let _ = tokio::signal::ctrl_c().await;
            s3.quit().await;
        });
        let _ = self.mc.run().await;
        match self.ipc_manager.stop().await {
            Ok(_) => {}
            Err(err) => info!("ipc manager stop error: {}", err),
        };
    }

    pub async fn dispatch(self: &Arc<Self>) {
        loop {
            match self.mrx.recv().await.unwrap() {
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
                    self.restart().await;
                }
                DMLMessage::RequestExit => {
                    self.quit().await;
                }
                DMLMessage::SetVideoInfo((w, h, pts)) => {
                    info!("video info: w {} h {} pts {}", w, h, pts);
                    let s1 = self.clone();
                    // danmaku task
                    tokio::task::spawn_local(async move {
                        if matches!(s1.cm.site, crate::config::Site::BiliVideo) {
                            let _ = s1.dm.run_bilivideo(16.0 * h as f64 / w as f64 / 9.0).await;
                        } else {
                            let _ = s1.dm.run(16.0 * h as f64 / w as f64 / 9.0, pts).await;
                        }
                    });
                }
                DMLMessage::GoToBVPage(p) => {
                    match self.sf.run_bilivideo(p).await {
                        Ok((title, urls)) => {
                            let u1 = urls[0].to_string();
                            self.dm.set_bili_video_cid(&u1).await;
                            let s2 = self.clone();
                            tokio::task::spawn_local(async move {
                                let _ = s2.mc.reload_edl_video(&urls, &title).await;
                            });
                        }
                        Err(_) => {}
                    };
                }
            }
        }
    }

    pub async fn restart(self: &Arc<Self>) {
        if matches!(self.state.read().await.deref(), DMLState::Exiting) {
            return;
        }
        let (title, urls) = match self.sf.run().await {
            Ok(it) => it,
            Err(_) => {
                self.quit().await;
                return;
            }
        };
        self.cm.set_stream_type(&urls[0]).await;
        let u1 = urls[0].to_string();
        self.dm.set_bili_video_cid(&u1).await;
        let s2 = self.clone();
        let urls1 = urls.clone();
        // ffmpeg task
        tokio::task::spawn_local(async move {
            if matches!(s2.cm.site, crate::config::Site::BiliVideo)
                && matches!(s2.cm.run_mode, crate::config::RunMode::Play)
            {
                let _ = s2.mc.reload_edl_video(&urls1, &title).await;
            } else {
                let _ = s2.fc.run(&title, &urls1).await;
                info!("ffmpeg exit");
                if matches!(s2.cm.site, crate::config::Site::BiliVideo) {
                    // bilibili video download completed, then quit
                    let _ = s2.mc.quit().await;
                } else {
                    s2.restart().await;
                }
            }
        });
        let s3 = self.clone();
        // streamer task
        tokio::task::spawn_local(async move {
            if !matches!(s3.cm.site, crate::config::Site::BiliVideo) {
                let _ = s3.st.run(urls).await;
                let _ = s3.fc.quit_new().await;
            }
        });
    }

    pub async fn quit(self: &Arc<Self>) {
        *self.state.write().await = DMLState::Exiting;
        let _ = self.mc.quit().await;
    }
}
