mod bilibili;
mod bilivideo;
mod douyu;
mod fudujikiller;
mod huya;
mod mkv_header;
mod twitch;
mod youtube;

use crate::{
    config::ConfigManager,
    dmlive::DMLMessage,
    ipcmanager::DMLStream,
};
use anyhow::anyhow;
use anyhow::Result;
use async_channel::Sender;
use chrono::{
    Duration,
    NaiveTime,
};
use log::info;
use log::warn;
use std::rc::Rc;
use std::{
    collections::BTreeMap,
    ops::BitXorAssign,
    sync::Arc,
};
use tokio::{
    io::AsyncWriteExt,
    sync::RwLock,
    task::spawn_local,
};

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
    bili_video_cid: RwLock<String>,
    fk: fudujikiller::FudujiKiller,
}

impl Danmaku {
    pub fn new(cm: Arc<ConfigManager>, im: Arc<crate::ipcmanager::IPCManager>, _mtx: Sender<DMLMessage>) -> Self {
        Self {
            ipc_manager: im,
            cm,
            show_nick: Arc::new(RwLock::new(false)),
            font_size: Arc::new(RwLock::new(40)),
            channel_num: Arc::new(RwLock::new(14)),
            fk: fudujikiller::FudujiKiller::new(),
            bili_video_cid: RwLock::new("".into()),
        }
    }

    pub async fn set_speed(&self, speed: u64) {
        if speed > 1000 {
            *self.cm.danmaku_speed.write().await = speed;
            let _ = self.cm.write_config().await;
        }
    }

    pub async fn set_font_size(&self, font_scale: f64) {
        if font_scale > 0.0 {
            *self.font_size.write().await = (40_f64 * font_scale) as usize;
            *self.channel_num.write().await = (540.0 / *self.font_size.read().await as f64).ceil() as usize;
            *self.cm.font_scale.write().await = font_scale;
            let _ = self.cm.write_config().await;
        }
    }

    pub async fn set_font_alpha(&self, font_alpha: f64) {
        if (0.0..=1.0).contains(&font_alpha) {
            *self.cm.font_alpha.write().await = font_alpha;
            let _ = self.cm.write_config().await;
        }
    }

    pub async fn set_bili_video_cid(self: &Arc<Self>, cid: &str) {
        let mut bvc = self.bili_video_cid.write().await;
        bvc.clear();
        bvc.push_str(cid);
    }

    pub async fn toggle_show_nick(&self) {
        self.show_nick.write().await.bitxor_assign(true);
    }

    async fn get_avail_danmaku_channel(
        &self,
        c_pts: usize,
        len: usize,
        channels: &mut [DanmakuChannel],
    ) -> Option<usize> {
        let s = (1920.0 + len as f64) / *self.cm.danmaku_speed.read().await as f64;
        for (i, c) in channels.iter_mut().enumerate() {
            if i >= *self.channel_num.read().await {
                break;
            }
            if c.length == 0 {
                c.length = len;
                c.begin_pts = c_pts;
                return Some(i);
            }
            if ((*self.cm.danmaku_speed.read().await as f64 - c_pts as f64 + c.begin_pts as f64) * s) > 1920.0 {
                continue;
            } else if ((c.length + 1920) as f64 * (c_pts as f64 - c.begin_pts as f64)
                / *self.cm.danmaku_speed.read().await as f64)
                < c.length as f64
            {
                continue;
            } else {
                c.length = len;
                c.begin_pts = c_pts;
                return Some(i);
            }
        }
        None
    }

    async fn get_danmaku_display_length(&self, nick: &str, dm: &str, ratio_scale: f64) -> usize {
        let mut ascii_num = 0;
        let mut non_ascii_num = 0;
        for c in dm.chars() {
            if c.is_ascii() {
                ascii_num += 1;
            } else {
                non_ascii_num += 1;
            }
        }
        if *self.show_nick.read().await {
            for c in nick.chars() {
                if c.is_ascii() {
                    ascii_num += 1;
                } else {
                    non_ascii_num += 1;
                }
            }
            non_ascii_num += 1;
        }
        let fs = *self.font_size.read().await;
        (((fs as f64 * 0.75 * non_ascii_num as f64) + (fs as f64 * 0.50 * ascii_num as f64)) * ratio_scale).round()
            as usize
    }

