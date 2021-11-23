mod bilibili;
mod douyu;
mod huya;
mod mkv_header;
mod twitch;
mod youtube;

use crate::{config::ConfigManager, dmlive::DMLMessage, ipcmanager::DMLStream};
use anyhow::*;
use async_channel::Sender;
use log::info;
use std::sync::Arc;
use tokio::{io::AsyncWriteExt, sync::RwLock, task::spawn_local};

#[derive(Clone, Debug)]
struct DanmakuChannel {
    length: usize,
    begin_pts: usize,
}
pub struct Danmaku {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    show_nick: Arc<RwLock<bool>>,
    font_size: Arc<RwLock<usize>>,
    channel_num: Arc<RwLock<usize>>,
    font_alpha: Arc<RwLock<f64>>,
    ratio_scale: Arc<RwLock<f64>>,
    speed: Arc<RwLock<usize>>,
}

impl Danmaku {
    pub fn new(cm: Arc<ConfigManager>, im: Arc<crate::ipcmanager::IPCManager>, _mtx: Sender<DMLMessage>) -> Self {
        Self {
            ipc_manager: im,
            cm,
            show_nick: Arc::new(RwLock::new(false)),
            font_size: Arc::new(RwLock::new(40)),
            font_alpha: Arc::new(RwLock::new(0.0)),
            speed: Arc::new(RwLock::new(8000)),
            ratio_scale: Arc::new(RwLock::new(1.0)),
            channel_num: Arc::new(RwLock::new(14)),
        }
    }

    async fn get_avail_danmaku_channel(
        &self,
        now: &std::time::Instant,
        len: usize,
        channels: &mut [DanmakuChannel],
    ) -> Option<usize> {
        let s = (1920.0 + len as f64) / *self.speed.read().await as f64;
        let c_pts = now.elapsed().as_millis() as usize;
        for (i, c) in channels.iter_mut().enumerate() {
            if i >= *self.channel_num.read().await {
                break;
            }
            if c.length == 0 {
                c.length = len;
                c.begin_pts = c_pts;
                return Some(i);
            }
            if ((*self.speed.read().await as f64 - c_pts as f64 + c.begin_pts as f64) * s) > 1920.0 {
                // println!("1");
                continue;
            } else {
                if ((c.length + 1920) as f64 * (c_pts as f64 - c.begin_pts as f64) / *self.speed.read().await as f64)
                    < c.length as f64
                {
                    // println!("2");
                    continue;
                } else {
                    c.length = len;
                    c.begin_pts = c_pts;
                    return Some(i);
                }
            }
        }
        None
    }

    async fn get_danmaku_display_length(&self, dm: &str) -> usize {
        let mut ascii_num = 0;
        let mut non_ascii_num = 0;
        for c in dm.chars() {
            if c.is_ascii() {
                ascii_num += 1;
            } else {
                non_ascii_num += 1;
            }
        }
        let fs = *self.font_size.read().await;
        (((fs as f64 * 0.75 * non_ascii_num as f64) + (fs as f64 * 0.25 * ascii_num as f64))
            * *self.ratio_scale.read().await)
            .round() as usize
    }

