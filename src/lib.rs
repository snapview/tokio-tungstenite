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

#![deny(missing_docs)]

#[macro_use]
extern crate futures;
extern crate tokio_core;
extern crate tungstenite;

use std::io::ErrorKind;

use futures::{Poll, Future, Async, AsyncSink, Stream, Sink, StartSend};
use tokio_core::io::Io;

use tungstenite::handshake::client::{ClientHandshake, Request};
use tungstenite::handshake::server::ServerHandshake;
use tungstenite::handshake::{Handshake, HandshakeResult};
use tungstenite::protocol::{WebSocket, Message};
use tungstenite::error::Error as WsError;

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

/// Future returned from `ClientHandshakeExt::new_async` which will resolve
/// once the connection handshake has finished.
pub struct ClientHandshakeAsync<S> {
    inner: Option<ClientHandshake<S>>,
}

/// Future returned from `ServerHandshakeExt::new_async` which will resolve
/// once the connection handshake has finished.
pub struct ServerHandshakeAsync<S: Io> {
    inner: Option<ServerHandshake<S>>,
}

/// Extension trait for the `ClientHandshake` type in the `tungstenite` crate.
pub trait ClientHandshakeExt {
    /// Create a handshake provided stream and assuming the provided request.
    ///
    /// This function will internally call `ClientHandshake::new` to create a
    /// handshake representation and returns a future representing the
    /// resolution of the WebSocket handshake. The returned future will resolve
    /// to either `WebSocketStream<S>` or `Error` depending if it's successful
    /// or not.
    ///
    /// This is typically used for clients who have already established, for
    /// example, a TCP connection to the remove server.
    fn new_async<S: Io>(stream: S, request: Request) -> ClientHandshakeAsync<S>;
}

/// Extension trait for the `ServerHandshake` type in the `tungstenite` crate.
pub trait ServerHandshakeExt {
    /// Accepts a new WebSocket connection with the provided stream.
    ///
    /// This function will internally call `ServerHandshake::new` to create a
    /// handshake representation and returns a future representing the
    /// resolution of the WebSocket handshake. The returned future will resolve
    /// to either `WebSocketStream<S>` or `Error` depending if it's successful
    /// or not.
    ///
    /// This is typically used after a socket has been accepted from a
    /// `TcpListener`. That socket is then passed to this function to perform
    /// the server half of the accepting a client's websocket connection.
    fn new_async<S: Io>(stream: S) -> ServerHandshakeAsync<S>;
}

impl<S: Io> ClientHandshakeExt for ClientHandshake<S> {
    fn new_async<Stream: Io>(stream: Stream, request: Request) -> ClientHandshakeAsync<Stream> {
        ClientHandshakeAsync {
            inner: Some(ClientHandshake::new(stream, request)),
        }
    }
}

impl<S: Io> ServerHandshakeExt for ServerHandshake<S> {
    fn new_async<Stream: Io>(stream: Stream) -> ServerHandshakeAsync<Stream> {
        ServerHandshakeAsync {
            inner: Some(ServerHandshake::new(stream)),
        }
    }
}

impl<S: Io> Future for ClientHandshakeAsync<S> {
    type Item = WebSocketStream<S>;
    type Error = WsError;

    fn poll(&mut self) -> Poll<WebSocketStream<S>, WsError> {
        let hs = self.inner.take().expect("Cannot poll a handshake twice");
        match hs.handshake()? {
            HandshakeResult::Done(stream) => {
                Ok(WebSocketStream { inner: stream }.into())
            },
            HandshakeResult::Incomplete(handshake) => {
                self.inner= Some(handshake);
                Ok(Async::NotReady)
            },
        }
    }
}

impl<S: Io> Future for ServerHandshakeAsync<S> {
    type Item = WebSocketStream<S>;
    type Error = WsError;

    fn poll(&mut self) -> Poll<WebSocketStream<S>, WsError> {
        let hs = self.inner.take().expect("Cannot poll a handshake twice");
        match hs.handshake()? {
            HandshakeResult::Done(stream) => {
                Ok(WebSocketStream { inner: stream }.into())
            },
            HandshakeResult::Incomplete(handshake) => {
                self.inner= Some(handshake);
                Ok(Async::NotReady)
            },
        }
    }
}

impl<T> Stream for WebSocketStream<T> where T: Io {
    type Item = Message;
    type Error = WsError;

    fn poll(&mut self) -> Poll<Option<Message>, WsError> {
        match self.inner.read_message() {
            Ok(message) => Ok(Async::Ready(Some(message))),
            Err(error) => {
                match error {
                    WsError::Io(ref err) if err.kind() == ErrorKind::WouldBlock => Ok(Async::NotReady),
                    _ => Err(error),
                }
            },
        }
    }
}

impl<T> Sink for WebSocketStream<T> where T: Io {
    type SinkItem = Message;
    type SinkError = WsError;

    fn start_send(&mut self, item: Message) -> StartSend<Message, WsError> {
        try!(self.inner.write_message(item));
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), WsError> {
        Ok(Async::Ready(()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
