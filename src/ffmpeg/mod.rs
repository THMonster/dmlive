use crate::config::{RunMode, Site, StreamType};
use crate::ipcmanager::IPCManager;
use crate::{config::ConfigManager, dmlive::DMLMessage};
use anyhow::anyhow;
use anyhow::Result;
use log::info;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::io::{AsyncRead, BufReader};
use tokio::process::ChildStdin;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::Command,
};

pub struct FfmpegControl {
    ipc_manager: Rc<IPCManager>,
    cm: Rc<ConfigManager>,
    ff_stdin: RefCell<Option<ChildStdin>>,
    mtx: async_channel::Sender<DMLMessage>,
}
impl FfmpegControl {
    pub fn new(cm: Rc<ConfigManager>, im: Rc<IPCManager>, mtx: async_channel::Sender<DMLMessage>) -> Self {
        Self {
            ipc_manager: im,
            cm,
            mtx,
            ff_stdin: RefCell::new(None),
        }
    }

    pub async fn write_record_task(&self) -> Result<()> {
        let in_stream = self.ipc_manager.get_f2m_socket_path();
        let max_len = match self.cm.title.borrow().char_indices().nth(70) {
            Some(it) => it.0,
            None => self.cm.title.borrow().len(),
        };
        let now = chrono::Local::now();
        let filename = format!(
            "{} - {}.mkv",
            self.cm.title.borrow()[..max_len].replace('/', "-"),
            now.format("%F %T")
        );
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-y", "-hide_banner", "-nostdin"]);
        cmd.arg("-i");
        cmd.arg(&in_stream);
        cmd.args(["-c", "copy", "-f", "matroska"]);
        cmd.arg(&filename);
        let mut ff = cmd
            .stdin(std::process::Stdio::null())
            // .stderr(std::process::Stdio::null())
            .kill_on_drop(false)
            .spawn()
            .unwrap();
        let _ = ff.wait().await;
        Ok(())
    }

    pub fn create_pre_ff_command(&self) -> Result<Command> {
        let mut ret = Command::new("ffmpeg");
        ret.args(["-y", "-xerror"]);
        ret.arg("-hide_banner");
        ret.arg("-nostats");
        // ret.args(["-fflags", "+nobuffer"]);
        ret.args(["-probesize", "204800"]);
        ret.arg("-i").arg(self.ipc_manager.get_video_socket_path());
        ret.args(["-map", "0:v:0?", "-map", "0:a:0?"]);
        ret.args(["-c", "copy"]);
        ret.args(["-f", "flv", "-"]);
        Ok(ret)
    }

    pub fn create_ff_command(&self, rurl: &Vec<String>) -> Result<Command> {
        let mut ret = Command::new("ffmpeg");
        ret.args(["-y", "-xerror"]);
        ret.arg("-hide_banner");
        ret.arg("-nostats");
        // ret.arg("-report");
        // ret.arg("-loglevel").arg("quiet");
        // ret.args(["-probesize", "102400"]);
        match self.cm.stream_type.get() {
            crate::config::StreamType::DASH => {
                if self.cm.site == Site::BiliVideo {
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
                ret.args(["-map", "0:v:0?", "-map", "1:a:0?", "-map", "2:s:0"]);
            }
            crate::config::StreamType::HLS(0) => {
                ret.arg("-i").arg("-");
                ret.arg("-i").arg(self.ipc_manager.get_danmaku_socket_path());
                ret.args(["-map", "0:v:0?", "-map", "0:a:0?", "-map", "1:s:0"]);
            }
            _ => {
                ret.arg("-i").arg(self.ipc_manager.get_video_socket_path());
                ret.arg("-i").arg(self.ipc_manager.get_danmaku_socket_path());
                ret.args(["-map", "0:v:0?", "-map", "0:a:0?", "-map", "1:s:0"]);
            }
        }
        ret.args(&["-c", "copy"]);
        ret.args(&[
            "-metadata",
            format!("title={}", self.cm.title.borrow()).as_str(),
            "-f",
            "matroska",
        ]);
        match self.cm.run_mode {
            RunMode::Play => {
                ret.arg("-listen").arg("1").arg(self.ipc_manager.get_f2m_socket_path());
            }
            RunMode::Record => {
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
        info!("close ffmpeg");
        let _ = self
            .ff_stdin
            .borrow_mut()
            .take()
            .ok_or(anyhow!("ffmpeg stdin not found"))?
            .write_all("q\n".as_bytes())
            .await?;
        Ok(())
    }

    pub async fn get_video_info<T: AsyncRead + Unpin>(&self, ffstderr: T) -> Result<()> {
        let mut reader = BufReader::new(ffstderr).lines();
        let res_re = regex::Regex::new(r"Stream #[0-9].+? Video:.*?\D(\d{3,5})x(\d{2,5})\D.*").unwrap();
        let pts_re = regex::Regex::new(r"Duration: ([^,\s]+),\s+(start: ([0-9.]+))*.+").unwrap();
        let dm_re = regex::Regex::new(r"Stream #[0-9:]+\s*Subtitle:\s*ass").unwrap();
        let mut vinfo_sent = false;
        let mut ffready_sent = false;
        while let Some(line) = reader.next_line().await.unwrap_or(None) {
            info!("{}", &line);
            let line = line.trim();
            if let Some(_it) = pts_re.captures(&line) {
                // duration = utils::str_to_ms(&it[1]);
                // let st: f64 = it.get(3).map_or("0", |it| it.as_str()).parse().unwrap_or(0.0);
                // start = (st * 1000.0) as u64;
                // continue;
            } else if let Some(_it) = dm_re.captures(&line) {
                if ffready_sent == false {
                    let _ = self.mtx.send(DMLMessage::FfmpegOutputReady).await;
                    ffready_sent = true;
                }
            } else if let Some(it) = res_re.captures(&line) {
                let w = it[1].parse().unwrap();
                let h = it[2].parse().unwrap();
                if w < 100 || h < 100 {
                    let _ = self.quit().await;
                }
                if vinfo_sent == false {
                    let _ = self.mtx.send(DMLMessage::SetVideoInfo((w, h, 0))).await;
                    vinfo_sent = true;
                }
            }
        }
        // warn!("get video info failed!");
        let _ = self.quit().await;
        Ok(())
    }

    pub async fn run(&self, rurl: &Vec<String>) -> Result<()> {
        let mut ff = self
            .create_ff_command(rurl)?
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let ffstderr = ff.stderr.take().unwrap();
        let ff_task = async {
            if self.cm.stream_type.get() == StreamType::HLS(0) {
                let mut preff = self
                    .create_pre_ff_command()?
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(false)
                    .spawn()
                    .unwrap();
                let ffstdin = preff.stdin.take().unwrap();
                *self.ff_stdin.borrow_mut() = Some(ffstdin);
                let mut ffin = ff.stdin.take().unwrap();
                let mut preffout = preff.stdout.take().unwrap();
                tokio::io::copy(&mut preffout, &mut ffin).await?;
                ff.kill().await?;
                ff.wait().await?;
            } else {
                let ffstdin = ff.stdin.take().unwrap();
                *self.ff_stdin.borrow_mut() = Some(ffstdin);
                ff.wait().await?;
            };
            anyhow::Ok(())
        };

        let _ = tokio::join!(ff_task, self.get_video_info(ffstderr));
        Ok(())
    }
}
