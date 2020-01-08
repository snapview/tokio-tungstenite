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

use futures::channel::mpsc::UnboundedSender;
use futures::stream::SplitStream;
use futures::{SinkExt, StreamExt};
use log::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;

type WsRx = SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>;

type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

async fn handle_message(addr: SocketAddr, peer_map: PeerMap, ws_rx: WsRx) {
    let mut ws_rx = ws_rx;
    while let Some(msg) = ws_rx.next().await {
        let msg = msg.expect("Failed to get response");
        info!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );

        for peer in peer_map.lock().unwrap().iter_mut() {
            let _ = peer.1.unbounded_send(msg.clone());
        }
    }
}

async fn accept_connection(peer_map: PeerMap, raw_stream: TcpStream) {
    let addr = raw_stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {}", addr);

    let (mut ws_tx, ws_rx) = ws_stream.split();
    let (msg_tx, mut msg_rx) = futures::channel::mpsc::unbounded();

    //We want to be able to send data to a specific peer, use this tx.
    peer_map.lock().unwrap().insert(addr, msg_tx);

    //Handle incoming messages
    tokio::spawn(handle_message(addr, peer_map.clone(), ws_rx));

    //If msg_rx get's some data, send it on the websocket.
    while let Some(msg) = msg_rx.next().await {
        info!("Sending message to {}", addr);
        ws_tx.send(msg).await.expect("Failed to send request");
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = env_logger::try_init();
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let hm: HashMap<SocketAddr, Tx> = HashMap::new();
    let state = PeerMap::new(Mutex::new(hm));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let mut listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(state.clone(), stream));
    }

    Ok(())
}
