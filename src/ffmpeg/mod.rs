use std::sync::Arc;

use anyhow::*;
use tokio::{io::AsyncWriteExt, process::Command};

use crate::{config::ConfigManager, dmlive::DMLMessage};

pub struct FfmpegControl {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
}
impl FfmpegControl {
    pub fn new(
        cm: Arc<ConfigManager>,
        im: Arc<crate::ipcmanager::IPCManager>,
        _mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        Self { ipc_manager: im, cm }
    }

    pub async fn create_ff_command(&self, title: &str) -> Result<Command> {
        let stream_type = &*self.cm.stream_type.read().await;
        let mut ret = Command::new("ffmpeg");
        ret.args(&["-y", "-xerror"]);
        // ret.arg("-report");
        ret.arg("-loglevel").arg("quiet");
        match stream_type {
            crate::config::StreamType::DASH => {
                ret.arg("-i").arg(self.ipc_manager.get_video_socket_path());
                ret.arg("-i").arg(self.ipc_manager.get_audio_socket_path());
                ret.arg("-i").arg(self.ipc_manager.get_danmaku_socket_path());
                ret.args(&["-map", "0:v:0", "-map", "1:a:0", "-map", "2:s:0"]);
            }
            _ => {
                ret.arg("-i").arg(self.ipc_manager.get_stream_socket_path());
                ret.arg("-i").arg(self.ipc_manager.get_danmaku_socket_path());
                ret.args(&["-map", "0:v:0", "-map", "0:a:0", "-map", "1:s:0"]);
            }
        }
        ret.args(&["-c", "copy"]);
        if matches!(stream_type, crate::config::StreamType::HLS) {
            ret.args(&["-c:a", "pcm_s16le"]);
        }
        ret.args(&["-metadata", format!("title={}", &title).as_str(), "-f", "matroska"]);
        ret.arg("-listen").arg("1").arg(self.ipc_manager.get_f2m_socket_path());
        Ok(ret)
    }

    pub async fn run(self: &Arc<Self>, title: &str) -> Result<()> {
        let mut ff = self
            .create_ff_command(&title)
            .await?
            .stdin(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut ffstdin = ff.stdin.take().unwrap();
        // tokio::task::spawn_local(async move {
        //     tokio::time::sleep(tokio::time::Duration::from_millis(10000)).await;
        //     let _ = ffstdin.write_all("q\n".as_bytes()).await;
        // });
        ff.wait().await?;
        Ok(())
    }
}
