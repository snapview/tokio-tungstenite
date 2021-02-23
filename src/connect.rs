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

        use crate::stream::MaybeTlsStream;

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
                Mode::Plain => Ok(MaybeTlsStream::Plain(socket)),
                Mode::Tls => {
                    let try_connector = tls_connector.map_or_else(TlsConnector::new, Ok);
                    let connector = try_connector.map_err(TlsError::Native)?;
                    let stream = TokioTlsConnector::from(connector);
                    let connected = stream.connect(&domain, socket).await;
                    match connected {
                        Err(e) => Err(Error::Tls(e.into())),
                        Ok(s) => Ok(MaybeTlsStream::NativeTls(s)),
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

        use crate::stream::MaybeTlsStream;

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
                Mode::Plain => Ok(MaybeTlsStream::Plain(socket)),
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
                        Ok(s) => Ok(MaybeTlsStream::Rustls(s)),
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

        use crate::stream::MaybeTlsStream;

        pub async fn wrap_stream<S>(socket: S, mode: Mode) -> Result<MaybeTlsStream<S>, Error>
        where
            S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
        {
            match mode {
                Mode::Plain => Ok(MaybeTlsStream::Plain(socket)),
                Mode::Tls => Err(Error::Url(UrlError::TlsFeatureNotEnabled)),
            }
        }
    }
}

pub use self::encryption::TlsConnector;
pub use crate::stream::MaybeTlsStream;

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
