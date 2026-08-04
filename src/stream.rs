use std::{
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_socks::tcp::Socks5Stream;

pin_project! {
    #[project = ProxyStreamProject]
    pub enum ProxyStream {
        Tcp { #[pin] inner: TcpStream },
        Socks5 { #[pin] inner: Socks5Stream<TcpStream> },
    }
}

impl AsyncRead for ProxyStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.project() {
            ProxyStreamProject::Tcp { inner } => inner.poll_read(cx, buf),
            ProxyStreamProject::Socks5 { inner } => inner.poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ProxyStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.project() {
            ProxyStreamProject::Tcp { inner } => inner.poll_write(cx, buf),
            ProxyStreamProject::Socks5 { inner } => inner.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.project() {
            ProxyStreamProject::Tcp { inner } => inner.poll_flush(cx),
            ProxyStreamProject::Socks5 { inner } => inner.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.project() {
            ProxyStreamProject::Tcp { inner } => inner.poll_shutdown(cx),
            ProxyStreamProject::Socks5 { inner } => inner.poll_shutdown(cx),
        }
    }
}
