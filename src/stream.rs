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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn the_tcp_variant_reads_writes_flushes_and_shuts_down() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = [0_u8; 4];
            socket.read_exact(&mut received).await.unwrap();
            socket.write_all(b"pong").await.unwrap();
        });

        let mut stream = ProxyStream::Tcp {
            inner: TcpStream::connect(addr).await.unwrap(),
        };

        stream.write_all(b"ping").await.unwrap();
        stream.flush().await.unwrap();

        let mut answer = [0_u8; 4];
        stream.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"pong");

        stream.shutdown().await.unwrap();
    }
}
