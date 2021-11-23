pub mod cmdparser;

use crate::{config::ConfigManager, dmlive::DMLMessage};
use anyhow::*;
use log::info;
use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
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

    pub async fn create_mpv_command(&self, tp: u8) -> Result<Command> {
        let mut ret = Command::new("mpv");
        if tp == 0 {
            ret.args(&[
                "--idle=yes",
                "--player-operation-mode=pseudo-gui",
                "--cache=yes",
                "--cache-pause-initial=yes",
                r#"--vf=lavfi="fps=60""#,
            ])
            .arg(format!(
                "--input-ipc-server={}",
                self.ipc_manager.get_mpv_socket_path()
            ));
        } else {
            todo!()
        }
        Ok(ret)
    }

    pub async fn reload_video(&self) -> Result<()> {
        self.mpv_command_tx
            .send(format!(
                "{{ \"command\": [\"loadfile\", \"{}\"] }}\n            ",
                self.ipc_manager.get_f2m_socket_path()
            ))
            .await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        let _ = self.mpv_command_tx.send("{ \"command\": [\"get_property\", \"video-params\"] }\n".into()).await;
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
                r#"{ "command": ["keybind", "alt+r", "script-message qlp:r"] }
{ "command": ["keybind", "alt+z", "script-message qlp:fsdown"] }
{ "command": ["keybind", "alt+x", "script-message qlp:fsup"] }
{ "command": ["keybind", "alt+i", "script-message qlp:nick"] }
"#
                .into(),
            )
            .await?;

        Ok(())
    }

    async fn handle_mpv_event(self: &Arc<Self>, line: &str) -> Result<()> {
        let j: serde_json::Value = serde_json::from_str(&line)?;
        let event = j.pointer("/event").ok_or(anyhow!("hme err 1"))?.as_str().ok_or(anyhow!("hme err 2"))?;
        if event.eq("end-file") {
            let s1 = self.clone();
            tokio::task::spawn_local(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                let _ = s1.reload_video().await;
            });
        } else if event.eq("client-message") {
            let cmds = cmdparser::CmdParser::new(
                j.pointer("/args/0").ok_or(anyhow!("hme err 3"))?.as_str().ok_or(anyhow!("hme err 4"))?,
            );
            if cmds.restart {
                self.stop().await?;
            }
        }
        Ok(())
    }

    pub async fn run(self: &Arc<Self>) -> Result<()> {
        let mut mpv = self.create_mpv_command(0).await?.kill_on_drop(true).spawn().unwrap();
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
            let mut reader = tokio::io::BufReader::new(usocket_read).lines();
            while let Some(line) = reader.next_line().await.unwrap() {
                info!("mpv rpc: {}", &line);
                let _ = s2.handle_mpv_event(&line).await;
            }
        });
        let _ = self.init_mpv_rpc().await;
        let _ = self.reload_video().await;
        mpv.wait().await?;
        Ok(())
    }
}
