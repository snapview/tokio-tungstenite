//! Async WebSocket usage.
//!
//! This library is an implementation of WebSocket handshakes and streams. It
//! is based on the crate which implements all required WebSocket protocol
//! logic. So this crate basically just brings tokio support / tokio integration
//! to it.
//!
//! Each WebSocket stream implements the required `Stream` and `Sink` traits,
//! so the socket is just a stream of messages coming in and going out.

#![deny(
    missing_docs,
    unused_must_use,
    unused_mut,
    unused_imports,
    unused_import_braces)]

extern crate futures;
extern crate tokio_io;

pub extern crate tungstenite;

#[cfg(feature="connect")]
mod connect;

#[cfg(feature="stream")]
pub mod stream;

use std::io::ErrorKind;

use futures::{Poll, Future, Async, AsyncSink, Stream, Sink, StartSend};
use tokio_io::{AsyncRead, AsyncWrite};

use tungstenite::handshake::client::{ClientHandshake, Response, Request};
use tungstenite::handshake::server::{ServerHandshake, Callback, NoCallback};
use tungstenite::handshake::{HandshakeRole, HandshakeError};
use tungstenite::protocol::{WebSocket, Message};
use tungstenite::error::Error as WsError;
use tungstenite::server;

#[cfg(feature="connect")]
pub use connect::{connect_async, client_async_tls};

/// Creates a WebSocket handshake from a request and a stream.
/// For convenience, the user may call this with a url string, a URL,
/// or a `Request`. Calling with `Request` allows the user to add
/// a WebSocket protocol or other custom headers.
///
/// Internally, this custom creates a handshake representation and returns
/// a future representing the resolution of the WebSocket handshake. The
/// returned future will resolve to either `WebSocketStream<S>` or `Error`
/// depending on whether the handshake is successful.
///
/// This is typically used for clients who have already established, for
/// example, a TCP connection to the remote server.
pub fn client_async<'a, R, S>(request: R, stream: S) -> ConnectAsync<S>
where
    R: Into<Request<'a>>,
    S: AsyncRead + AsyncWrite
{
    ConnectAsync {
        inner: MidHandshake {
            inner: Some(ClientHandshake::start(stream, request.into()).handshake())
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
pub fn accept_async<S>(stream: S) -> AcceptAsync<S, NoCallback>
where
    S: AsyncRead + AsyncWrite,
{
    accept_hdr_async(stream, NoCallback)
}

/// Accepts a new WebSocket connection with the provided stream.
///
/// This function does the same as `accept_async()` but accepts an extra callback
/// for header processing. The callback receives headers of the incoming
/// requests and is able to add extra headers to the reply.
pub fn accept_hdr_async<S, C>(stream: S, callback: C) -> AcceptAsync<S, C>
where
    S: AsyncRead + AsyncWrite,
    C: Callback,
{
    AcceptAsync {
        inner: MidHandshake {
            inner: Some(server::accept_hdr(stream, callback))
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

impl<T> Stream for WebSocketStream<T> where T: AsyncRead + AsyncWrite {
    type Item = Message;
    type Error = WsError;

    fn poll(&mut self) -> Poll<Option<Message>, WsError> {
        self.inner.read_message().map(|m| Some(m)).to_async()
    }
}

impl<T> Sink for WebSocketStream<T> where T: AsyncRead + AsyncWrite {
    type SinkItem = Message;
    type SinkError = WsError;

    fn start_send(&mut self, item: Message) -> StartSend<Message, WsError> {
        self.inner.write_message(item).to_start_send()
    }

    fn poll_complete(&mut self) -> Poll<(), WsError> {
        self.inner.write_pending().to_async()
    }

    fn close(&mut self) -> Poll<(), WsError> {
        self.inner.close(None).to_async()
    }
}

/// Future returned from client_async() which will resolve
/// once the connection handshake has finished.
pub struct ConnectAsync<S: AsyncRead + AsyncWrite> {
    inner: MidHandshake<ClientHandshake<S>>,
}

impl<S: AsyncRead + AsyncWrite> Future for ConnectAsync<S> {
    type Item = (WebSocketStream<S>, Response);
    type Error = WsError;

    fn poll(&mut self) -> Poll<Self::Item, WsError> {
        match self.inner.poll()? {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready((ws, resp)) => Ok(Async::Ready((WebSocketStream { inner: ws }, resp))),
        }
    }
}

/// Future returned from accept_async() which will resolve
/// once the connection handshake has finished.
pub struct AcceptAsync<S: AsyncRead + AsyncWrite, C: Callback> {
    inner: MidHandshake<ServerHandshake<S, C>>,
}

impl<S: AsyncRead + AsyncWrite, C: Callback> Future for AcceptAsync<S, C> {
    type Item = WebSocketStream<S>;
    type Error = WsError;

    fn poll(&mut self) -> Poll<Self::Item, WsError> {
        match self.inner.poll()? {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(ws) => Ok(Async::Ready(WebSocketStream { inner: ws })),
        }
    }
}

struct MidHandshake<H: HandshakeRole> {
    inner: Option<Result<<H as HandshakeRole>::FinalResult, HandshakeError<H>>>,
}

impl<H: HandshakeRole> Future for MidHandshake<H> {
    type Item = <H as HandshakeRole>::FinalResult;
    type Error = WsError;

    fn poll(&mut self) -> Poll<Self::Item, WsError> {
        match self.inner.take().expect("cannot poll MidHandshake twice") {
            Ok(result) => Ok(Async::Ready(result)),
            Err(HandshakeError::Failure(e)) => Err(e),
            Err(HandshakeError::Interrupted(s)) => {
                match s.handshake() {
                    Ok(result) => Ok(Async::Ready(result)),
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

trait ToStartSend {
    type T;
    type E;
    fn to_start_send(self) -> StartSend<Self::T, Self::E>;
}

impl ToStartSend for Result<(), (WsError, Option<Message>)> {
    type T = Message;
    type E = WsError;
    fn to_start_send(self) -> StartSend<Self::T, Self::E> {
        match self {
            Ok(_) => Ok(AsyncSink::Ready),
            Err((_, Some(msg))) => Ok(AsyncSink::NotReady(msg)),
            Err((error, None)) => match error {
                WsError::Io(ref err) if err.kind() == ErrorKind::WouldBlock => Ok(AsyncSink::Ready),
                err => Err(err),
            },
        }
    }
}

