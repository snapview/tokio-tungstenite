//! Connection helper.

extern crate tokio_dns;

use std::io::Result as IoResult;

use tokio::net::TcpStream;

use futures::{future, Future};
use tokio::io::{AsyncRead, AsyncWrite};

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
    use tokio::io::{AsyncRead, AsyncWrite};

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

    pub fn wrap_stream<S>(socket: S, domain: Option<String>, mode: Mode)
        -> Box<Future<Item=AutoStream<S>, Error=Error> + Send>
    where
        S: 'static + AsyncRead + AsyncWrite + Send,
    {
        match mode {
            Mode::Plain => Box::new(future::ok(StreamSwitcher::Plain(socket))),
            Mode::Tls => {
                Box::new(future::result(TlsConnector::builder().danger_accept_invalid_hostnames(domain.is_none()).build())
                            .and_then(move |connector| if let Some(domain) = domain {
                                connector.connect_async(&domain, socket)
                            } else {
                                connector.connect_async("", socket)
                            })
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

    pub fn wrap_stream<S>(socket: S, _domain: Option<String>, mode: Mode)
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

fn imp_client_async_tls<R, S>(request: R, stream: S, danger_accept_invalid_hostnames: bool)
    -> Box<Future<Item=(WebSocketStream<AutoStream<S>>, Response), Error=Error> + Send>
where
    R: Into<Request<'static>>,
    S: 'static + AsyncRead + AsyncWrite + NoDelay + Send,
{
    let request: Request = request.into();

    let domain = if !danger_accept_invalid_hostnames {
        match domain(&request) {
            Ok(domain) => Some(domain),
            Err(err) => return Box::new(future::err(err)),
        }
    } else {
        None
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

/// Creates a WebSocket handshake from a request and a stream,
/// upgrading the stream to TLS if required.
pub fn client_async_tls<R, S>(request: R, stream: S)
    -> Box<Future<Item=(WebSocketStream<AutoStream<S>>, Response), Error=Error> + Send>
where
    R: Into<Request<'static>>,
    S: 'static + AsyncRead + AsyncWrite + NoDelay + Send,
{
    imp_client_async_tls(request, stream, false)
}

/// Creates a WebSocket handshake from a request and a stream,
/// upgrading the stream to TLS if required.
///
/// Accepts invalid hostnames.
pub fn danger_client_async_tls<R, S>(request: R, stream: S)
    -> Box<Future<Item=(WebSocketStream<AutoStream<S>>, Response), Error=Error> + Send>
where
    R: Into<Request<'static>>,
    S: 'static + AsyncRead + AsyncWrite + NoDelay + Send,
{
    imp_client_async_tls(request, stream, true)
}

fn imp_connect_async<R>(request: R, danger_accept_invalid_hostnames: bool)
    -> Box<Future<Item=(WebSocketStream<AutoStream<TcpStream>>, Response), Error=Error> + Send>
where
    R: Into<Request<'static>>
{
    let request: Request = request.into();

    let domain = match domain(&request) {
        Ok(domain) => domain,
        Err(err) => return Box::new(future::err(err)),
    };

    let port = request.url.port_or_known_default().expect("Bug: port unknown");

    Box::new(tokio_dns::TcpStream::connect((domain.as_str(), port)).map_err(|e| e.into())
        .and_then(move |socket| imp_client_async_tls(request, socket, danger_accept_invalid_hostnames)))
}

/// Connect to a given URL.
pub fn connect_async<R>(request: R)
    -> Box<Future<Item=(WebSocketStream<AutoStream<TcpStream>>, Response), Error=Error> + Send>
where
    R: Into<Request<'static>>
{
    imp_connect_async(request, false)
}

/// Connect to a given URL.
///
/// Accepts invalid hostnames.
pub fn danger_connect_async<R>(request: R)
    -> Box<Future<Item=(WebSocketStream<AutoStream<TcpStream>>, Response), Error=Error> + Send>
where
    R: Into<Request<'static>>
{
    imp_connect_async(request, true)
}
