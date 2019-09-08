use std::io;

use futures::{Future, Stream, Sink, Async};
use tokio_tcp::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, client_async};
use tungstenite::{Message};

#[test]
fn send_after_close() {
    use std::sync::mpsc::channel;
    use std::thread;

    let (tx, rx) = channel();

    thread::spawn(move || {
        let address = "0.0.0.0:12346".parse().unwrap();
        let listener = TcpListener::bind(&address).unwrap();
        let connections = listener.incoming();
        tx.send(()).unwrap();
        let handshakes = connections.and_then(|connection| {
            accept_async(connection).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        });
        let server = handshakes.for_each(|stream| {
            Stream::wait( stream ).for_each( |res| { dbg!( res ).expect( "for_each" ); });
            Ok(())
        });

        server.wait().unwrap();
    });

    rx.recv().unwrap();
    let address = "0.0.0.0:12346".parse().unwrap();
    let tcp = TcpStream::connect(&address);
    let handshake = tcp.and_then(|stream| {
        let url = url::Url::parse("ws://localhost:12345/").unwrap();
        client_async(url, stream).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    });
    let client = handshake.and_then(|(mut stream, _response)| {
        let res = stream.close().expect( "close connection" );
        assert_eq!( res, Async::Ready(()) );
        let res = stream.start_send( Message::Text("hi".to_owned()) );
        assert!( res.is_err() );
        Ok(())
    });
    client.wait().unwrap();
}
