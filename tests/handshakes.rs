extern crate futures;
extern crate tokio_tcp;
extern crate tokio_tungstenite;
extern crate url;

use std::io;

use futures::{Future, Stream};
use tokio_tcp::{TcpStream, TcpListener};
use tokio_tungstenite::{client_async, accept_async};

#[test]
fn handshakes() {
    use std::sync::mpsc::channel;
    use std::thread;

    let (tx, rx) = channel();

    thread::spawn(move || {
        let address = "0.0.0.0:12345".parse().unwrap();
        let listener = TcpListener::bind(&address).unwrap();
        let connections = listener.incoming();
        tx.send(()).unwrap();
        let handshakes = connections.and_then(|connection| {
            accept_async(connection).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        });
        let server = handshakes.for_each(|_| {
            Ok(())
        });

        server.wait().unwrap();
    });

    rx.recv().unwrap();
    let address = "0.0.0.0:12345".parse().unwrap();
    let tcp = TcpStream::connect(&address);
    let handshake = tcp.and_then(|stream| {
        let url = url::Url::parse("ws://localhost:12345/").unwrap();
        client_async(url, stream).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    });
    let client = handshake.and_then(|_| {
        Ok(())
    });
    client.wait().unwrap();
}
