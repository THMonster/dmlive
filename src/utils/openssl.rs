use hyper::rt::{Read, ReadBufCursor, Write};
use std::{
    pin::Pin,
    task::ready,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub struct TlsStream {
    _proc: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl TlsStream {
    pub fn connect(sni: &str, port: &str) -> Self {
        let mut proc = Command::new("openssl")
            .args(["s_client", "-connect"])
            .arg(format!("{sni}:{port}"))
            .arg("-quiet")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stdin = proc.stdin.take().unwrap();
        let stdout = proc.stdout.take().unwrap();
        Self {
            _proc: proc,
            stdin,
            stdout,
        }
    }
}

impl Read for TlsStream {
    fn poll_read(
        mut self: Pin<&mut Self>, cx: &mut Context<'_>, mut buf: ReadBufCursor<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let inner = unsafe { buf.as_mut() };
        let mut tokio_buf = ReadBuf::uninit(inner);
        ready!(Pin::new(&mut self.stdout).poll_read(cx, &mut tokio_buf))?;
        let n = tokio_buf.filled().len();
        unsafe { buf.advance(n) };
        Poll::Ready(Ok(()))
    }
}

impl Write for TlsStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

impl AsyncRead for TlsStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for TlsStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}
