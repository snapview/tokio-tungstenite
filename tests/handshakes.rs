use std::net::ToSocketAddrs;
use tokio::net::tcp::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, client_async};

#[tokio::test]
async fn handshakes() {
    let (tx, rx) = futures::channel::oneshot::channel();

    let f = async move {
        let address = "0.0.0.0:12345"
            .to_socket_addrs()
            .expect("Not a valid address")
            .next()
            .expect("No address resolved");
        let mut listener = TcpListener::bind(&address).await.unwrap();
        tx.send(()).unwrap();
        while let Ok((connection, _)) = listener.accept().await {
            let stream = accept_async(connection).await;
            stream.expect("Failed to handshake with connection");
        }
    };

    tokio::spawn(f);

    rx.await.expect("Failed to wait for server to be ready");
    let address = "0.0.0.0:12345"
        .to_socket_addrs()
        .expect("Not a valid address")
        .next()
        .expect("No address resolved");
    let tcp = TcpStream::connect(&address)
        .await
        .expect("Failed to connect");
    let url = url::Url::parse("ws://localhost:12345/").unwrap();
    let _stream = client_async(url, tcp)
        .await
        .expect("Client failed to connect");
}
