pub mod cmdparser;

use anyhow::Result;
use futures::StreamExt;
use log::info;
use std::cell::Cell;
use std::rc::Rc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    net::UnixStream,
    process::Command,
};

use crate::config::Platform;
use crate::dmlerr;
use crate::dmlive::DMLContext;
use crate::{dmlive::DMLMessage, utils::gen_ua};

pub struct MpvControl {
    last_rpc_ts: Cell<i64>,
    mpv_command_tx: async_channel::Sender<String>,
    mpv_command_rx: async_channel::Receiver<String>,
    ctx: Rc<DMLContext>,
}
impl MpvControl {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self {
            mpv_command_tx: tx,
            mpv_command_rx: rx,
            last_rpc_ts: Cell::new(0),
            ctx,
        }
    }

    pub async fn create_mpv_command(&self) -> Result<Command> {
        let mut ret = Command::new("mpv");
        if matches!(self.ctx.cm.site, crate::config::Site::BiliVideo) {
            ret.arg(format!("--user-agent={}", gen_ua()))
                .arg("--http-header-fields-add=Referer: https://www.bilibili.com/");
        } else {
            ret.args(&["--cache=yes", "--cache-pause-initial=yes"]);
        }
        ret.args(&[
            "--loop=no",
            "--keep-open=no",
            "--idle=yes",
            "--player-operation-mode=pseudo-gui",
            "--sub=1",
        ])
        .arg(format!(
            "--input-ipc-server={}",
            self.ctx.im.get_mpv_socket_path()
        ));
        Ok(ret)
    }

    pub async fn reload_edl_video(&self) -> Result<()> {
        let edl = {
            let si = self.ctx.cm.stream_info.borrow();
            format!(
                "edl://!no_clip;!no_chapters;%{0}%{1};!new_stream;!no_clip;!no_chapters;%{2}%{3}",
                si["url_a"].chars().count(),
                si["url_a"],
                si["url_v"].chars().count(),
                si["url_v"]
            )
        };
        info!("load video: {edl}--{}", self.ctx.cm.title.borrow());
        self.mpv_command_tx
            .send(format!(
                r#"{{ "command": ["loadfile", "{edl}"], "async": true }}"#,
            ))
            .await?;
        self.mpv_command_tx
            .send(format!(
                r#"{{ "command": ["set_property", "force-media-title", "{}"] }}"#,
                self.ctx.cm.title.borrow().replace(r#"""#, r#"\""#)
            ))
            .await?;
        Ok(())
    }

    pub async fn reload_video(&self) -> Result<()> {
        if self.ctx.cm.plat == Platform::Android {
            Command::new("termux-open").arg(self.ctx.im.get_f2m_socket_path()).spawn()?;
        } else {
            self.mpv_command_tx
                .send(format!(
                    r#"{{ "command": ["loadfile", "{}"] }}"#,
                    self.ctx.im.get_f2m_socket_path()
                ))
                .await?;
        }
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.mpv_command_tx.send(r#"{ "command": [ "stop" ] }"#.to_string()).await?;
        Ok(())
    }

    pub async fn init_mpv_rpc(&self) -> Result<()> {
        self.mpv_command_tx
            .send(
                r#"{ "command": ["keybind", "alt+r", "script-message dml:r"] }
                { "command": ["keybind", "alt+z", "script-message dml:fsdown"] }
                { "command": ["keybind", "alt+x", "script-message dml:fsup"] }
                { "command": ["keybind", "alt+i", "script-message dml:nick"] }
                { "command": ["keybind", "alt+b", "script-message dml:back"] }
                { "command": ["keybind", "alt+n", "script-message dml:next"] }
                { "command": ["keybind", "alt+f", "script-message dml:fps"] }"#
                    .to_string(),
            )
            .await?;

        Ok(())
    }

    async fn handle_mpv_event(&self, line: String) -> Result<()> {
        let j: serde_json::Value = serde_json::from_str(&line)?;
        match j.pointer("/request_id").and_then(|x| x.as_u64()) {
            Some(114) => {
                let w = j.pointer("/data/w").and_then(|x| x.as_u64()).ok_or_else(|| dmlerr!())?;
                let h = j.pointer("/data/h").and_then(|x| x.as_u64()).ok_or_else(|| dmlerr!())?;
                if matches!(self.ctx.cm.site, crate::config::Site::BiliVideo) {
                    let _ = self.ctx.mtx.send(DMLMessage::SetVideoInfo((w, h, 0))).await;
                    self.mpv_command_tx.send(r#"{ "command": ["sub-remove", 1], "async": true }"#.to_string()).await?;
                    self.mpv_command_tx
                        .send(format!(
                            r#"{{ "command": ["sub-add", "{}"], "async": true }}"#,
                            self.ctx.im.get_danmaku_socket_path()
                        ))
                        .await?;
                }
            }
            Some(514) => {
                if let Some(it) = j.pointer("/data").and_then(|x| x.as_f64()) {
                    self.ctx.cm.display_fps.set((it.round() as u64, self.ctx.cm.display_fps.get().1));
                }
            }
            Some(1919) => {
                if let Some(it) = j.pointer("/data").and_then(|x| x.as_f64()) {
                    if self.ctx.cm.display_fps.get().1 == 0 && it < 59.0 {
                        self.mpv_command_tx
                            .send(r#"{ "command": ["set_property", "vf", "fps=fps=60:round=near"] }"#.to_string())
                            .await?;
                    }
                }
            }
            _ => {}
        }
        let event = j.pointer("/event").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        if event.eq("end-file") {
            match self.ctx.cm.site_type {
                crate::config::SiteType::Live => {
                    self.ctx.mtx.send(DMLMessage::RequestRestart).await?;
                }
                crate::config::SiteType::Video => {
                    info!("{j:?}");
                    if j.pointer("/reason").and_then(|x| x.as_str()).eq(&Some("eof")) {
                        self.ctx.cm.bvideo_info.borrow_mut().current_page += 1;
                    }
                    self.ctx.mtx.send(DMLMessage::RequestRestart).await?;
                }
            }
        } else if event.eq("file-loaded") {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            let _ = self
                .mpv_command_tx
                .send(
                    r#"{ "command": ["get_property", "video-params"], "request_id": 114, "async": true }
                    { "command": ["get_property", "display-fps"], "request_id": 514, "async": true }
                    { "command": ["get_property", "container-fps"], "request_id": 1919, "async": true }"#
                        .to_string(),
                )
                .await;
        } else if event.eq("client-message") {
            let now = chrono::Utc::now().timestamp_millis();
            if now - self.last_rpc_ts.get() < 1000 {
                return Ok(());
            }
            self.last_rpc_ts.set(now);
            let cmds =
                cmdparser::CmdParser::new(j.pointer("/args/0").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?);
            if cmds.restart {
                self.stop().await?;
            }
            if cmds.fs.is_some() {
                let _ = self.ctx.mtx.send(DMLMessage::SetFontScale(cmds.fs.unwrap())).await;
            } else if cmds.fsup {
                let _ = self
                    .ctx
                    .mtx
                    .send(DMLMessage::SetFontScale(
                        self.ctx.cm.font_scale.get() + 0.15,
                    ))
                    .await;
            } else if cmds.fsdown {
                let _ = self
                    .ctx
                    .mtx
                    .send(DMLMessage::SetFontScale(
                        self.ctx.cm.font_scale.get() - 0.15,
                    ))
                    .await;
            }
            if cmds.fa.is_some() {
                let _ = self.ctx.mtx.send(DMLMessage::SetFontAlpha(cmds.fa.unwrap())).await;
            }
            if cmds.speed.is_some() {
                let _ = self.ctx.mtx.send(DMLMessage::SetDMSpeed(cmds.speed.unwrap())).await;
            }
            if cmds.page.is_some() {
                self.ctx.cm.bvideo_info.borrow_mut().current_page = cmds.page.unwrap() as usize;
                let _ = self.ctx.mtx.send(DMLMessage::PlayVideo).await;
            }
            if cmds.nick {
                let _ = self.ctx.mtx.send(DMLMessage::ToggleShowNick).await;
            }
            if cmds.back {
                let p = self.ctx.cm.bvideo_info.borrow().current_page.saturating_sub(1);
                self.ctx.cm.bvideo_info.borrow_mut().current_page = if p == 0 { 1 } else { p };
                let _ = self.ctx.mtx.send(DMLMessage::PlayVideo).await;
            }
            if cmds.next {
                self.ctx.cm.bvideo_info.borrow_mut().current_page += 1;
                let _ = self.ctx.mtx.send(DMLMessage::PlayVideo).await;
            }
            if cmds.fps {
                let fps: u64 = {
                    let df = self.ctx.cm.display_fps.get();
                    let i = df.1 as usize % 3;
                    [df.0, 0u64, 60u64][i]
                };
                if fps == 0 {
                    self.mpv_command_tx.send(r#"{ "command": ["set_property", "vf", ""] }"#.into()).await?;
                } else {
                    self.mpv_command_tx
                        .send(format!(
                            r#"{{ "command": ["set_property", "vf", "fps=fps={fps}:round=near"] }}"#,
                        ))
                        .await?;
                }
                let df = self.ctx.cm.display_fps.get();
                self.ctx.cm.display_fps.set((df.0, df.1.saturating_add(1)));
            }
        }
        Ok(())
    }

    pub async fn run_normal(&self) -> Result<()> {
        let mut mpv = self.create_mpv_command().await?.kill_on_drop(true).spawn().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        let s = UnixStream::connect(self.ctx.im.get_mpv_socket_path()).await?;
        let (usocket_read, mut usocket_write) = tokio::io::split(s);
        let mpv_rpc_write_task = async {
            while let Ok(mut s) = self.mpv_command_rx.recv().await {
                s.push('\n');
                let _ = usocket_write.write_all(s.as_bytes()).await;
            }
        };
        let mpv_rpc_read_task = async {
            let mut tasks = futures::stream::FuturesUnordered::new();
            let mut reader = tokio::io::BufReader::new(usocket_read).lines();
            loop {
                tokio::select! {
                    Some(_) = tasks.next() => {},
                    msg = reader.next_line() => {
                        if let Some(it) = msg? {
                            tasks.push(self.handle_mpv_event(it));
                        }
                    }
                }
            }
            #[allow(unreachable_code)]
            anyhow::Ok(())
        };
        let _ = self.init_mpv_rpc().await;
        tokio::select! {
            _ = mpv_rpc_write_task => {},
            _ = mpv_rpc_read_task => {},
            _ = mpv.wait() => {},
        }
        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        let mode = match self.ctx.cm.run_mode {
            crate::config::RunMode::Play => match self.ctx.cm.plat {
                Platform::Android => 1,
                _ => 0,
            },
            crate::config::RunMode::Record => 0,
        };
        if mode == 0 {
            self.run_normal().await?;
        } else {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
}
