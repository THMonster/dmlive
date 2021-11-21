use crate::{config::ConfigManager, dmlive::DMLMessage};
use anyhow::*;
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

    pub async fn reload_video(&self, file_path: &str) -> Result<()> {
        self.mpv_command_tx
            .send(format!(
                r#"{{ "command": ["loadfile", "{}"] }}\n"#,
                &file_path
            ))
            .await?;
        Ok(())
    }

    pub async fn run(self: &Arc<Self>, title: &str) -> Result<()> {
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
                println!("{}", line);
            }
        });
        mpv.wait().await?;
        Ok(())
    }
}
