use anyhow::{Result, bail};
use std::cell::{Cell, RefCell};
use std::io::Read;
use std::os::fd::AsRawFd;
use tokio::net::{TcpStream, UnixStream};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
};

pub trait DMLStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}
impl<T> DMLStream for T where T: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

async fn get_tcp_listener() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    (l, p)
}

pub fn get_unixsocket_pair() -> Result<(UnixStream, std::os::unix::net::UnixStream)> {
    let (a, b) = UnixStream::pair()?;
    let b = b.into_std()?;
    // disable CLOEXEC
    unsafe { libc::fcntl(b.as_raw_fd(), libc::F_SETFD, 0) };
    Ok((a, b))
}

pub fn get_unixsocket_pair_two_fd() -> Result<(
    std::os::unix::net::UnixStream,
    std::os::unix::net::UnixStream,
)> {
    let (a, b) = UnixStream::pair()?;
    let a = a.into_std()?;
    let b = b.into_std()?;
    // disable CLOEXEC
    unsafe {
        libc::fcntl(a.as_raw_fd(), libc::F_SETFD, 0);
        libc::fcntl(b.as_raw_fd(), libc::F_SETFD, 0);
    }
    Ok((a, b))
}

fn clean_socket_buffer(so: &mut std::os::unix::net::UnixStream) {
    let mut buf = [0u8; 4096];
    so.set_nonblocking(true).unwrap();
    loop {
        match so.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_e) => return,
        }
    }
    // so.set_nonblocking(false).unwrap();
}

pub struct IPCManager {
    f2m_ff_unix_fd: RefCell<std::os::unix::net::UnixStream>,
    f2m_mpv_unix_fd: RefCell<std::os::unix::net::UnixStream>,
    danmaku_unix: RefCell<Option<UnixStream>>,
    danmaku_unix_fd: RefCell<Option<std::os::unix::net::UnixStream>>,
    danmaku_tcp_listener: RefCell<Option<TcpListener>>,
    danmaku_tcp_port: Cell<u16>,
    video_unix: RefCell<Option<UnixStream>>,
    video_unix_fd: RefCell<Option<std::os::unix::net::UnixStream>>,
    audio_unix: RefCell<Option<UnixStream>>,
    audio_unix_fd: RefCell<Option<std::os::unix::net::UnixStream>>,
}

impl Default for IPCManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IPCManager {
    pub fn new() -> Self {
        // let base_uuid = Uuid::new_v4().as_hyphenated().to_string();
        let (f2ma, f2mb) = get_unixsocket_pair_two_fd().unwrap();
        IPCManager {
            danmaku_unix: None.into(),
            danmaku_unix_fd: None.into(),
            video_unix: None.into(),
            video_unix_fd: None.into(),
            audio_unix: None.into(),
            audio_unix_fd: None.into(),
            f2m_ff_unix_fd: f2ma.into(),
            f2m_mpv_unix_fd: f2mb.into(),
            danmaku_tcp_listener: None.into(),
            danmaku_tcp_port: 0.into(),
        }
    }

    pub async fn generate(&self) -> Result<()> {
        clean_socket_buffer(&mut self.f2m_mpv_unix_fd.borrow_mut());
        let (a, b) = get_unixsocket_pair()?;
        self.video_unix.replace(Some(a));
        self.video_unix_fd.replace(Some(b));
        let (a, b) = get_unixsocket_pair()?;
        self.audio_unix.replace(Some(a));
        self.audio_unix_fd.replace(Some(b));
        let (a, b) = get_unixsocket_pair()?;
        self.danmaku_unix.replace(Some(a));
        self.danmaku_unix_fd.replace(Some(b));
        let (l, p) = get_tcp_listener().await;
        self.danmaku_tcp_listener.replace(Some(l));
        self.danmaku_tcp_port.set(p);
        Ok(())
    }

    pub fn replace_f2m_socket(&self) -> std::os::unix::net::UnixStream {
        let (f2ma, f2mb) = get_unixsocket_pair_two_fd().unwrap();
        let _ = self.f2m_ff_unix_fd.replace(f2ma).shutdown(std::net::Shutdown::Both);
        self.f2m_mpv_unix_fd.replace(f2mb)
    }

    pub fn get_f2m_socket_ff(&self) -> i32 {
        self.f2m_ff_unix_fd.borrow().as_raw_fd()
    }

    pub fn get_f2m_socket_mpv(&self) -> i32 {
        self.f2m_mpv_unix_fd.borrow().as_raw_fd()
    }

    pub fn get_video_socket(&self) -> Option<UnixStream> {
        self.video_unix.take()
    }

    pub fn get_video_socket_fd(&self) -> Option<std::os::unix::net::UnixStream> {
        self.video_unix_fd.take()
    }

    pub fn get_audio_socket(&self) -> Option<UnixStream> {
        self.audio_unix.take()
    }

    pub fn get_audio_socket_fd(&self) -> Option<std::os::unix::net::UnixStream> {
        self.audio_unix_fd.take()
    }

    pub fn get_danmaku_socket(&self) -> Option<UnixStream> {
        self.danmaku_unix.take()
    }

    pub fn get_danmaku_socket_fd(&self) -> Option<std::os::unix::net::UnixStream> {
        self.danmaku_unix_fd.take()
    }

    pub async fn get_danmaku_tcp_socket(&self) -> Result<TcpStream> {
        let Some(l) = self.danmaku_tcp_listener.take() else {
            bail!("danmaku tcp listener not found.")
        };
        let (s, _) = l.accept().await?;
        Ok(s)
    }

    pub fn get_danmaku_tcp_port(&self) -> u16 {
        self.danmaku_tcp_port.get()
    }
}
