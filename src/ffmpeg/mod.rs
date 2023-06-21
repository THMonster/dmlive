use anyhow::anyhow;
use anyhow::Result;
use log::info;
use log::warn;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::spawn_local;
use tokio::time::timeout;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::Command,
    sync::RwLock,
};

use crate::{config::ConfigManager, dmlive::DMLMessage};

pub struct FfmpegControl {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    ff_command_tx: RwLock<Option<oneshot::Sender<bool>>>,
    mtx: async_channel::Sender<DMLMessage>,
}
impl FfmpegControl {
    pub fn new(
        cm: Arc<ConfigManager>,
        im: Arc<crate::ipcmanager::IPCManager>,
        mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        Self {
            ipc_manager: im,
            cm,
            ff_command_tx: RwLock::new(None),
            mtx,
        }
    }

    async fn run_write_record_task(&self, title: String) -> tokio::task::JoinHandle<()> {
        let in_stream = self.ipc_manager.get_f2m_socket_path();
        let max_len = match title.char_indices().nth(70) {
            Some(it) => it.0,
            None => title.len(),
        };
        spawn_local(async move {
            let now = chrono::Local::now();
            let filename = format!(
                "{} - {}.mkv",
                title[..max_len].replace('/', "-"),
                now.format("%F %T")
            );
            loop {
                let mut cmd = Command::new("ffmpeg");
                cmd.args(&["-y", "-xerror", "-hide_banner", "-nostats", "-nostdin"]);
                cmd.arg("-i");
                cmd.arg(&in_stream);
                cmd.args(&["-c", "copy", "-f", "matroska"]);
                cmd.arg(&filename);
                let mut ff = cmd
                    .stdin(std::process::Stdio::null())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(false)
                    .spawn()
                    .unwrap();
                let mut reader = tokio::io::BufReader::new(ff.stderr.take().unwrap()).lines();
                let mut retry = false;
                while let Some(line) = reader.next_line().await.unwrap_or(None) {
                    info!("{}", &line);
                    if line.contains("Connection refused") {
                        retry = true;
                    }
                }
                let _ = ff.wait().await;
                if retry == true {
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    continue;
                } else {
                    return;
                }
            }
        })
    }

    pub async fn create_ff_command(&self, title: &str, rurl: &Vec<String>) -> Result<Command> {
        let stream_type = &*self.cm.stream_type.read().await;
        let mut ret = Command::new("ffmpeg");
        ret.args(&["-y", "-xerror"]);
        ret.arg("-hide_banner");
        ret.arg("-nostats");
        // ret.arg("-report");
        // ret.arg("-loglevel").arg("quiet");
        match stream_type {
            crate::config::StreamType::DASH => {
                if matches!(self.cm.site, crate::config::Site::BiliVideo) {
                    ret.args(&[
                        "-user_agent",
                        &crate::utils::gen_ua(),
                        "-headers",
                        "Referer: https://www.bilibili.com/",
                    ]);
                    ret.arg("-i").arg(&rurl[1]);
                    ret.args(&[
                        "-user_agent",
                        &crate::utils::gen_ua(),
                        "-headers",
                        "Referer: https://www.bilibili.com/",
                    ]);
                    ret.arg("-i").arg(&rurl[2]);
                } else {
                    ret.arg("-i").arg(self.ipc_manager.get_video_socket_path());
                    ret.arg("-i").arg(self.ipc_manager.get_audio_socket_path());
                }
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
        ret.args(&["-reserve_index_space", " 1024000"]);
        match self.cm.run_mode {
            crate::config::RunMode::Play => {
                ret.arg("-listen").arg("1").arg(self.ipc_manager.get_f2m_socket_path());
            }
            crate::config::RunMode::Record => {
                match self.cm.http_address.as_ref() {
                    Some(it) => {
                        ret.arg("-listen").arg("1").arg(it);
                    }
                    None => {
                        ret.arg("-listen").arg("1").arg(self.ipc_manager.get_f2m_socket_path());
                    }
                };
            }
        }
        Ok(ret)
    }

    pub async fn quit(&self) -> Result<()> {
        match self.ff_command_tx.write().await.take().ok_or(anyhow!("ffmpeg quit err 1"))?.send(true) {
            Ok(_) => {}
            Err(_) => {}
        };
        Ok(())
    }

    pub async fn run(self: &Arc<Self>, title: &str, rurl: &Vec<String>) -> Result<()> {
        let mut ff = self
            .create_ff_command(title, rurl)
            .await?
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(false)
            .spawn()
            .unwrap();
        let mut ffstdin = ff.stdin.take().unwrap();
        let ffstderr = ff.stderr.take().unwrap();
        let (tx, rx) = oneshot::channel();
        *self.ff_command_tx.write().await = Some(tx);
        tokio::task::spawn_local(async move {
            match rx.await {
                Ok(_) => {
                    info!("close ffmpeg");
                    let _ = ffstdin.write_all("q\n".as_bytes()).await;
                }
                Err(_) => {}
            }
        });

        let s1 = self.clone();
        tokio::task::spawn_local(async move {
            let mut reader = tokio::io::BufReader::new(ffstderr).lines();
            let s11 = s1.clone();
            match timeout(tokio::time::Duration::from_secs(10), async move {
                let res_re = regex::Regex::new(r#"Stream #[0-9].+? Video:.*?\D(\d{3,5})x(\d{2,5})\D.*"#).unwrap();
                while let Some(line) = reader.next_line().await.unwrap_or(None) {
                    info!("{}", &line);
                    match res_re.captures(&line) {
                        Some(it) => {
                            info!("{}", &line);
                            let w = it[1].parse().unwrap();
                            let h = it[2].parse().unwrap();
                            if w < 100 || h < 100 {
                                let _ = s11.quit().await;
                            }
                            let _ = s11.mtx.send(DMLMessage::SetVideoRes((w, h))).await;
                            return;
                        }
                        None => {}
                    }
                }
            })
            .await
            {
                Ok(_) => {}
                Err(_) => {
                    warn!("set video resolution failed!");
                    let _ = s1.quit().await;
                }
            }
        });

        let mut write_record_task: Option<tokio::task::JoinHandle<()>> = None;
        match self.cm.run_mode {
            crate::config::RunMode::Play => {}
            crate::config::RunMode::Record => {
                if self.cm.http_address.is_none() {
                    write_record_task = Some(self.run_write_record_task(title.into()).await);
                }
            }
        }
        ff.wait().await?;
        match write_record_task {
            Some(it) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                it.abort()
            }
            None => {}
        }
        Ok(())
    }
}
