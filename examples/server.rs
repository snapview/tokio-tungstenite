//! A chat server that broadcasts a message to all connections.
//!
//! This is a simple line-based server which accepts WebSocket connections,
//! reads lines from those connections, and broadcasts the lines to all other
//! connected clients.
//!
//! You can test this out by running:
//!
//!     cargo run --example server 127.0.0.1:12345
//!
//! And then in another window run:
//!
//!     cargo run --example client ws://127.0.0.1:12345/
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.

use std::env;
use std::io::Error;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::{SinkExt, StreamExt};
use log::*;
use std::net::{SocketAddr, ToSocketAddrs};
use tokio_net::tcp::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

struct Connection {
    addr: SocketAddr,
    rx: UnboundedReceiver<Message>,
    tx: UnboundedSender<Message>,
}

async fn handle_connection(connection: Connection) {
    let mut connection = connection;
    while let Some(msg) = connection.rx.next().await {
        info!("Received a message from {}: {}", connection.addr, msg);
        connection
            .tx
            .unbounded_send(msg)
            .expect("Failed to forward message");
    }
}

async fn accept_connection(stream: TcpStream) {
    let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {}", addr);

    let mut ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {}", addr);

    // Create a channel for our stream, which other sockets will use to
    // send us messages. Then register our address with the stream to send
    // data to us.
    let (msg_tx, msg_rx) = futures::channel::mpsc::unbounded();
    let (response_tx, mut response_rx) = futures::channel::mpsc::unbounded();
    let c = Connection {
        addr: addr,
        rx: msg_rx,
        tx: response_tx,
    };
    tokio::spawn(handle_connection(c));

    while let Some(message) = ws_stream.next().await {
        let message = message.expect("Failed to get request");
        msg_tx
            .unbounded_send(message)
            .expect("Failed to forward request");
        if let Some(resp) = response_rx.next().await {
            ws_stream.send(resp).await.expect("Failed to send response");
        }
    }
}
#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = env_logger::try_init();
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr
        .to_socket_addrs()
        .expect("Not a valid address")
        .next()
        .expect("Not a socket address");

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let mut listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(stream));
    }

    Ok(())
}
