//! Connection helper.

extern crate tokio_dns;
extern crate tokio_core;

use std::io::Result as IoResult;

use self::tokio_core::net::TcpStream;
use self::tokio_core::reactor::Remote;
use self::tokio_dns::tcp_connect;

use futures::future;
use futures::{Future, BoxFuture};
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

    use super::tokio_core::net::TcpStream;

    use self::native_tls::TlsConnector;
    use self::tokio_tls::{TlsConnectorExt, TlsStream};

    use std::io::{Read, Write, Result as IoResult};

    use futures::{Future, BoxFuture};
    use futures::future;

    use tungstenite::Error;
    use tungstenite::stream::Mode;

    use stream::NoDelay;

    pub use stream::Stream as StreamSwitcher;
    pub type AutoStream = StreamSwitcher<TcpStream, TlsStream<TcpStream>>;

    impl<T: Read + Write + NoDelay> NoDelay for TlsStream<T> {
        fn set_nodelay(&mut self, nodelay: bool) -> IoResult<()> {
            self.get_mut().get_mut().set_nodelay(nodelay)
        }
    }

    pub fn wrap_stream(socket: TcpStream, domain: String, mode: Mode) -> BoxFuture<AutoStream, Error> {
        match mode {
            Mode::Plain => future::ok(StreamSwitcher::Plain(socket)).boxed(),
            Mode::Tls => {
                future::result(TlsConnector::builder())
                .and_then(move |builder| future::result(builder.build()))
                .and_then(move |connector| connector.connect_async(&domain, socket))
                .map(|s| StreamSwitcher::Tls(s))
                .map_err(|e| Error::Tls(e))
                .boxed()
            }
        }
    }
}

#[cfg(not(feature="tls"))]
mod encryption {
    use super::tokio_core::net::TcpStream;

    use futures::{Future, BoxFuture};
    use futures::future;

    use tungstenite::Error;
    use tungstenite::stream::Mode;

    pub type AutoStream = TcpStream;

    pub fn wrap_stream(socket: TcpStream, _domain: String, mode: Mode) -> BoxFuture<AutoStream, Error> {
        match mode {
            Mode::Plain => future::ok(socket).boxed(),
            Mode::Tls => future::err(Error::Url("TLS support not compiled in.".into())).boxed(),
        }
    }
}

use self::encryption::{AutoStream, wrap_stream};

/// Connect to a given URL.
pub fn connect_async<R>(request: R, handle: Remote)
    -> BoxFuture<(WebSocketStream<AutoStream>, Response), Error>
where
    R: Into<Request<'static>>
{
    let request: Request = request.into();

    // Make sure we check domain and mode first. URL must be valid.
    let mode = match url_mode(&request.url) {
        Ok(m) => m,
        Err(e) => return future::err(e.into()).boxed(),
    };
    let domain = match request.url.host_str() {
        Some(d) => d.to_string(),
        None => return future::err(Error::Url("No host name in the URL".into())).boxed(),
    };
    let port = request.url.port_or_known_default().expect("Bug: port unknown");

    tcp_connect((domain.as_str(), port), handle).map_err(|e| e.into())
    .and_then(move |socket| wrap_stream(socket, domain, mode))
    .and_then(|mut stream| {
        NoDelay::set_nodelay(&mut stream, true)
        .map(move |()| stream)
        .map_err(|e| e.into())
     })
    .and_then(move |stream| client_async(request, stream))
    .boxed()
}
