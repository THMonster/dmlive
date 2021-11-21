use std::sync::Arc;

use anyhow::*;
use tokio::process::Command;

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
        let mut ret = Command::new("ffmpeg");
        ret.args(&["-y", "-xerror"]).arg("-loglevel").arg("quiet");
        // ret.arg("-report");
        if self.ipc_manager.is_dash {
            ret.arg("-i").arg(self.ipc_manager.get_video_socket_path());
            ret.arg("-i").arg(self.ipc_manager.get_audio_socket_path());
        } else {
            ret.arg("-i").arg(self.ipc_manager.get_stream_socket_path());
        }
        ret.arg("-i").arg(self.ipc_manager.get_danmaku_socket_path());
        ret.args(&["-map", "v:0", "-map", "a:0", "-map", "s:0"]);
        ret.args(&[
            "-c",
            "copy",
            "-metadata",
            format!("title={}", &title).as_str(),
            "-f",
            "matroska",
        ]);
        ret.arg("-listen").arg("1").arg(self.ipc_manager.get_f2m_socket_path());
        Ok(ret)
    }

    pub async fn run(self: &Arc<Self>, title: &str) -> Result<()> {
        let mut ff = self.create_ff_command(&title).await?.kill_on_drop(true).spawn().unwrap();
        ff.wait().await?;
        Ok(())
    }
}
