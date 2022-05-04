use std::sync::Arc;

use anyhow::anyhow;
use anyhow::Result;
use async_channel::Receiver;
use futures::{
    future::{
        select_ok,
        AbortHandle,
        Abortable,
    },
    Future,
};
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    net::{
        TcpListener,
        UnixListener,
    },
};
use uuid::Uuid;

use crate::config::ConfigManager;

pub trait DMLStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}
impl<T> DMLStream for T where T: AsyncRead + AsyncWrite + Send + Sync + Unpin
{
}

pub struct IPCManager {
    pub is_dash: bool,
    plat: u8,
    base_uuid: String,
    base_socket_dir: String,
    f2m_port: u16,
    stream_port: u16,
    danmaku_port: u16,
    video_port: u16,
    audio_port: u16,
    abort_handle: Option<AbortHandle>,
    danmaku_socket_rx: Option<Receiver<Box<dyn DMLStream>>>,
    // danmaku_unix_rx: Option<Receiver<UnixStream>>,
    stream_socket_rx: Option<Receiver<Box<dyn DMLStream>>>,
    dashv_socket_rx: Option<Receiver<Box<dyn DMLStream>>>,
    dasha_socket_rx: Option<Receiver<Box<dyn DMLStream>>>,
}

impl IPCManager {
    pub fn new(cm: Arc<ConfigManager>) -> Self {
        let is_dash = if cm.room_url.contains("youtube.com") {
            true
        } else {
            false
        };
        let base_uuid = Uuid::new_v4().to_hyphenated().to_string();
        IPCManager {
            is_dash,
            plat: cm.plat,
            base_uuid,
            base_socket_dir: "/tmp".into(),
            f2m_port: 0,
            stream_port: 0,
            danmaku_port: 0,
            video_port: 0,
            audio_port: 0,
            abort_handle: None,
            danmaku_socket_rx: None,
            stream_socket_rx: None,
            dashv_socket_rx: None,
            dasha_socket_rx: None,
        }
    }

    pub async fn stop(&self) -> Result<()> {
        self.abort_handle.as_ref().unwrap().abort();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if self.plat == 0 {
            tokio::fs::remove_file(format!(
                "{}/dml-{}-dm",
                &self.base_socket_dir, &self.base_uuid
            ))
            .await?;
            if !self.is_dash {
                tokio::fs::remove_file(format!(
                    "{}/dml-{}-s",
                    &self.base_socket_dir, &self.base_uuid
                ))
                .await?;
            }
            tokio::fs::remove_file(format!(
                "{}/dml-{}-mpv",
                &self.base_socket_dir, &self.base_uuid
            ))
            .await?;
        }
        Ok(())
    }
    pub async fn run_normal(&mut self) -> Result<()> {
        let mut tasks: Vec<std::pin::Pin<Box<dyn Future<Output = Result<()>>>>> = Vec::new();
        let dml = UnixListener::bind(format!(
            "{}/dml-{}-dm",
            &self.base_socket_dir, &self.base_uuid
        ))?;
        let (tx, rx) = async_channel::bounded(1);
        self.danmaku_socket_rx = Some(rx);
        let danmaku_socket_task = async move {
            while let std::result::Result::Ok((s, _)) = dml.accept().await {
                tx.send(Box::new(s)).await?;
            }
            // anyhow::Ok::<()>(())
            Ok::<(), _>(())
        };
        tasks.push(Box::pin(danmaku_socket_task));
        if self.is_dash {
            let (vl, p) = Self::get_tcp_listener().await;
            self.video_port = p;
            let (tx, rx) = async_channel::bounded(1);
            self.dashv_socket_rx = Some(rx);
            let dashv_socket_task = async move {
                while let std::result::Result::Ok((s, _)) = vl.accept().await {
                    tx.send(Box::new(s)).await?;
                }
                anyhow::Ok::<()>(())
            };
            tasks.push(Box::pin(dashv_socket_task));

            let (al, p) = Self::get_tcp_listener().await;
            self.audio_port = p;
            let (tx, rx) = async_channel::bounded(1);
            self.dasha_socket_rx = Some(rx);
            let dasha_socket_task = async move {
                while let Ok((s, _)) = al.accept().await {
                    tx.send(Box::new(s)).await?;
                }
                anyhow::Ok::<()>(())
            };
            tasks.push(Box::pin(dasha_socket_task));
        } else {
            let sl = UnixListener::bind(format!(
                "{}/dml-{}-s",
                &self.base_socket_dir, &self.base_uuid
            ))?;
            let (tx, rx) = async_channel::bounded(1);
            self.stream_socket_rx = Some(rx);
            let stream_socket_task = async move {
                while let Ok((s, _)) = sl.accept().await {
                    tx.send(Box::new(s)).await?;
                }
                anyhow::Ok::<()>(())
            };
            tasks.push(Box::pin(stream_socket_task));
        }
        let (_, p) = Self::get_tcp_listener().await;
        self.f2m_port = p;

        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let tasks = Abortable::new(select_ok(tasks.into_iter()), abort_registration);
        self.abort_handle = Some(abort_handle);
        tokio::task::spawn_local(async move {
            let _ = tasks.await;
        });
        Ok(())
    }
    pub async fn run_tcp_only(&mut self) -> Result<()> {
        let mut tasks: Vec<std::pin::Pin<Box<dyn Future<Output = Result<()>>>>> = Vec::new();
        let (dml, p) = Self::get_tcp_listener().await;
        self.danmaku_port = p;
        let (tx, rx) = async_channel::bounded(1);
        self.danmaku_socket_rx = Some(rx);
        let danmaku_socket_task = async move {
            while let Ok((s, _)) = dml.accept().await {
                tx.send(Box::new(s)).await?;
            }
            anyhow::Ok::<()>(())
        };
        tasks.push(Box::pin(danmaku_socket_task));
        if self.is_dash {
            let (vl, p) = Self::get_tcp_listener().await;
            self.video_port = p;
            let (tx, rx) = async_channel::bounded(1);
            self.dashv_socket_rx = Some(rx);
            let dashv_socket_task = async move {
                while let Ok((s, _)) = vl.accept().await {
                    tx.send(Box::new(s)).await?;
                }
                anyhow::Ok::<()>(())
            };
            tasks.push(Box::pin(dashv_socket_task));

            let (al, p) = Self::get_tcp_listener().await;
            self.audio_port = p;
            let (tx, rx) = async_channel::bounded(1);
            self.dasha_socket_rx = Some(rx);
            let dasha_socket_task = async move {
                while let Ok((s, _)) = al.accept().await {
                    tx.send(Box::new(s)).await?;
                }
                anyhow::Ok::<()>(())
            };
            tasks.push(Box::pin(dasha_socket_task));
        } else {
            let (sl, p) = Self::get_tcp_listener().await;
            self.stream_port = p;
            let (tx, rx) = async_channel::bounded(1);
            self.stream_socket_rx = Some(rx);
            let stream_socket_task = async move {
                while let Ok((s, _)) = sl.accept().await {
                    tx.send(Box::new(s)).await?;
                }
                anyhow::Ok::<()>(())
            };
            tasks.push(Box::pin(stream_socket_task));
        }
        let (_, p) = Self::get_tcp_listener().await;
        self.f2m_port = p;

        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let tasks = Abortable::new(select_ok(tasks.into_iter()), abort_registration);
        self.abort_handle = Some(abort_handle);
        tokio::task::spawn_local(async move {
            let _ = tasks.await;
        });
        Ok(())
    }
    pub async fn run(&mut self) -> Result<()> {
        if self.plat == 0 {
            self.run_normal().await?;
        } else {
            self.run_tcp_only().await?;
        }
        Ok(())
    }

