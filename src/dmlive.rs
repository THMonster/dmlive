use std::{ops::Deref, sync::Arc};

use async_channel::Receiver;
use log::info;
use tokio::sync::RwLock;

use crate::{
    config::ConfigManager, danmaku::Danmaku, ffmpeg::FfmpegControl, ipcmanager::IPCManager, mpv::MpvControl,
    streamer::Streamer, streamfinder::StreamFinder,
};

pub enum DMLMessage {
    SetFontSize(usize),
    SetFontAlpha(f64),
    SetShowNick(bool),
    StreamStarted,
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
        let mut im = IPCManager::new(cm.clone(), 0);
        im.run().await.unwrap();
        let im = Arc::new(im);
        let (mtx, mrx) = async_channel::unbounded();
        let mc = Arc::new(MpvControl::new(cm.clone(), im.clone(), mtx.clone()));
        let fc = Arc::new(FfmpegControl::new(cm.clone(), im.clone(), mtx.clone()));
        let sf = Arc::new(StreamFinder::new(cm.clone(), im.clone(), mtx.clone()));
        let st = Arc::new(Streamer::new(cm.clone(), im.clone(), mtx.clone()));
        let dm = Arc::new(Danmaku::new(cm.clone(), im.clone(), mtx.clone()));
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
            let _ = s2.restart().await;
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
                DMLMessage::SetFontSize(_) => todo!(),
                DMLMessage::SetFontAlpha(_) => todo!(),
                DMLMessage::SetShowNick(_) => todo!(),
                DMLMessage::StreamStarted => {
                    info!("stream started");
                    let s1 = self.clone();
                    tokio::task::spawn_local(async move {
                        let _ = s1.dm.run().await;
                    });
                }
                DMLMessage::RequestRestart => {
                    self.restart().await;
                }
                DMLMessage::RequestExit => {
                    self.stop().await;
                }
            }
        }
    }

    pub async fn restart(self: &Arc<Self>) {
        if matches!(self.state.read().await.deref(), DMLState::Exiting) {
            return;
        }
        // let _ = self.mc.reload_video().await;
        let (title, urls) = match self.sf.run().await {
            Ok(it) => it,
            Err(_) => {
                let _ = self.mc.quit().await;
                return;
            }
        };
        self.cm.set_stream_type(&urls[0]).await;
        let s2 = self.clone();
        tokio::task::spawn_local(async move {
            let _ = s2.fc.run(&title).await;
            info!("ffmpeg exit");
            let _ = s2.restart().await;
        });
        let s3 = self.clone();
        tokio::task::spawn_local(async move {
            let _ = s3.st.run(urls).await;
        });
    }

    pub async fn stop(self: &Arc<Self>) {
        *self.state.write().await = DMLState::Exiting;
        let _ = self.mc.quit().await;
    }
}
