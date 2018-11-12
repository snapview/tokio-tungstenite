//! Connection helper.

extern crate tokio_dns;
extern crate tokio_tcp;

use std::net::SocketAddr;
use std::io::Result as IoResult;

use self::tokio_tcp::TcpStream;

use futures::{future, Future};
use tokio_io::{AsyncRead, AsyncWrite};

use tungstenite::Error;
use tungstenite::client::url_mode;
use tungstenite::handshake::client::Response;

use stream::{NoDelay, PeerAddr};
use super::{WebSocketStream, Request, client_async};

impl NoDelay for TcpStream {
    fn set_nodelay(&mut self, nodelay: bool) -> IoResult<()> {
        TcpStream::set_nodelay(self, nodelay)
    }
}

impl PeerAddr for TcpStream {
    fn peer_addr(&self) -> IoResult<SocketAddr> {
        self.peer_addr()
    }
}

#[cfg(feature="tls")]
mod encryption {
    extern crate native_tls;
    extern crate tokio_tls;

    use self::native_tls::TlsConnector;
    use self::tokio_tls::{TlsConnector as TokioTlsConnector, TlsStream};

    use std::net::SocketAddr;
    use std::io::{Read, Write, Result as IoResult};

    use futures::{future, Future};
    use tokio_io::{AsyncRead, AsyncWrite};

    use tungstenite::Error;
    use tungstenite::stream::Mode;

    use stream::{NoDelay, PeerAddr, Stream as StreamSwitcher};

    /// A stream that might be protected with TLS.
    pub type MaybeTlsStream<S> = StreamSwitcher<S, TlsStream<S>>;

    pub type AutoStream<S> = MaybeTlsStream<S>;

    impl<T: Read + Write + NoDelay> NoDelay for TlsStream<T> {
        fn set_nodelay(&mut self, nodelay: bool) -> IoResult<()> {
            self.get_mut().get_mut().set_nodelay(nodelay)
        }
    }

    impl<S: Read + Write + PeerAddr> PeerAddr for TlsStream<S> {
        fn peer_addr(&self) -> IoResult<SocketAddr> {
            self.get_ref().get_ref().peer_addr()
        }
    }

    pub fn wrap_stream<S>(socket: S, domain: String, mode: Mode)
        -> Box<Future<Item=AutoStream<S>, Error=Error> + Send>
    where
        S: 'static + AsyncRead + AsyncWrite + Send,
    {
        match mode {
            Mode::Plain => Box::new(future::ok(StreamSwitcher::Plain(socket))),
            Mode::Tls => {
                Box::new(future::result(TlsConnector::new())
                            .map(TokioTlsConnector::from)
                            .and_then(move |connector| connector.connect(&domain, socket))
                            .map(|s| StreamSwitcher::Tls(s))
                            .map_err(|e| Error::Tls(e)))
            }
        }
    }
}

#[cfg(feature="tls")]
pub use self::encryption::MaybeTlsStream;

#[cfg(not(feature="tls"))]
mod encryption {
    use futures::{future, Future};
    use tokio_io::{AsyncRead, AsyncWrite};

    use tungstenite::Error;
    use tungstenite::stream::Mode;

    pub type AutoStream<S> = S;

    pub fn wrap_stream<S>(socket: S, _domain: String, mode: Mode)
        -> Box<Future<Item=AutoStream<S>, Error=Error> + Send>
    where
        S: 'static + AsyncRead + AsyncWrite + Send,
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
    -> Box<Future<Item=(WebSocketStream<AutoStream<S>>, Response), Error=Error> + Send>
where
    R: Into<Request<'static>>,
    S: 'static + AsyncRead + AsyncWrite + NoDelay + Send,
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
pub fn connect_async<R>(request: R)
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
                .and_then(move |socket| client_async_tls(request, socket)))
}
