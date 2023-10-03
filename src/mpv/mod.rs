pub mod cmdparser;

use crate::{
    config::ConfigManager,
    dmlive::DMLMessage,
    utils::gen_ua,
};
use anyhow::anyhow;
use anyhow::Result;
use log::info;
use std::sync::Arc;
use tokio::{
    io::{
        AsyncBufReadExt,
        AsyncWriteExt,
    },
    net::UnixStream,
    process::Command,
};

pub struct MpvControl {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
    mpv_command_tx: async_channel::Sender<String>,
    mpv_command_rx: async_channel::Receiver<String>,
}
impl MpvControl {
    pub fn new(
        cm: Arc<ConfigManager>,
        im: Arc<crate::ipcmanager::IPCManager>,
        mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self {
            ipc_manager: im,
            cm,
            mtx,
            mpv_command_tx: tx,
            mpv_command_rx: rx,
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
        ])
        .arg(format!(
            "--input-ipc-server={}",
            self.ipc_manager.get_mpv_socket_path()
        ));
        Ok(ret)
    }

    pub async fn reload_edl_video(&self, urls: &Vec<String>, title: &str) -> Result<()> {
        let edl = format!(
            "edl://!no_clip;!no_chapters;%{0}%{1};!new_stream;!no_clip;!no_chapters;%{2}%{3}",
            urls[2].chars().count(),
            urls[2],
            urls[1].chars().count(),
            urls[1]
        );
        info!("{}--{}", &edl, title);
        self.mpv_command_tx
            .send(format!(
                "{{ \"command\": [\"loadfile\", \"{}\"], \"async\": true }}\n",
                &edl
            ))
            .await?;
        self.mpv_command_tx
            .send(format!(
                "{{ \"command\": [\"set_property\", \"force-media-title\", \"{}\"] }}\n",
                title.replace(r#"""#, r#"\""#)
            ))
            .await?;
        Ok(())
    }

    pub async fn reload_video(&self) -> Result<()> {
        self.mpv_command_tx
            .send(format!(
                "{{ \"command\": [\"loadfile\", \"{}\"] }}\n            ",
                self.ipc_manager.get_f2m_socket_path()
            ))
            .await?;
        Ok(())
    }

    pub async fn quit(&self) -> Result<()> {
        self.mpv_command_tx.send("{ \"command\": [\"quit\"] }\n".into()).await?;
        Ok(())
    }

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

    async fn handle_mpv_event(self: &Arc<Self>, line: &str, last_time: &mut i64) -> Result<()> {
        let j: serde_json::Value = serde_json::from_str(line)?;
        if let Some(rid) = j.pointer("/request_id") {
            if rid.as_u64().eq(&Some(114)) {
                let w = j.pointer("/data/w").ok_or(anyhow!("hme err a1"))?.as_u64().unwrap();
                let h = j.pointer("/data/h").ok_or(anyhow!("hme err a2"))?.as_u64().unwrap();
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
                            self.cm.display_fps.write().await.0 = it.round() as u64;
                        }
                        None => {}
                    },
                    None => {}
                }
            } else if rid.as_u64().eq(&Some(1919)) {
                match j.pointer("/data") {
                    Some(it) => match it.as_f64() {
                        Some(it) => {
                            if self.cm.display_fps.read().await.1 == 0 && it < 59.0 {
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
        let event = j.pointer("/event").ok_or(anyhow!("hme err 1"))?.as_str().ok_or(anyhow!("hme err 2"))?;
        if event.eq("end-file") {
            if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                if j.pointer("/reason").ok_or(anyhow!("hme err 9"))?.as_str().unwrap().eq("eof") {
                    let _ = self
                        .mtx
                        .send(DMLMessage::GoToBVPage(
                            self.cm.bvideo_info.read().await.current_page + 1,
                        ))
                        .await;
                }
            } else {
                let s1 = self.clone();
                tokio::task::spawn_local(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    let _ = s1.reload_video().await;
                });
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
        } else if event.eq("client-message") && (chrono::Utc::now().timestamp_millis() - *last_time > 1000) {
            *last_time = chrono::Utc::now().timestamp_millis();
            let cmds = cmdparser::CmdParser::new(
                j.pointer("/args/0").ok_or(anyhow!("hme err 3"))?.as_str().ok_or(anyhow!("hme err 4"))?,
            );
            if cmds.restart {
                if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                    let p = self.cm.bvideo_info.read().await.current_page;
                    let _ = self.mtx.send(DMLMessage::GoToBVPage(p)).await;
                } else {
                    self.stop().await?;
                }
            }
            if cmds.fs.is_some() {
                let _ = self.mtx.send(DMLMessage::SetFontScale(cmds.fs.unwrap())).await;
            } else if cmds.fsup {
                let _ = self
                    .mtx
                    .send(DMLMessage::SetFontScale(
                        *self.cm.font_scale.read().await + 0.15,
                    ))
                    .await;
            } else if cmds.fsdown {
                let _ = self
                    .mtx
                    .send(DMLMessage::SetFontScale(
                        *self.cm.font_scale.read().await - 0.15,
                    ))
                    .await;
            }
            if cmds.fa.is_some() {
                let _ = self.mtx.send(DMLMessage::SetFontAlpha(cmds.fa.unwrap())).await;
            }
            if cmds.speed.is_some() {
                let _ = self.mtx.send(DMLMessage::SetDMSpeed(cmds.speed.unwrap())).await;
            }
            if cmds.page.is_some() {
                let _ = self.mtx.send(DMLMessage::GoToBVPage(cmds.page.unwrap() as usize)).await;
            }
            if cmds.nick {
                let _ = self.mtx.send(DMLMessage::ToggleShowNick).await;
            }
            if cmds.back {
                let p = self.cm.bvideo_info.read().await.current_page.saturating_sub(1);
                let p = if p == 0 { 1 } else { p };
                let _ = self.mtx.send(DMLMessage::GoToBVPage(p)).await;
            }
            if cmds.next {
                let p = self.cm.bvideo_info.read().await.current_page.saturating_add(1);
                let _ = self.mtx.send(DMLMessage::GoToBVPage(p)).await;
            }
            if cmds.fps {
                let fps: u64 = {
                    let df = self.cm.display_fps.read().await;
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
                self.cm.display_fps.write().await.1 += 1;
            }
        }
        Ok(())
    }

    pub async fn run_normal(self: &Arc<Self>) -> Result<()> {
        let mut mpv = self.create_mpv_command().await?.kill_on_drop(true).spawn().unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        let s = UnixStream::connect(self.ipc_manager.get_mpv_socket_path()).await?;
        let (usocket_read, mut usocket_write) = tokio::io::split(s);
        let s1 = self.clone();
        tokio::task::spawn_local(async move {
            while let Ok(s) = s1.mpv_command_rx.recv().await {
                let _ = usocket_write.write_all(s.as_bytes()).await;
            }
        });
        let s2 = self.clone();
        tokio::task::spawn_local(async move {
            let mut last_time = chrono::Utc::now().timestamp_millis();
            let mut reader = tokio::io::BufReader::new(usocket_read).lines();
            while let Some(line) = reader.next_line().await? {
                info!("mpv rpc: {}", &line);
                let _ = s2.handle_mpv_event(&line, &mut last_time).await;
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        });
        let _ = self.init_mpv_rpc().await;
        if !matches!(self.cm.site, crate::config::Site::BiliVideo) {
            let _ = self.reload_video().await;
        }
        mpv.wait().await?;
        Ok(())
    }

    pub async fn run_android(self: &Arc<Self>) -> Result<()> {
        'l1: loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            let mut ns = Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "netstat -apn | grep ffmpeg | grep {}",
                    self.ipc_manager.get_f2m_socket_path().trim_start_matches("tcp://")
                ))
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let o = ns.stdout.take().unwrap();
            let mut reader = tokio::io::BufReader::new(o).lines();
            while let Some(line) = reader.next_line().await.unwrap() {
                if line.contains(self.ipc_manager.get_f2m_socket_path().trim_start_matches("tcp://")) {
                    break 'l1;
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        Command::new("termux-open").arg(self.ipc_manager.get_f2m_socket_path()).spawn().unwrap();
        while let Ok(s) = self.mpv_command_rx.recv().await {
            if s.contains("quit") {
                break;
            }
        }
        Ok(())
    }

    pub async fn run(self: &Arc<Self>) -> Result<()> {
        match self.cm.run_mode {
            crate::config::RunMode::Play => {
                if cfg!(target_os = "android") {
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
