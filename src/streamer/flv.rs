use log::{info, warn};
use std::{
    cell::Cell,
    os::fd::{FromRawFd, IntoRawFd},
    rc::Rc,
};
use tokio::{io::AsyncWriteExt, process::Command};

use crate::{
    config::Site,
    dmlerr,
    dmlive::{DMLContext, DMLMessage},
};

#[allow(unused)]
pub struct Flv {
    ctx: Rc<DMLContext>,
}

impl Flv {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Flv { ctx }
    }

    async fn download(&self) -> anyhow::Result<()> {
        let mut stream = self.ctx.im.get_video_socket().ok_or_else(|| dmlerr!())?;
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let watch_dog = Cell::new(0);
        let watchdog_task = async {
            loop {
                watch_dog.set(watch_dog.get() + 1);
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                if watch_dog.get() > 10 {
                    warn!("connection too slow");
                    break;
                }
            }
        };
        let dl_task = async {
            let mut resp = client
                .get(self.ctx.cm.stream_info.borrow()["url"].as_str())
                .header("Referer", self.ctx.cm.room_url.as_str());
            if self.ctx.cm.plive && matches!(self.ctx.cm.site, Site::BiliLive) {
                resp = resp.header("Cookie", self.ctx.cm.bcookie.as_str());
            }
            let mut resp = resp.send().await?;
            self.ctx.mtx.send(DMLMessage::StreamReady)?;
            while let Some(chunk) = resp.chunk().await? {
                stream.write_all(&chunk).await?;
                watch_dog.set(0);
            }
            info!("flv downloader exit normally");
            anyhow::Ok(())
        };
        tokio::select! {
            it = dl_task => { it?; }
            _ = watchdog_task => {}
        }
        Ok(())
    }

    async fn download_douyu(&self) -> anyhow::Result<()> {
        let stream = self.ctx.im.get_video_socket().ok_or_else(|| dmlerr!())?;
        let stream = stream.into_std()?;
        stream.set_nonblocking(false).unwrap();
        let fd = stream.into_raw_fd();
        unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
        let mut curl = Command::new("curl")
            .arg(self.ctx.cm.stream_info.borrow()["url"].as_str())
            .arg("-H")
            .arg(format!("Referer: {}", self.ctx.cm.room_url.as_str()))
            .arg("-H")
            .arg(format!("User-Agent: {}", crate::utils::gen_ua()))
            .arg("-s")
            .stdin(std::process::Stdio::null())
            .stdout(unsafe { std::process::Stdio::from_raw_fd(fd) })
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        curl.wait().await?;
        Ok(())
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        match self.ctx.cm.site {
            Site::DouyuLive => self.download_douyu().await?,
            _ => self.download().await?,
        }
        anyhow::bail!("flv streamer exit");
    }
}
