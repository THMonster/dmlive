pub mod cmdparser;
use crate::config::Platform;
use crate::dmlerr;
use crate::ipcmanager::IPCManager;
use crate::{config::ConfigManager, dmlive::DMLMessage, utils::gen_ua};
use anyhow::Result;
use futures::StreamExt;
use log::info;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    net::UnixStream,
    process::Command,
};

pub struct MpvControl {
    last_rpc_ts: Cell<i64>,
    ipc_manager: Rc<IPCManager>,
    cm: Rc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
    mpv_command_tx: async_channel::Sender<String>,
    mpv_command_rx: async_channel::Receiver<String>,
}
impl MpvControl {
    pub fn new(cm: Rc<ConfigManager>, im: Rc<IPCManager>, mtx: async_channel::Sender<DMLMessage>) -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self {
            ipc_manager: im,
            cm,
            mtx,
            mpv_command_tx: tx,
            mpv_command_rx: rx,
            last_rpc_ts: Cell::new(0),
        }
    }

    pub async fn create_mpv_command(&self) -> Result<Command> {
        let mut ret = Command::new("mpv");
        if matches!(self.cm.site, crate::config::Site::BiliVideo) {
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
            self.ipc_manager.get_mpv_socket_path()
        ));
        Ok(ret)
    }

    pub async fn reload_edl_video(&self, stream_info: &HashMap<&str, String>) -> Result<()> {
        let edl = format!(
            "edl://!no_clip;!no_chapters;%{0}%{1};!new_stream;!no_clip;!no_chapters;%{2}%{3}",
            stream_info["url_a"].chars().count(),
            stream_info["url_a"],
            stream_info["url_v"].chars().count(),
            stream_info["url_v"]
        );
        info!("load video: {}--{}", &edl, self.cm.title.borrow());
        self.mpv_command_tx
            .send(format!(
                "{{ \"command\": [\"loadfile\", \"{}\"], \"async\": true }}\n",
                &edl
            ))
            .await?;
        self.mpv_command_tx
            .send(format!(
                "{{ \"command\": [\"set_property\", \"force-media-title\", \"{}\"] }}\n",
                self.cm.title.borrow().replace(r#"""#, r#"\""#)
            ))
            .await?;
        Ok(())
    }

    pub async fn reload_video(&self) -> Result<()> {
        if self.cm.plat == Platform::Android {
            Command::new("termux-open").arg(self.ipc_manager.get_f2m_socket_path()).spawn()?;
        } else {
            self.mpv_command_tx
                .send(format!(
                    "{{ \"command\": [\"loadfile\", \"{}\"] }}\n",
                    self.ipc_manager.get_f2m_socket_path()
                ))
                .await?;
        }
        Ok(())
    }

    // pub async fn quit(&self) -> Result<()> {
    //     self.mpv_command_tx.send("{ \"command\": [\"quit\"] }\n".into()).await?;
    //     Ok(())
    // }

    pub async fn stop(&self) -> Result<()> {
        self.mpv_command_tx.send("{ \"command\": [\"stop\"] }\n".into()).await?;
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
                { "command": ["keybind", "alt+f", "script-message dml:fps"] }
                "#
                .into(),
            )
            .await?;

        Ok(())
    }

    async fn handle_mpv_event(&self, line: String) -> Result<()> {
        let j: serde_json::Value = serde_json::from_str(&line)?;
        if let Some(rid) = j.pointer("/request_id") {
            if rid.as_u64().eq(&Some(114)) {
                let w = j.pointer("/data/w").ok_or_else(|| dmlerr!())?.as_u64().unwrap();
                let h = j.pointer("/data/h").ok_or_else(|| dmlerr!())?.as_u64().unwrap();
                if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                    let _ = self.mtx.send(DMLMessage::SetVideoInfo((w, h, 0))).await;
                    self.mpv_command_tx
                        .send(
                            r#"{ "command": ["sub-remove", "1"], "async": true }
                              "#
                            .into(),
                        )
                        .await?;
                    self.mpv_command_tx
                        .send(format!(
                            "{{ \"command\": [\"sub-add\", \"{}\"], \"async\": true }}\n",
                            self.ipc_manager.get_danmaku_socket_path()
                        ))
                        .await?;
                }
            } else if rid.as_u64().eq(&Some(514)) {
                match j.pointer("/data") {
                    Some(it) => match it.as_f64() {
                        Some(it) => {
                            self.cm.display_fps.set((it.round() as u64, self.cm.display_fps.get().1));
                        }
                        None => {}
                    },
                    None => {}
                }
            } else if rid.as_u64().eq(&Some(1919)) {
                match j.pointer("/data") {
                    Some(it) => match it.as_f64() {
                        Some(it) => {
                            if self.cm.display_fps.get().1 == 0 && it < 59.0 {
                                self.mpv_command_tx
                                    .send(
                                        r#"{ "command": ["set_property", "vf", "fps=fps=60:round=near"] }
                                        "#
                                        .into(),
                                    )
                                    .await?;
                            }
                        }
                        None => {}
                    },
                    None => {}
                }
            }
        }
        let event = j.pointer("/event").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        if event.eq("end-file") {
            if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                if j.pointer("/reason").ok_or_else(|| dmlerr!())?.as_str().unwrap().eq("eof") {
                    self.cm.bvideo_info.borrow_mut().current_page += 1;
                    let _ = self.mtx.send(DMLMessage::PlayVideo).await;
                }
            } else {
                // tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                // let _ = self.reload_video().await;
            }
        } else if event.eq("file-loaded") {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            let _ = self
                .mpv_command_tx
                .send(
                    r#"{ "command": ["get_property", "video-params"], "request_id": 114, "async": true } }
                    { "command": ["get_property", "display-fps"], "request_id": 514, "async": true }
                    { "command": ["get_property", "container-fps"], "request_id": 1919, "async": true }
                    "#
                    .into(),
                )
                .await;
        } else if event.eq("client-message") {
            let now = chrono::Utc::now().timestamp_millis();
            if now - self.last_rpc_ts.get() < 1000 {
                return Ok(());
            }
            self.last_rpc_ts.set(now);
            let cmds = cmdparser::CmdParser::new(
                j.pointer("/args/0").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
            );
            if cmds.restart {
                if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                    let _ = self.mtx.send(DMLMessage::PlayVideo).await;
                } else {
                    self.stop().await?;
                }
            }
            if cmds.fs.is_some() {
                let _ = self.mtx.send(DMLMessage::SetFontScale(cmds.fs.unwrap())).await;
            } else if cmds.fsup {
                let _ = self.mtx.send(DMLMessage::SetFontScale(self.cm.font_scale.get() + 0.15)).await;
            } else if cmds.fsdown {
                let _ = self.mtx.send(DMLMessage::SetFontScale(self.cm.font_scale.get() - 0.15)).await;
            }
            if cmds.fa.is_some() {
                let _ = self.mtx.send(DMLMessage::SetFontAlpha(cmds.fa.unwrap())).await;
            }
            if cmds.speed.is_some() {
                let _ = self.mtx.send(DMLMessage::SetDMSpeed(cmds.speed.unwrap())).await;
            }
            if cmds.page.is_some() {
                self.cm.bvideo_info.borrow_mut().current_page = cmds.page.unwrap() as usize;
                let _ = self.mtx.send(DMLMessage::PlayVideo).await;
            }
            if cmds.nick {
                let _ = self.mtx.send(DMLMessage::ToggleShowNick).await;
            }
            if cmds.back {
                let p = self.cm.bvideo_info.borrow().current_page.saturating_sub(1);
                self.cm.bvideo_info.borrow_mut().current_page = if p == 0 { 1 } else { p };
                let _ = self.mtx.send(DMLMessage::PlayVideo).await;
            }
            if cmds.next {
                self.cm.bvideo_info.borrow_mut().current_page += 1;
                let _ = self.mtx.send(DMLMessage::PlayVideo).await;
            }
            if cmds.fps {
                let fps: u64 = {
                    let df = self.cm.display_fps.get();
                    let i = df.1 as usize % 3;
                    [df.0, 0u64, 60u64][i]
                };
                if fps == 0 {
                    self.mpv_command_tx
                        .send(
                            r#"{ "command": ["set_property", "vf", ""] }
                            "#
                            .into(),
                        )
                        .await?;
                } else {
                    self.mpv_command_tx
                        .send(format!(
                            r#"{{ "command": ["set_property", "vf", "fps=fps={}:round=near"] }}
                        "#,
                            fps
                        ))
                        .await?;
                }
                let df = self.cm.display_fps.get();
                self.cm.display_fps.set((df.0, df.1.saturating_add(1)));
            }
        }
        Ok(())
    }

    pub async fn run_normal(&self) -> Result<()> {
        let mut mpv = self.create_mpv_command().await?.kill_on_drop(true).spawn().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        let s = UnixStream::connect(self.ipc_manager.get_mpv_socket_path()).await?;
        let (usocket_read, mut usocket_write) = tokio::io::split(s);
        let mpv_rpc_write_task = async {
            while let Ok(s) = self.mpv_command_rx.recv().await {
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
                        match msg {
                            Ok(it) => { tasks.push(self.handle_mpv_event(it.unwrap_or("".to_string()))); },
                            Err(_) => { return; },
                        }
                    }
                }
            }
        };
        let _ = self.init_mpv_rpc().await;
        // let _ = self.reload_video().await;
        tokio::select! {
            _ = mpv_rpc_write_task => {},
            _ = mpv_rpc_read_task => {},
            _ = mpv.wait() => {},
        }
        Ok(())
    }

    pub async fn run_android(&self) -> Result<()> {
        // Command::new("termux-open").arg(self.ipc_manager.get_f2m_socket_path()).spawn().unwrap();
        while let Ok(s) = self.mpv_command_rx.recv().await {
            if s.contains("quit") {
                break;
            }
        }
        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        match self.cm.run_mode {
            crate::config::RunMode::Play => {
                if self.cm.plat == Platform::Android {
                    self.run_android().await?;
                } else {
                    self.run_normal().await?;
                }
            }
            crate::config::RunMode::Record => {
                while let Ok(s) = self.mpv_command_rx.recv().await {
                    if s.contains("quit") {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
