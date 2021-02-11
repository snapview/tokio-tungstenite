//! Connection helper.
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

use tungstenite::{
    client::uri_mode,
    error::{Error, UrlError},
    handshake::client::{Request, Response},
    protocol::WebSocketConfig,
};

use super::{client_async_with_config, IntoClientRequest, WebSocketStream};

pub(crate) mod encryption {
    use std::{
        marker::PhantomData,
        pin::Pin,
        task::{Context, Poll},
    };

    use pin_project::pin_project;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    use crate::stream::Stream;

    /// A stream that might be protected with TLS.
    pub type MaybeTlsStream<S> = Stream<S, TlsStream<S>>;

    #[allow(missing_docs)]
    #[non_exhaustive]
    #[pin_project(project = StreamProj)]
    pub enum TlsStream<S> {
        Plain(PhantomData<S>),
        #[cfg(feature = "native-tls")]
        NativeTls(tokio_native_tls::TlsStream<S>),
        #[cfg(feature = "rustls-tls")]
        Rustls(tokio_rustls::client::TlsStream<S>),
    }

    #[cfg_attr(not(any(feature = "native-tls", feature = "rustls-tls")), allow(unused_variables))]
    impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for TlsStream<S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            match self.project() {
                StreamProj::Plain(_) => Poll::Ready(Ok(())),
                #[cfg(feature = "native-tls")]
                StreamProj::NativeTls(s) => Pin::new(s).poll_read(cx, buf),
                #[cfg(feature = "rustls-tls")]
                StreamProj::Rustls(s) => Pin::new(s).poll_read(cx, buf),
            }
        }
    }

    #[cfg_attr(not(any(feature = "native-tls", feature = "rustls-tls")), allow(unused_variables))]
    impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for TlsStream<S> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            match self.project() {
                StreamProj::Plain(_) => Poll::Ready(Ok(0)),
                #[cfg(feature = "native-tls")]
                StreamProj::NativeTls(s) => Pin::new(s).poll_write(cx, buf),
                #[cfg(feature = "rustls-tls")]
                StreamProj::Rustls(s) => Pin::new(s).poll_write(cx, buf),
            }
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            match self.project() {
                StreamProj::Plain(_) => Poll::Ready(Ok(())),
                #[cfg(feature = "native-tls")]
                StreamProj::NativeTls(s) => Pin::new(s).poll_flush(cx),
                #[cfg(feature = "rustls-tls")]
                StreamProj::Rustls(s) => Pin::new(s).poll_flush(cx),
            }
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            match self.project() {
                StreamProj::Plain(_) => Poll::Ready(Ok(())),
                #[cfg(feature = "native-tls")]
                StreamProj::NativeTls(s) => Pin::new(s).poll_shutdown(cx),
                #[cfg(feature = "rustls-tls")]
                StreamProj::Rustls(s) => Pin::new(s).poll_shutdown(cx),
            }
        }
    }

    /// A TLS connector that can be used when establishing TLS connections.
    #[non_exhaustive]
    pub enum TlsConnector {
        /// Plain (non-TLS) connector.
        Plain,
        /// `native-tls` TLS connector.
        #[cfg(feature = "native-tls")]
        NativeTls(native_tls_crate::TlsConnector),
        /// `rustls` TLS connector.
        #[cfg(feature = "rustls-tls")]
        Rustls(std::sync::Arc<rustls::ClientConfig>),
    }

    #[cfg(feature = "native-tls")]
    pub mod native_tls {
        use native_tls_crate::TlsConnector;
        use tokio_native_tls::TlsConnector as TokioTlsConnector;

        use tokio::io::{AsyncRead, AsyncWrite};

        use tungstenite::{error::TlsError, stream::Mode, Error};

        use super::{MaybeTlsStream, TlsStream};
        use crate::stream::Stream as StreamSwitcher;

        pub async fn wrap_stream<S>(
            socket: S,
            domain: String,
            mode: Mode,
            tls_connector: Option<TlsConnector>,
        ) -> Result<MaybeTlsStream<S>, Error>
        where
            S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
        {
            match mode {
                Mode::Plain => Ok(StreamSwitcher::Plain(socket)),
                Mode::Tls => {
                    let try_connector = tls_connector.map_or_else(TlsConnector::new, Ok);
                    let connector = try_connector.map_err(TlsError::Native)?;
                    let stream = TokioTlsConnector::from(connector);
                    let connected = stream.connect(&domain, socket).await;
                    match connected {
                        Err(e) => Err(Error::Tls(e.into())),
                        Ok(s) => Ok(StreamSwitcher::Tls(TlsStream::NativeTls(s))),
                    }
                }
            }
        }
    }

    #[cfg(feature = "rustls-tls")]
    pub mod rustls {
        pub use rustls::ClientConfig;
        use tokio_rustls::{webpki::DNSNameRef, TlsConnector as TokioTlsConnector};

        use std::sync::Arc;
        use tokio::io::{AsyncRead, AsyncWrite};

        use tungstenite::{error::TlsError, stream::Mode, Error};

        use super::{MaybeTlsStream, TlsStream};
        use crate::stream::Stream as StreamSwitcher;

        pub async fn wrap_stream<S>(
            socket: S,
            domain: String,
            mode: Mode,
            tls_connector: Option<Arc<ClientConfig>>,
        ) -> Result<MaybeTlsStream<S>, Error>
        where
            S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
        {
            match mode {
                Mode::Plain => Ok(StreamSwitcher::Plain(socket)),
                Mode::Tls => {
                    let config = tls_connector.unwrap_or_else(|| {
                        let mut config = ClientConfig::new();
                        config.root_store.add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);

                        Arc::new(config)
                    });
                    let domain = DNSNameRef::try_from_ascii_str(&domain).map_err(TlsError::Dns)?;
                    let stream = TokioTlsConnector::from(config);
                    let connected = stream.connect(domain, socket).await;

                    match connected {
                        Err(e) => Err(Error::Io(e)),
                        Ok(s) => Ok(StreamSwitcher::Tls(TlsStream::Rustls(s))),
                    }
                }
            }
        }
    }

    pub mod plain {
        use tokio::io::{AsyncRead, AsyncWrite};

        use tungstenite::{
            error::{Error, UrlError},
            stream::Mode,
        };

        use super::MaybeTlsStream;
        use crate::stream::Stream as StreamSwitcher;

        pub async fn wrap_stream<S>(socket: S, mode: Mode) -> Result<MaybeTlsStream<S>, Error>
        where
            S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
        {
            match mode {
                Mode::Plain => Ok(StreamSwitcher::Plain(socket)),
                Mode::Tls => Err(Error::Url(UrlError::TlsFeatureNotEnabled)),
            }
        }
    }
}

