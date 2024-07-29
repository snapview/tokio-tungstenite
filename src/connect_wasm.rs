use crate::{stream::MaybeTlsStream, Connector, WebSocketStream};

use tungstenite::{
    error::{Error, UrlError},
    handshake::client::{Request, Response},
    protocol::WebSocketConfig,
    //client::IntoClientRequest,
};
//use web_sys::WebSocket;
use ws_stream_wasm::WsStreamIo;
use async_io_stream::IoStream;



pub async fn connect(
    request: Request,
    config: Option<WebSocketConfig>,
    disable_nagle: bool,
    //connector: Option<Connector>,
) -> Result<(WebSocketStream<MaybeTlsStream<IoStream<WsStreamIo, Vec<u8>>>>, Response), Error> {
    //let domain = domain(&request)?;
    let domain = request.uri().host().unwrap();
    let port = request
        .uri()
        .port_u16()
        .or_else(|| match request.uri().scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or(Error::Url(UrlError::UnsupportedUrlScheme))?;

    let addr = format!("{domain}:{port}");
    //let socket = TcpStream::connect(addr).await.map_err(Error::Io)?;

    if disable_nagle {
        //socket.set_nodelay(true)?;
    }
    todo!();

    //crate::tls::client_async_tls_with_config(request, socket, config, connector).await
}