    async fn get_tcp_listener() -> (TcpListener, u16) {
        for _ in 0..100 {
            let rn = rand::random::<u16>();
            let rn = (rn % 10000) + 20000;
            match TcpListener::bind(format!("127.0.0.1:{}", &rn)).await {
                Ok(it) => {
                    return (it, rn);
                }
                Err(_) => {
                    continue;
                }
            };
        }
        panic!("cannot bind tcp");
    }
    pub fn get_mpv_socket_path(&self) -> String {
        format!("{}/dml-{}-mpv", &self.base_socket_dir, &self.base_uuid)
    }

    pub fn get_f2m_socket_path(&self) -> String {
        // if self.plat == 0 {
        //     format!(
        //         "unix://{}/dml-{}-f2m",
        //         &self.base_socket_dir, &self.base_uuid
        //     )
        // } else {
        format!("tcp://127.0.0.1:{}", &self.f2m_port)
        // }
    }

    pub fn get_stream_socket_path(&self) -> String {
        if self.plat == 0 {
            format!("unix://{}/dml-{}-s", &self.base_socket_dir, &self.base_uuid)
        } else {
            format!("tcp://127.0.0.1:{}", &self.stream_port)
        }
    }

    pub fn get_video_socket_path(&self) -> String {
        format!("tcp://127.0.0.1:{}", &self.video_port)
    }

    pub fn get_audio_socket_path(&self) -> String {
        format!("tcp://127.0.0.1:{}", &self.audio_port)
    }

    pub fn get_danmaku_socket_path(&self) -> String {
        if self.plat == 0 {
            format!(
                "unix://{}/dml-{}-dm",
                &self.base_socket_dir, &self.base_uuid
            )
        } else {
            format!("tcp://127.0.0.1:{}", &self.danmaku_port)
        }
    }

    pub async fn get_danmaku_socket(&self) -> Result<Box<dyn DMLStream>> {
        let rx = self.danmaku_socket_rx.as_ref().ok_or(anyhow!("gds err 1"))?;
        let s = loop {
            let s = rx.recv().await?;
            if rx.is_empty() {
                break s;
            }
        };
        Ok(s)
    }

    pub async fn get_stream_socket(&self) -> Result<Box<dyn DMLStream>> {
        let s = self.stream_socket_rx.as_ref().ok_or(anyhow!("gss err 1"))?.recv().await?;
        Ok(s)
    }

    pub async fn get_video_socket(&self) -> Result<Box<dyn DMLStream>> {
        let s = self.dashv_socket_rx.as_ref().ok_or(anyhow!("gvs err 1"))?.recv().await?;
        Ok(s)
    }

    pub async fn get_audio_socket(&self) -> Result<Box<dyn DMLStream>> {
        let s = self.dasha_socket_rx.as_ref().ok_or(anyhow!("gas err 1"))?.recv().await?;
        Ok(s)
    }
}
