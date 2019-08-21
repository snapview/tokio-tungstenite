use futures::{Future, Stream};
use log::*;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Error as WsError};

fn main() {
    env_logger::init();

    let mut runtime = tokio::runtime::Builder::new().build().unwrap();

    let addr = "127.0.0.1:9002".parse().unwrap();
    let socket = TcpListener::bind(&addr).unwrap();
    info!("Listening on: {}", addr);

    let srv = socket
        .incoming()
        .map_err(Into::into)
        .for_each(move |stream| {
            let peer = stream
                .peer_addr()
                .expect("connected streams should have a peer address");
            info!("Peer address: {}", peer);

            accept_async(stream).and_then(move |ws_stream| {
                info!("New WebSocket connection: {}", peer);
                let (sink, stream) = ws_stream.split();
                let job = stream
                    .filter(|msg| msg.is_text() || msg.is_binary())
                    .forward(sink)
                    .and_then(|(_stream, _sink)| Ok(()))
                    .map_err(|err| match err {
                        WsError::ConnectionClosed => (),
                        err => info!("WS error: {}", err),
                    });

                tokio::spawn(job);
                Ok(())
            })
        });

    runtime.block_on(srv).unwrap();
}
