use async_channel::{Receiver, Sender};
use futures::StreamExt;
use log::info;
use std::{cell::Cell, rc::Rc};
use tokio::{sync::Notify, time::Duration};

use crate::{
    config::{ConfigManager, RecordMode, RunMode, SiteType},
    danmaku::Danmaku,
    ffmpeg::FfmpegControl,
    ipcmanager::IPCManager,
    mpv::MpvControl,
    streamer::Streamer,
    streamfinder::StreamFinder,
};

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
}

#[allow(unused)]
pub struct DMLive {
    mc: Rc<MpvControl>,
    fc: Rc<FfmpegControl>,
    sf: Rc<StreamFinder>,
    st: Rc<Streamer>,
    dm: Rc<Danmaku>,
    ctx: Rc<DMLContext>,
    quit_notify: (Notify, Cell<bool>),
    ready2play_notify: (Notify, Cell<bool>),
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
            quit_notify: (Notify::new(), Cell::new(false)),
            ready2play_notify: (Notify::new(), Cell::new(false)),
        }
    }

    pub async fn run(&self) {
        let signal_task = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        tokio::select! {
            _ = self.dispatch_task() => {},
            _ = self.mc.run() => {},
            _ = self.start() => {},
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
                // let _ = self.fc.quit().await;
                info!("request restart");
                if self.quit_notify.1.get() == true {
                    self.quit_notify.0.notify_one();
                }
            }
            DMLMessage::RequestExit => {}
            DMLMessage::SetVideoInfo((w, h, pts)) => {
                info!("video info: w {w} h {h} pts {pts}");
                self.dm.set_ratio_scale((16.0 / 9.0) / (w as f64 / h as f64));
                if self.dm.ready_notify.1.get() == true {
                    self.dm.ready_notify.0.notify_one();
                }
            }
            DMLMessage::StreamReady => {
                info!("stream ready");
                // let _ = self.dm.run().await;
            }
            DMLMessage::PlayVideo => {
                // let _ = self.play_video().await.map_err(|e| info!("play video error: {}", e));
            }
            DMLMessage::FfmpegOutputReady => {
                info!("ffmpeg output ready");
                tokio::time::sleep(Duration::from_millis(200)).await;
                if self.ready2play_notify.1.get() == true {
                    self.ready2play_notify.0.notify_one();
                }
            }
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let mode = match self.ctx.cm.run_mode {
            RunMode::Play => match self.ctx.cm.site_type {
                SiteType::Live => 0,
                SiteType::Video => 1,
            },
            RunMode::Record => match self.ctx.cm.site_type {
                SiteType::Live => 2,
                SiteType::Video => match self.ctx.cm.record_mode {
                    RecordMode::All => 3,
                    RecordMode::Danmaku => 4,
                },
            },
        };
        loop {
            if mode == 0 {
                self.play_live().await?;
            } else if mode == 1 {
                self.play_video().await?;
            } else if mode == 2 {
                self.record_live().await?;
            } else if mode == 3 {
                self.record_video().await?;
                break;
            } else {
                self.record_danmaku().await?;
                break;
            }
            tokio::time::sleep(Duration::from_millis(2000)).await;
        }
        Ok(())
    }

    pub async fn play_live_old(&self) -> anyhow::Result<()> {
        self.sf.run().await?;
        self.ctx.cm.set_stream_type();
        // *self.ctx.cm.title.borrow_mut() = stream_info.remove("title").unwrap();
        *self.ctx.cm.title.borrow_mut() = format!(
            "{} - {}",
            self.ctx.cm.stream_info.borrow()["title"],
            self.ctx.cm.stream_info.borrow()["owner_name"]
        );
        // self.ctx.cm.bvideo_info.borrow_mut().current_cid = stream_info.remove("bili_cid").unwrap_or("".to_string());
        let ff_task = async {
            // self.fc.run(&stream_info).await?;
            anyhow::Ok(())
        };
        let streamer_task = async {
            let _ = self.st.run().await.map_err(|e| info!("streamer error: {}", e));
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

    // pub async fn play_video(&self) -> anyhow::Result<()> {
    //     let mut stream_info = self.sf.run().await?;
    //     self.ctx.cm.set_stream_type();
    //     *self.ctx.cm.title.borrow_mut() = stream_info.remove("title").unwrap();
    //     self.ctx.cm.bvideo_info.borrow_mut().current_cid = stream_info.remove("bili_cid").unwrap_or("".to_string());
    //     // self.dm.set_bili_video_cid(stream_info.get("bili_cid").unwrap_or(&"".to_string())).await;
    //     self.mc.reload_edl_video(&stream_info).await?;
    //     Ok(())
    // }

    // pub async fn download_danmaku(&self) -> anyhow::Result<()> {
    //     let mut stream_info = self.sf.run().await?;
    //     *self.ctx.cm.title.borrow_mut() = stream_info.remove("title").unwrap();
    //     self.ctx.cm.bvideo_info.borrow_mut().current_cid = stream_info.remove("bili_cid").unwrap_or("".to_string());
    //     // self.dm.set_bili_video_cid(stream_info.get("bili_cid").unwrap_or(&"".to_string())).await;
    //     let ff_task = async {
    //         self.fc.write_danmaku_only_task().await?;
    //         anyhow::Ok(())
    //     };
    //     let danmaku_task = async {
    //         match self.ctx.cm.site {
    //             crate::config::Site::BiliVideo => {
    //                 // let _ = self.dm.run_bilivideo(1.0).await;
    //             }
    //             crate::config::Site::BahaVideo => {
    //                 // let _ = self.dm.run_baha().await;
    //             }
    //             _ => todo!(),
    //         }
    //         anyhow::Ok(())
    //     };
    //     let (_ff_res, _st_res) = tokio::join!(ff_task, danmaku_task);
    //     Ok(())
    // }

    pub async fn play_live(&self) -> anyhow::Result<()> {
        let _ = self.sf.run().await?;
        self.ctx.cm.set_stream_type();
        *self.ctx.cm.title.borrow_mut() = format!(
            "{} - {}",
            self.ctx.cm.stream_info.borrow()["title"],
            self.ctx.cm.stream_info.borrow()["owner_name"]
        );
        let watchdog = async {
            self.quit_notify.1.set(true);
            self.quit_notify.0.notified().await;
            self.quit_notify.1.set(false);
        };
        let mpv_task = async {
            self.ready2play_notify.1.set(true);
            self.ready2play_notify.0.notified().await;
            self.ready2play_notify.1.set(false);
            self.mc.reload_video().await?;
            std::future::pending::<()>().await;
            anyhow::Ok(())
        };
        tokio::select! {
            _ = watchdog => {},
            it = mpv_task => {it?},
            it = self.fc.run() => {it?},
            it = self.dm.run() => {it?},
            it = self.st.run() => {it?},
        }
        Ok(())
    }

    pub async fn play_video(&self) -> anyhow::Result<()> {
        let _ = self.sf.run().await?;
        *self.ctx.cm.title.borrow_mut() = self.ctx.cm.stream_info.borrow()["title"].to_string();

        let watchdog = async {
            self.quit_notify.1.set(true);
            self.quit_notify.0.notified().await;
            self.quit_notify.1.set(false);
        };
        self.mc.reload_edl_video().await?;
        let (_, r2) = tokio::join!(watchdog, self.dm.run());
        r2?;
        Ok(())
    }

    pub async fn record_live(&self) -> anyhow::Result<()> {
        let _ = self.sf.run().await?;
        self.ctx.cm.set_stream_type();
        *self.ctx.cm.title.borrow_mut() = format!(
            "{} - {}",
            self.ctx.cm.stream_info.borrow()["title"],
            self.ctx.cm.stream_info.borrow()["owner_name"]
        );
        let watchdog = async {
            self.quit_notify.1.set(true);
            self.quit_notify.0.notified().await;
            self.quit_notify.1.set(false);
        };
        let record_task = async {
            self.ready2play_notify.1.set(true);
            self.ready2play_notify.0.notified().await;
            self.ready2play_notify.1.set(false);
            self.fc.write_record_task().await?;
            anyhow::Ok(())
        };
        tokio::select! {
            _ = watchdog => {},
            it = record_task => {it?},
            it = self.fc.run() => {it?},
            it = self.dm.run() => {it?},
            it = self.st.run() => {it?},
        }
        Ok(())
    }

    pub async fn record_video(&self) -> anyhow::Result<()> {
        self.sf.run().await?;
        self.ctx.cm.set_stream_type();
        *self.ctx.cm.title.borrow_mut() = self.ctx.cm.stream_info.borrow()["title"].to_string();

        let record_task = async {
            self.ready2play_notify.1.set(true);
            self.ready2play_notify.0.notified().await;
            self.ready2play_notify.1.set(false);
            self.fc.write_record_task().await?;
            anyhow::Ok(())
        };
        let (r1, r2, r3) = tokio::join!(self.dm.run(), record_task, self.fc.run());
        r1?;
        r2?;
        r3?;
        Ok(())
    }

    pub async fn record_danmaku(&self) -> anyhow::Result<()> {
        self.sf.run().await?;
        *self.ctx.cm.title.borrow_mut() = self.ctx.cm.stream_info.borrow()["title"].to_string();

        // because there is no video to determine the ratio
        self.dm.ready_notify.0.notify_one();

        let (r1, r2) = tokio::join!(self.dm.run(), self.fc.write_danmaku_only_task());
        r1?;
        r2?;
        Ok(())
    }
}
