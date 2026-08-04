//! `ProxyStream`: the async read/write wrapper used for every hop.

use localproxy::stream::ProxyStream;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
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
