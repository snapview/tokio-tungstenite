//! Connection helper.

extern crate tokio_dns;
extern crate tokio_core;

use std::io::Result as IoResult;

use self::tokio_core::net::TcpStream;
use self::tokio_core::reactor::Remote;
use self::tokio_dns::tcp_connect;

use futures::{future, Future};
use tokio_io::{AsyncRead, AsyncWrite};

use tungstenite::Error;
use tungstenite::client::url_mode;
use tungstenite::handshake::client::Response;

use stream::NoDelay;
use super::{WebSocketStream, Request, client_async};

impl NoDelay for TcpStream {
    fn set_nodelay(&mut self, nodelay: bool) -> IoResult<()> {
        TcpStream::set_nodelay(self, nodelay)
    }
}

#[cfg(feature="tls")]
mod encryption {
    extern crate native_tls;
    extern crate tokio_tls;

    use self::native_tls::TlsConnector;
    use self::tokio_tls::{TlsConnectorExt, TlsStream};

    use std::io::{Read, Write, Result as IoResult};

    use futures::{future, Future};
    use tokio_io::{AsyncRead, AsyncWrite};

    use tungstenite::Error;
    use tungstenite::stream::Mode;

    use stream::NoDelay;

    pub use stream::Stream as StreamSwitcher;
    pub type AutoStream<S> = StreamSwitcher<S, TlsStream<S>>;

    impl<T: Read + Write + NoDelay> NoDelay for TlsStream<T> {
        fn set_nodelay(&mut self, nodelay: bool) -> IoResult<()> {
            self.get_mut().get_mut().set_nodelay(nodelay)
        }
    }

    pub fn wrap_stream<S>(socket: S, domain: String, mode: Mode)
        -> Box<Future<Item=AutoStream<S>, Error=Error>>
    where
        S: 'static + AsyncRead + AsyncWrite,
    {
        match mode {
            Mode::Plain => Box::new(future::ok(StreamSwitcher::Plain(socket))),
            Mode::Tls => {
                Box::new(future::result(TlsConnector::builder())
                            .and_then(move |builder| future::result(builder.build()))
                            .and_then(move |connector| connector.connect_async(&domain, socket))
                            .map(|s| StreamSwitcher::Tls(s))
                            .map_err(|e| Error::Tls(e)))
            }
        }
    }
}

#[cfg(not(feature="tls"))]
mod encryption {
    use futures::{future, Future};
    use tokio_io::{AsyncRead, AsyncWrite};

    use tungstenite::Error;
    use tungstenite::stream::Mode;

    pub type AutoStream<S> = S;

    pub fn wrap_stream<S>(socket: S, _domain: String, mode: Mode)
        -> Box<Future<Item=AutoStream<S>, Error=Error>>
    where
        S: 'static + AsyncRead + AsyncWrite,
    {
        match mode {
            Mode::Plain => Box::new(future::ok(socket)),
            Mode::Tls => Box::new(future::err(Error::Url("TLS support not compiled in.".into()))),
        }
    }
}

use self::encryption::{AutoStream, wrap_stream};

/// Get a domain from an URL.
#[inline]
fn domain(request: &Request) -> Result<String, Error> {
    match request.url.host_str() {
        Some(d) => Ok(d.to_string()),
        None => Err(Error::Url("no host name in the url".into())),
    }
}

/// Creates a WebSocket handshake from a request and a stream,
/// upgrading the stream to TLS if required.
pub fn client_async_tls<R, S>(request: R, stream: S)
    -> Box<Future<Item=(WebSocketStream<AutoStream<S>>, Response), Error=Error>>
where
    R: Into<Request<'static>>,
    S: 'static + AsyncRead + AsyncWrite + NoDelay,
{
    let request: Request = request.into();

    let domain = match domain(&request) {
        Ok(domain) => domain,
        Err(err) => return Box::new(future::err(err)),
    };

    // Make sure we check domain and mode first. URL must be valid.
    let mode = match url_mode(&request.url) {
        Ok(m) => m,
        Err(e) => return Box::new(future::err(e.into())),
    };

    Box::new(wrap_stream(stream, domain, mode)
                .and_then(|mut stream| {
                    NoDelay::set_nodelay(&mut stream, true)
                        .map(move |()| stream)
                        .map_err(|e| e.into())
                })
                .and_then(move |stream| client_async(request, stream)))
}

/// Connect to a given URL.
pub fn connect_async<R>(request: R, handle: Remote)
    -> Box<Future<Item=(WebSocketStream<AutoStream<TcpStream>>, Response), Error=Error>>
where
    R: Into<Request<'static>>
{
    let request: Request = request.into();

    let domain = match domain(&request) {
        Ok(domain) => domain,
        Err(err) => return Box::new(future::err(err)),
    };
    let port = request.url.port_or_known_default().expect("Bug: port unknown");

    Box::new(tcp_connect((domain.as_str(), port), handle).map_err(|e| e.into())
                .and_then(move |socket| client_async_tls(request, socket)))
}
