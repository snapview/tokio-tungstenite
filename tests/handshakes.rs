extern crate futures;
extern crate tokio_core;
extern crate tokio_tungstenite;
extern crate url;

use std::io;

use futures::{Future, Stream};
use tokio_core::net::{TcpStream, TcpListener};
use tokio_core::reactor::Core;
use tokio_tungstenite::{client_async, accept_async};

#[test]
fn handshakes() {
    use std::sync::mpsc::channel;
    use std::thread;

    let (tx, rx) = channel();

    thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        let address = "0.0.0.0:12345".parse().unwrap();
        let listener = TcpListener::bind(&address, &handle).unwrap();
        let connections = listener.incoming();
        tx.send(()).unwrap();
        let handshakes = connections.and_then(|(connection, _)| {
            accept_async(connection)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        });
        let server = handshakes.for_each(|_| {
            Ok(())
        });

        core.run(server).unwrap();
    });

    rx.recv().unwrap();
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let address = "0.0.0.0:12345".parse().unwrap();
    let tcp = TcpStream::connect(&address, &handle);
    let handshake = tcp.and_then(|stream| {
        let url = url::Url::parse("ws://localhost:12345/").unwrap();
        client_async(url, stream)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    });
    let client = handshake.and_then(|_| {
        Ok(())
    });
    core.run(client).unwrap();
}

