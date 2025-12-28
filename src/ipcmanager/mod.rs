use crate::config::{ConfigManager, Platform};
use crate::dmlerr;
use anyhow::Result;
use std::rc::Rc;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, UnixListener},
};
use uuid::Uuid;

pub trait DMLStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}
impl<T> DMLStream for T where T: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

pub struct IPCManager {
    base_uuid: String,
    base_socket_dir: String,
    f2m_port: u16,
    danmaku_port: u16,
    video_port: u16,
    audio_port: u16,
    danmaku_unix_listener: Option<UnixListener>,
    danmaku_tcp_listener: Option<TcpListener>,
    video_tcp_listener: Option<TcpListener>,
    audio_tcp_listener: Option<TcpListener>,
    // cm: Rc<ConfigManager>,
    cm: Rc<ConfigManager>,
}

impl IPCManager {
    pub fn new(cm: Rc<ConfigManager>) -> Self {
        let base_uuid = Uuid::new_v4().as_hyphenated().to_string();
        IPCManager {
            base_uuid,
            base_socket_dir: "/tmp".into(),
            f2m_port: 0,
            danmaku_port: 0,
            video_port: 0,
            audio_port: 0,
            danmaku_unix_listener: None,
            danmaku_tcp_listener: None,
            video_tcp_listener: None,
            audio_tcp_listener: None,
            cm,
        }
    }

    pub async fn stop(&self) -> Result<()> {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if self.cm.plat == Platform::Linux {
            let _ = tokio::fs::remove_file(format!(
                "{}/dml-{}-dm",
                &self.base_socket_dir, &self.base_uuid
            ))
            .await;
            let _ = tokio::fs::remove_file(format!(
                "{}/dml-{}-mpv",
                &self.base_socket_dir, &self.base_uuid
            ))
            .await;
        }
        Ok(())
    }

    async fn init_danmaku(&mut self) -> Result<()> {
        // if self.cm.plat == Platform::Android {
        //     let dml = UnixListener::bind(format!(
        //         "{}/dml-{}-dm",
        //         &self.base_socket_dir, &self.base_uuid
        //     ))?;
        //     self.danmaku_unix_listener = Some(dml);
        // } else {
        let (dml, p) = Self::get_tcp_listener().await;
        self.danmaku_port = p;
        self.danmaku_tcp_listener = Some(dml);
        // }
        Ok(())
    }

    async fn init_stream(&mut self) -> Result<()> {
        let (vl, p) = Self::get_tcp_listener().await;
        self.video_port = p;
        self.video_tcp_listener = Some(vl);
        let (al, p) = Self::get_tcp_listener().await;
        self.audio_port = p;
        self.audio_tcp_listener = Some(al);
        Ok(())
    }

    async fn init_f2m(&mut self) -> Result<()> {
        let (_, p) = Self::get_tcp_listener().await;
        self.f2m_port = p;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        self.init_danmaku().await?;
        self.init_stream().await?;
        self.init_f2m().await?;
        Ok(())
    }

    async fn get_tcp_listener() -> (TcpListener, u16) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p = l.local_addr().unwrap().port();
        (l, p)
    }

    pub fn get_mpv_socket_path(&self) -> String {
        format!("{}/dml-{}-mpv", &self.base_socket_dir, &self.base_uuid)
    }

    pub fn get_f2m_socket_path(&self) -> String {
        format!("tcp://127.0.0.1:{}", &self.f2m_port)
    }

    pub fn get_video_socket_path(&self) -> String {
        format!("tcp://127.0.0.1:{}", &self.video_port)
    }

    pub fn get_audio_socket_path(&self) -> String {
        format!("tcp://127.0.0.1:{}", &self.audio_port)
    }

    pub fn get_danmaku_socket_path(&self) -> String {
        // if self.cm.plat == Platform::Linux {
        //     format!(
        //         "unix://{}/dml-{}-dm",
        //         &self.base_socket_dir, &self.base_uuid
        //     )
        // } else {
        format!("tcp://127.0.0.1:{}", &self.danmaku_port)
        // }
    }

    pub async fn get_danmaku_socket(&self) -> Result<Box<dyn DMLStream>> {
        // if self.cm.plat == Platform::Linux {
        //     let (s, _) = self.danmaku_unix_listener.as_ref().ok_or_else(|| dmlerr!())?.accept().await?;
        //     Ok(Box::new(s))
        // } else {
        let (s, _) = self.danmaku_tcp_listener.as_ref().ok_or_else(|| dmlerr!())?.accept().await?;
        Ok(Box::new(s))
        // }
    }

    pub async fn get_video_socket(&self) -> Result<Box<dyn DMLStream>> {
        let (s, _) = self.video_tcp_listener.as_ref().ok_or_else(|| dmlerr!())?.accept().await?;
        Ok(Box::new(s))
    }

    pub async fn get_audio_socket(&self) -> Result<Box<dyn DMLStream>> {
        let (s, _) = self.audio_tcp_listener.as_ref().ok_or_else(|| dmlerr!())?.accept().await?;
        Ok(Box::new(s))
    }
}
