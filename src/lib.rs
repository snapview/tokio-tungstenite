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
extern crate tokio_io;
extern crate tungstenite;
extern crate url;

use std::io::ErrorKind;

use futures::{Poll, Future, Async, AsyncSink, Stream, Sink, StartSend};
use tokio_io::{AsyncRead, AsyncWrite};

use url::Url;

use tungstenite::handshake::client::ClientHandshake;
use tungstenite::handshake::server::ServerHandshake;
use tungstenite::handshake::{HandshakeRole, HandshakeError};
use tungstenite::protocol::{WebSocket, Message};
use tungstenite::error::Error as WsError;
use tungstenite::server;

/// A WebSocket request
pub struct Request<'a> {
    pub url: Url,
    pub headers: Vec<(&'a str, &'a str)>,
}

impl<'a> Request<'a> {
    /// Constructs a new WebSocket request with a URL or URL string
    pub fn new<U: Into<Url>>(url: U) -> Self {
        Request{url: url.into(), headers: vec![]}
    }

    /// Adds a WebSocket protocol to the request
    pub fn add_protocol(&mut self, protocol: &'a str) {
        self.headers.push(("Sec-WebSocket-Protocol", protocol));
    }

    /// Adds a custom header to the request
    pub fn add_header(&mut self, name: &'a str, value: &'a str) {
        self.headers.push((name, value));
    }
}

impl<'a, U: Into<Url>> From<U> for Request<'a> {
    fn from(u: U) -> Request<'a> {
        Request::new(u)
    }
}

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
/// example, a TCP connection to the remove server.
pub fn client_async<'a, R, S>(request: R, stream: S) -> ConnectAsync<S>
    where R: Into<Request<'a>>, S: AsyncRead + AsyncWrite {
    let Request{url, headers} = request.into();
    let tungstenite_request = tungstenite::handshake::client::Request{url: url, extra_headers: Some(&headers)};
    let handshake = ClientHandshake::start(stream, tungstenite_request).handshake();

    ConnectAsync {
        inner: MidHandshake {
            inner: Some(handshake)
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
pub fn accept_async<S: AsyncRead + AsyncWrite>(stream: S) -> AcceptAsync<S> {
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
        try!(self.inner.write_message(item).to_async());
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), WsError> {
        self.inner.write_pending().to_async()
    }
}

/// Future returned from client_async() which will resolve
/// once the connection handshake has finished.
pub struct ConnectAsync<S> {
    inner: MidHandshake<S, ClientHandshake>,
}

impl<S: AsyncRead + AsyncWrite> Future for ConnectAsync<S> {
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

impl<S: AsyncRead + AsyncWrite> Future for AcceptAsync<S> {
    type Item = WebSocketStream<S>;
    type Error = WsError;

    fn poll(&mut self) -> Poll<WebSocketStream<S>, WsError> {
        self.inner.poll()
    }
}

struct MidHandshake<S, R> {
    inner: Option<Result<WebSocket<S>, HandshakeError<S, R>>>,
}

impl<S: AsyncRead + AsyncWrite, R: HandshakeRole> Future for MidHandshake<S, R> {
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

