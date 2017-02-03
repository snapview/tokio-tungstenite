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

pub struct WebSocketStream<S> {
    inner: WebSocket<S>,
}

pub struct ClientHandshakeAsync<S> {
    inner: Option<ClientHandshake<S>>,
}

pub struct ServerHandshakeAsync<S: Io> {
    inner: Option<ServerHandshake<S>>,
}

pub trait ClientHandshakeExt {
    fn new_async<S: Io>(stream: S, request: Request) -> ClientHandshakeAsync<S>;
}

pub trait ServerHandshakeExt {
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
