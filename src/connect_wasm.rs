use crate::{stream::MaybeTlsStream, Connector, WebSocketStream, AllowStd};

use tungstenite::{
    error::{Error, UrlError},
    handshake::client::{Request, Response},
    protocol::{
        WebSocketConfig,
        WebSocket,
        Role,
    },
};
use ws_stream_wasm::{
    WsMeta,
    WsStreamIo,
};
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

    //let addr = format!("ws://{domain}:{port}");
    let addr = request.uri().to_string();

    let (mut _ws, wsio) = WsMeta::connect(addr, None ).await.expect("assume the connection succeeds");

    if disable_nagle {
        //socket.set_nodelay(true)?;
    }
    let io = wsio.into_io();

    let result = WebSocketStream::from_raw_socket(MaybeTlsStream::Plain(io), Role::Client, None).await;
    let response = Response::new(None);
    Ok((result, response))
}