pub use self::encryption::{MaybeTlsStream, TlsConnector, TlsStream};

/// Get a domain from an URL.
#[inline]
fn domain(request: &Request) -> Result<String, Error> {
    match request.uri().host() {
        Some(d) => Ok(d.to_string()),
        None => Err(Error::Url(UrlError::NoHostName)),
    }
}

/// Creates a WebSocket handshake from a request and a stream,
/// upgrading the stream to TLS if required.
pub async fn client_async_tls<R, S>(
    request: R,
    stream: S,
) -> Result<(WebSocketStream<MaybeTlsStream<S>>, Response), Error>
where
    R: IntoClientRequest + Unpin,
    S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
    MaybeTlsStream<S>: Unpin,
{
    client_async_tls_with_config(request, stream, None, None).await
}

/// The same as `client_async_tls()` but the one can specify a websocket configuration,
/// and an optional TLS connector. If no connector is specified, the default one will
/// be created.
///
/// Please refer to `client_async_tls()` for more details.
pub async fn client_async_tls_with_config<R, S>(
    request: R,
    stream: S,
    config: Option<WebSocketConfig>,
    tls_connector: Option<TlsConnector>,
) -> Result<(WebSocketStream<MaybeTlsStream<S>>, Response), Error>
where
    R: IntoClientRequest + Unpin,
    S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
    MaybeTlsStream<S>: Unpin,
{
    let request = request.into_client_request()?;

    #[cfg(any(feature = "native-tls", feature = "rustls-tls"))]
    let domain = domain(&request)?;

    // Make sure we check domain and mode first. URL must be valid.
    let mode = uri_mode(&request.uri())?;

    let stream = match tls_connector {
        Some(conn) => match conn {
            #[cfg(feature = "native-tls")]
            TlsConnector::NativeTls(conn) => {
                self::encryption::native_tls::wrap_stream(stream, domain, mode, Some(conn)).await
            }
            #[cfg(feature = "rustls-tls")]
            TlsConnector::Rustls(conn) => {
                self::encryption::rustls::wrap_stream(stream, domain, mode, Some(conn)).await
            }
            TlsConnector::Plain => self::encryption::plain::wrap_stream(stream, mode).await,
        },
        None => {
            #[cfg(feature = "native-tls")]
            {
                self::encryption::native_tls::wrap_stream(stream, domain, mode, None).await
            }
            #[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
            {
                self::encryption::rustls::wrap_stream(stream, domain, mode, None).await
            }
            #[cfg(not(any(feature = "native-tls", feature = "rustls-tls")))]
            {
                self::encryption::plain::wrap_stream(stream, mode).await
            }
        }
    }?;

    client_async_with_config(request, stream, config).await
}

/// Connect to a given URL.
pub async fn connect_async<R>(
    request: R,
) -> Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response), Error>
where
    R: IntoClientRequest + Unpin,
{
    connect_async_with_config(request, None).await
}

/// The same as `connect_async()` but the one can specify a websocket configuration.
/// Please refer to `connect_async()` for more details.
pub async fn connect_async_with_config<R>(
    request: R,
    config: Option<WebSocketConfig>,
) -> Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response), Error>
where
    R: IntoClientRequest + Unpin,
{
    let request = request.into_client_request()?;

    let domain = domain(&request)?;
    let port = request
        .uri()
        .port_u16()
        .or_else(|| match request.uri().scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or(Error::Url(UrlError::UnsupportedUrlScheme))?;

    let addr = format!("{}:{}", domain, port);
    let try_socket = TcpStream::connect(addr).await;
    let socket = try_socket.map_err(Error::Io)?;
    client_async_tls_with_config(request, socket, config, None).await
}