    async fn launch_danmaku(
        &self,
        c: &str,
        n: &str,
        d: &str,
        now: &std::time::Instant,
        channels: &mut [DanmakuChannel],
        read_order: &mut usize,
        socket: &mut Box<dyn DMLStream>,
    ) -> Result<()> {
        let cluster = if n.trim().is_empty() {
            let ass = format!(r#"{},0,Default,dmlive-empty,20,20,2,,"#, *read_order,).into_bytes();
            mkv_header::DMKVCluster::new(ass, now.elapsed().as_millis() as usize, 1)
        } else {
            let display_length = self.get_danmaku_display_length(&d).await;
            let avail_dc =
                self.get_avail_danmaku_channel(&now, display_length, channels).await.ok_or(anyhow!("ld err 1"))?;
            let ass = format!(
                r#"{4},0,Default,{5},0,0,0,,{{\alpha{0}\fs{7}\1c&{6}&\move(1920,{1},{2},{1})}}{3}"#,
                format!("{:02x}", (*self.font_alpha.read().await * 255 as f64) as u8),
                avail_dc * *self.font_size.read().await,
                0 - display_length as isize,
                &d,
                *read_order,
                &n,
                format!("{}{}{}", &c[4..6], &c[2..4], &c[0..2]),
                *self.font_size.read().await,
            )
            .into_bytes();
            mkv_header::DMKVCluster::new(
                ass,
                now.elapsed().as_millis() as usize,
                *self.speed.read().await,
            )
        };
        *read_order = read_order.saturating_add(1);
        match cluster.write_to_socket(socket).await {
            Ok(_) => {}
            Err(_) => return Err(anyhow!("socket error")),
        };
        Ok(())
    }

    async fn init(&self) {
        *self.font_size.write().await =
            (40 as f64 * self.cm.toml_config.read().await.font_scale.unwrap_or(1.0)) as usize;
        *self.channel_num.write().await = (540.0 / *self.font_size.read().await as f64).ceil() as usize;
    }

    pub async fn run_danmaku_client(
        self: &Arc<Self>,
        dtx: async_channel::Sender<(String, String, String)>,
    ) -> Result<()> {
        loop {
            match if self.cm.room_url.contains("live.bilibili.com") {
                let b = bilibili::Bilibili::new();
                b.run(&self.cm.room_url, dtx.clone()).await
            } else if self.cm.room_url.contains("douyu.com/") {
                let b = douyu::Douyu::new();
                b.run(&self.cm.room_url, dtx.clone()).await
            } else if self.cm.room_url.contains("huya.com/") {
                let b = huya::Huya::new();
                b.run(&self.cm.room_url, dtx.clone()).await
            } else if self.cm.room_url.contains("youtube.com/") {
                let b = youtube::Youtube::new();
                b.run(&self.cm.room_url, dtx.clone()).await
            } else if self.cm.room_url.contains("twitch.tv/") {
                let b = twitch::Twitch::new();
                b.run(&self.cm.room_url, dtx.clone()).await
            } else {
                return Err(anyhow!("unsupported url"));
            } {
                Ok(_) => {}
                Err(e) => {
                    println!("danmaku client error: {:?}", e);
                }
            };
            // if dtx.is_closed() {
            //     break;
            // }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        // Ok(())
    }

    pub async fn run(self: &Arc<Self>) -> Result<()> {
        self.init().await;
        let mut socket = self.ipc_manager.get_danmaku_socket().await?;
        let now = std::time::Instant::now();
        let mut read_order = 0usize;
        let emoji_re= regex::Regex::new(
            r#"[\x{1F300}-\x{1F5FF}|\x{1F1E6}-\x{1F1FF}|\x{2700}-\x{27BF}|\x{1F900}-\x{1F9FF}|\x{1F600}-\x{1F64F}|\x{1F680}-\x{1F6FF}|\x{2600}-\x{26FF}]"#)
            .unwrap();
        let mut dchannels = vec![
            DanmakuChannel {
                length: 0,
                begin_pts: 0
            };
            30
        ];
        let (dtx, drx) = async_channel::unbounded();
        let s1 = self.clone();
        let dc = spawn_local(async move {
            let _ = s1.run_danmaku_client(dtx).await;
        });

        socket.write_all(&mkv_header::get_mkv_header()).await?;
        // wu~wu~ warm up
        for _ in 1..77 {
            let _ = self
                .launch_danmaku(
                    "",
                    "",
                    "",
                    &now,
                    &mut dchannels,
                    &mut read_order,
                    &mut socket,
                )
                .await;
        }
        'l1: loop {
            if let Ok((c, n, d)) = drx.try_recv() {
                println!("[{}] {}", &n, &d);
                loop {
                    match self
                        .launch_danmaku(
                            &c,
                            &n,
                            &d,
                            &now,
                            &mut dchannels,
                            &mut read_order,
                            &mut socket,
                        )
                        .await
                    {
                        Ok(_) => {
                            break;
                        }
                        Err(e) => {
                            info!("danmaku send error: {}", &e);
                            let _ = self
                                .launch_danmaku(
                                    "",
                                    "",
                                    "",
                                    &now,
                                    &mut dchannels,
                                    &mut read_order,
                                    &mut socket,
                                )
                                .await;
                            if e.to_string().contains("socket error") {
                                break 'l1;
                            }
                            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                        }
                    };
                }
            } else {
                let _ = self
                    .launch_danmaku(
                        "",
                        "",
                        "",
                        &now,
                        &mut dchannels,
                        &mut read_order,
                        &mut socket,
                    )
                    .await;
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        }
        dc.abort();
        Ok(())
    }
}
