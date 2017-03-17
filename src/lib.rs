//! Async WebSocket usage.
//!
//! This library is an implementation of WebSocket handshakes and streams. It
//! is based on the crate which implements all required WebSocket protocol
//! logic. So this crate basically just brings tokio support / tokio integration
//! to it.
//!
//! Each WebSocket stream implements the required `Stream` and `Sink` traits,
//! so the socket is just a stream of messages coming in and going out.
//!
//! This crate primarily exports this ability through two extension traits,
//! `ClientHandshakeExt` and `ServerHandshakeExt`. These traits augment the
//! functionality provided by the `tungestenite` crate, on which this crate is
//! built. Configuration is done through `tungestenite` crate as well.

#![deny(
    missing_docs,
    unused_must_use,
    unused_mut,
    unused_imports,
    unused_import_braces)]

extern crate futures;
extern crate tokio_core;
extern crate tungstenite;
extern crate url;

use std::io::ErrorKind;

use futures::{Poll, Future, Async, AsyncSink, Stream, Sink, StartSend};
use tokio_core::io::Io;

use url::Url;

use tungstenite::handshake::client::ClientHandshake;
use tungstenite::handshake::server::ServerHandshake;
use tungstenite::handshake::{HandshakeRole, HandshakeError};
use tungstenite::protocol::{WebSocket, Message};
use tungstenite::error::Error as WsError;
use tungstenite::client;
use tungstenite::server;

/// Create a handshake provided stream and assuming the provided request.
///
/// This function will internally call `client::client` to create a
/// handshake representation and returns a future representing the
/// resolution of the WebSocket handshake. The returned future will resolve
/// to either `WebSocketStream<S>` or `Error` depending if it's successful
/// or not.
///
/// This is typically used for clients who have already established, for
/// example, a TCP connection to the remove server.
pub fn client_async<S: Io>(url: Url, stream: S) -> ConnectAsync<S> {
    ConnectAsync {
        inner: MidHandshake {
            inner: Some(client::client(url, stream))
        }
    }
}

/// Accepts a new WebSocket connection with the provided stream.
///
/// This function will internally call `server::accept` to create a
/// handshake representation and returns a future representing the
/// resolution of the WebSocket handshake. The returned future will resolve
/// to either `WebSocketStream<S>` or `Error` depending if it's successful
/// or not.
///
/// This is typically used after a socket has been accepted from a
/// `TcpListener`. That socket is then passed to this function to perform
/// the server half of the accepting a client's websocket connection.
pub fn accept_async<S: Io>(stream: S) -> AcceptAsync<S> {
    AcceptAsync {
        inner: MidHandshake {
            inner: Some(server::accept(stream))
        }
    }
}

/// A wrapper around an underlying raw stream which implements the WebSocket
/// protocol.
///
/// A `WebSocketStream<S>` represents a handshake that has been completed
/// successfully and both the server and the client are ready for receiving
/// and sending data. Message from a `WebSocketStream<S>` are accessible
/// through the respective `Stream` and `Sink`. Check more information about
/// them in `futures-rs` crate documentation or have a look on the examples
/// and unit tests for this crate.
pub struct WebSocketStream<S> {
    inner: WebSocket<S>,
}

impl<T> Stream for WebSocketStream<T> where T: Io {
    type Item = Message;
    type Error = WsError;

    fn poll(&mut self) -> Poll<Option<Message>, WsError> {
        self.inner.read_message().map(|m| Some(m)).to_async()
    }
}

impl<T> Sink for WebSocketStream<T> where T: Io {
    type SinkItem = Message;
    type SinkError = WsError;

    fn start_send(&mut self, item: Message) -> StartSend<Message, WsError> {
        try!(self.inner.write_message(item).to_async());
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), WsError> {
        self.inner.write_pending().to_async()
    }
}

/// Future returned from connect_async() which will resolve
/// once the connection handshake has finished.
pub struct ConnectAsync<S> {
    inner: MidHandshake<S, ClientHandshake>,
}

impl<S: Io> Future for ConnectAsync<S> {
    type Item = WebSocketStream<S>;
    type Error = WsError;

    fn poll(&mut self) -> Poll<WebSocketStream<S>, WsError> {
        self.inner.poll()
    }
}

/// Future returned from accept_async() which will resolve
/// once the connection handshake has finished.
pub struct AcceptAsync<S> {
    inner: MidHandshake<S, ServerHandshake>,
}

impl<S: Io> Future for AcceptAsync<S> {
    type Item = WebSocketStream<S>;
    type Error = WsError;

    fn poll(&mut self) -> Poll<WebSocketStream<S>, WsError> {
        self.inner.poll()
    }
}

struct MidHandshake<S, R> {
    inner: Option<Result<WebSocket<S>, HandshakeError<S, R>>>,
}

impl<S: Io, R: HandshakeRole> Future for MidHandshake<S, R> {
    type Item = WebSocketStream<S>;
    type Error = WsError;

    fn poll(&mut self) -> Poll<WebSocketStream<S>, WsError> {
        match self.inner.take().expect("cannot poll MidHandshake twice") {
            Ok(stream) => Ok(WebSocketStream { inner: stream }.into()),
            Err(HandshakeError::Failure(e)) => Err(e),
            Err(HandshakeError::Interrupted(s)) => {
                match s.handshake() {
                    Ok(stream) => Ok(WebSocketStream { inner: stream }.into()),
                    Err(HandshakeError::Failure(e)) => Err(e),
                    Err(HandshakeError::Interrupted(s)) => {
                        self.inner = Some(Err(HandshakeError::Interrupted(s)));
                        Ok(Async::NotReady)
                    }
                }
            }
        }
    }
}

trait ToAsync {
    type T;
    type E;
    fn to_async(self) -> Result<Async<Self::T>, Self::E>;
}

impl<T> ToAsync for Result<T, WsError> {
    type T = T;
    type E = WsError;
    fn to_async(self) -> Result<Async<Self::T>, Self::E> {
        match self {
            Ok(x) => Ok(Async::Ready(x)),
            Err(error) => match error {
                WsError::Io(ref err) if err.kind() == ErrorKind::WouldBlock => Ok(Async::NotReady),
                err => Err(err),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url;

    use std::io;

    use futures::{Future, Stream};
    use tokio_core::net::{TcpStream, TcpListener};
    use tokio_core::reactor::Core;

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
}
