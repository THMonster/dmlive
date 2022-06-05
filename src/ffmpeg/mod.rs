use anyhow::anyhow;
use anyhow::Result;
use log::info;
use log::warn;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::net::TcpStream;
use tokio::task::spawn_local;
use tokio::{
    io::{
        AsyncBufReadExt,
        AsyncWriteExt,
    },
    process::Command,
    sync::RwLock,
};

use crate::{
    config::ConfigManager,
    dmlive::DMLMessage,
};

pub struct FfmpegControl {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    ff_command_tx: RwLock<Option<async_channel::Sender<bool>>>,
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
        let tcp_addr = self.ipc_manager.get_f2m_socket_path()[6..].to_string();
        spawn_local(async move {
            loop {
                let socket = match TcpStream::connect(&tcp_addr).await {
                    Ok(it) => it,
                    Err(_) => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                        continue;
                    }
                };
                let now = chrono::Local::now();
                let mut file = match OpenOptions::new()
                    .read(false)
                    .write(true)
                    .create(true)
                    .open(format!(
                        "{} - {}.mkv",
                        title.replace("/", "-"),
                        now.format("%F %T")
                    ))
                    .await
                {
                    Ok(it) => it,
                    Err(e) => {
                        warn!("open record file failed: {}", e);
                        break;
                    }
                };

                let mut socket = tokio::io::BufReader::with_capacity(3 * 1024, socket);
                let _ = tokio::io::copy_buf(&mut socket, &mut file).await;
                return;
            }
        })
    }

    pub async fn create_ff_command(&self, title: &str, rurl: &Vec<String>) -> Result<Command> {
        let stream_type = &*self.cm.stream_type.read().await;
        let mut ret = Command::new("ffmpeg");
        ret.args(&["-y", "-xerror"]);
        ret.arg("-hide_banner");
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
        self.ff_command_tx.read().await.as_ref().ok_or(anyhow!("ffmpeg quit err 1"))?.send(true).await?;
        Ok(())
    }

    pub async fn run(self: &Arc<Self>, title: &str, rurl: &Vec<String>) -> Result<()> {
        let mut ff = self
            .create_ff_command(&title, rurl)
            .await?
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(false)
            .spawn()
            .unwrap();
        let mut ffstdin = ff.stdin.take().unwrap();
        let ffstderr = ff.stderr.take().unwrap();
        let (tx, rx) = async_channel::unbounded();
        *self.ff_command_tx.write().await = Some(tx);
        tokio::task::spawn_local(async move {
            while let Ok(_) = rx.recv().await {
                info!("close ffmpeg");
                let _ = ffstdin.write_all("q\n".as_bytes()).await;
            }
        });
        let s1 = self.clone();
        tokio::task::spawn_local(async move {
            let mut reader = tokio::io::BufReader::new(ffstderr).lines();
            let res_re = regex::Regex::new(r#"Stream #[0-9].+? Video:.*?\D(\d{3,5})x(\d{2,5})\D.*"#).unwrap();
            while let Some(line) = reader.next_line().await.unwrap() {
                info!("{}", &line);
                match res_re.captures(&line) {
                    Some(it) => {
                        info!("{}", &line);
                        let w = it[1].parse().unwrap();
                        let h = it[2].parse().unwrap();
                        if w < 100 || h < 100 {
                            let _ = s1.quit().await;
                        }
                        let _ = s1.mtx.send(DMLMessage::SetVideoRes((w, h))).await;
                        break;
                    }
                    None => {}
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