    async fn launch_danmaku(
        &self,
        c: &str,
        n: &str,
        d: &str,
        c_pts: usize,
        ratio_scale: f64,
        channels: &mut [DanmakuChannel],
        read_order: &mut usize,
        socket: &mut Box<dyn DMLStream>,
    ) -> Result<()> {
        let cluster = if n.trim().is_empty() {
            let ass = format!(r#"{},0,Default,dmlive-empty,20,20,2,,"#, *read_order,).into_bytes();
            mkv_header::DMKVCluster::new(ass, c_pts, 1)
        } else {
            let display_length = self.get_danmaku_display_length(n, d, ratio_scale).await;
            let avail_dc =
                self.get_avail_danmaku_channel(c_pts, display_length, channels).await.ok_or(anyhow!("ld err 1"))?;
            let ass = format!(
                r#"{4},0,Default,{5},0,0,0,,{{\alpha{0}\fs{7}\1c&{6}&\move(1920,{1},{2},{1})}}{8}{9}{3}"#,
                format_args!("{:02x}", (*self.cm.font_alpha.read().await * 255_f64) as u8),
                avail_dc * *self.font_size.read().await,
                0 - display_length as isize,
                &d,
                *read_order,
                &n,
                format!("{}{}{}", &c[4..6], &c[2..4], &c[0..2]),
                *self.font_size.read().await,
                if *self.show_nick.read().await { n } else { "" },
                if *self.show_nick.read().await { ": " } else { "" },
            )
            .into_bytes();
            mkv_header::DMKVCluster::new(ass, c_pts, *self.cm.danmaku_speed.read().await as usize)
        };
        *read_order = read_order.saturating_add(1);
        match cluster.write_to_socket(socket).await {
            Ok(_) => {}
            Err(_) => return Err(anyhow!("socket error")),
        };
        Ok(())
    }

    async fn init(&self) {
        *self.font_size.write().await = (40_f64 * *self.cm.font_scale.read().await) as usize;
        *self.channel_num.write().await = (540.0 / *self.font_size.read().await as f64).ceil() as usize;
    }

    pub async fn run_danmaku_client(
        self: &Arc<Self>,
        dtx: async_channel::Sender<(String, String, String)>,
    ) -> Result<()> {
        loop {
            match match self.cm.site {
                crate::config::Site::BiliLive => {
                    let b = bilibili::Bilibili::new();
                    b.run(&self.cm.room_url, dtx.clone()).await
                }
                crate::config::Site::BiliVideo => {
                    let b = bilivideo::Bilibili::new();
                    b.run(&self.cm.room_url, dtx.clone()).await
                }
                crate::config::Site::DouyuLive => {
                    let b = douyu::Douyu::new();
                    b.run(&self.cm.room_url, dtx.clone()).await
                }
                crate::config::Site::HuyaLive => {
                    let b = huya::Huya::new();
                    b.run(&self.cm.room_url, dtx.clone()).await
                }
                crate::config::Site::TwitchLive => {
                    let b = twitch::Twitch::new();
                    b.run(&self.cm.room_url, dtx.clone()).await
                }
                crate::config::Site::YoutubeLive => {
                    let b = youtube::Youtube::new();
                    b.run(&self.cm.room_url, dtx.clone()).await
                }
            } {
                Ok(_) => {}
                Err(e) => {
                    warn!("danmaku client error: {:?}", e);
                }
            };
            if dtx.is_closed() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        warn!("danmaku client exited.");
        Ok(())
    }

    pub async fn run_bilivideo(self: &Arc<Self>, ratio_scale: f64) -> Result<()> {
        self.init().await;
        // *self.channel_num.write().await = 30;
        info!("ratio: {}", &ratio_scale);
        let mut socket = self.ipc_manager.get_danmaku_socket().await?;
        let mut dchannels = vec![
            DanmakuChannel {
                length: 0,
                begin_pts: 0
            };
            30
        ];
        let (dtx, drx) = async_channel::unbounded();
        let cid = self.bili_video_cid.read().await.clone();
        let _ = spawn_local(async move {
            let b = bilivideo::Bilibili::new();
            match b
                .run(
                    // format!("https://comment.bilibili.com/{}.xml", cid).as_str(),
                    format!("http://api.bilibili.com/x/v1/dm/list.so?oid={}", cid).as_str(),
                    dtx.clone(),
                )
                .await
            {
                Ok(_) => {}
                Err(err) => {
                    info!("bilivideo danmaku err: {}", err)
                }
            }
        });

        socket.write_all(r#"[Script Info]
; Script generated by QLivePlayer
; https://github.com/THMonster/QLivePlayer
Title: Danmaku file
ScriptType: v4.00+
WrapStyle: 0
ScaledBorderAndShadow: yes
YCbCr Matrix: None
PlayResX: 1920
PlayResY: 1080

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Sans,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,1,0,7,0,0,0,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
"#.as_bytes()).await?;
        let mut dm_hashmap: BTreeMap<i64, (String, String, String)> = BTreeMap::new();
        while let Ok((c, n, d)) = drx.recv().await {
            let tmps: Vec<&str> = n.split(',').collect();
            dm_hashmap.insert(
                (tmps[0].parse::<f64>().unwrap() * 1000.0) as i64,
                (c.to_string(), tmps[1].to_string(), d.to_string()),
            );
        }
        for (k, (c, t, d)) in dm_hashmap.into_iter() {
            info!("{}-{}-{}-{}", &k, &c, &t, &d);
            let t1 = NaiveTime::from_hms_opt(0, 0, 0).unwrap() + Duration::milliseconds(k);
            let t2 = t1 + Duration::milliseconds(*self.cm.danmaku_speed.read().await as i64);
            let mut t1_s = t1.format("%k:%M:%S%.3f").to_string();
            let mut t2_s = t2.format("%k:%M:%S%.3f").to_string();
            t1_s.remove(t1_s.len() - 1);
            t2_s.remove(t2_s.len() - 1);
            if t.trim().eq("4") {
                socket
                    .write_all(
                        format!(
                            r#"Dialogue: 0,{4},{5},Default,,0,0,0,,{{\alpha{0}\fs{3}\1c&{2}&\an2}}{1}"#,
                            format_args!("{:02x}", (*self.cm.font_alpha.read().await * 255_f64) as u8),
                            &d,
                            format_args!("{}{}{}", &c[4..6], &c[2..4], &c[0..2]),
                            *self.font_size.read().await,
                            t1_s,
                            t2_s,
                        )
                        .as_bytes(),
                    )
                    .await?;
                socket.write_all("\n".as_bytes()).await?;
            } else if t.trim().eq("5") {
                socket
                    .write_all(
                        format!(
                            r#"Dialogue: 0,{4},{5},Default,,0,0,0,,{{\alpha{0}\fs{3}\1c&{2}&\an8}}{1}"#,
                            format_args!("{:02x}", (*self.cm.font_alpha.read().await * 255_f64) as u8),
                            &d,
                            format!("{}{}{}", &c[4..6], &c[2..4], &c[0..2]),
                            *self.font_size.read().await,
                            t1_s,
                            t2_s,
                        )
                        .as_bytes(),
                    )
                    .await?;
                socket.write_all("\n".as_bytes()).await?;
            } else {
                let display_length = self.get_danmaku_display_length("", &d, ratio_scale).await;
                let avail_dc = match self.get_avail_danmaku_channel(k as usize, display_length, &mut dchannels).await {
                    Some(it) => it,
                    None => {
                        continue;
                    }
                };
                let ass = format!(
                    r#"Dialogue: 0,{4},{5},Default,,0,0,0,,{{\alpha{0}\fs{7}\1c&{6}&\move(1920,{1},{2},{1})}}{3}"#,
                    format_args!("{:02x}", (*self.cm.font_alpha.read().await * 255_f64) as u8),
                    avail_dc * *self.font_size.read().await,
                    0 - display_length as isize,
                    &d,
                    t1_s,
                    t2_s,
                    format!("{}{}{}", &c[4..6], &c[2..4], &c[0..2]),
                    *self.font_size.read().await,
                );
                socket.write_all(ass.as_bytes()).await?;
                socket.write_all("\n".as_bytes()).await?;
            }
        }
        Ok(())
    }

    pub async fn run(self: &Arc<Self>, ratio_scale: f64) -> Result<()> {
        let now = std::time::Instant::now();
        self.init().await;
        let mut socket = self.ipc_manager.get_danmaku_socket().await?;
        let mut read_order = 0usize;
        let emoji_re = regex::Regex::new(
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
                    now.elapsed().as_millis() as usize,
                    ratio_scale,
                    &mut dchannels,
                    &mut read_order,
                    &mut socket,
                )
                .await;
        }
        'l1: loop {
            if let Ok((c, n, d)) = drx.try_recv() {
                if !self.cm.quiet {
                    println!("[{}] {}", &n, &d);
                }
                let d = Rc::new(d);
                if !self.fk.dm_check(d.clone()).await {
                    continue;
                }
                let d = emoji_re.replace_all(&d, "[em]");
                loop {
                    match self
                        .launch_danmaku(
                            &c,
                            &n,
                            &d,
                            now.elapsed().as_millis() as usize,
                            ratio_scale,
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
                                    now.elapsed().as_millis() as usize,
                                    ratio_scale,
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
                        now.elapsed().as_millis() as usize,
                        ratio_scale,
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
