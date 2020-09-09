use tungstenite::extensions::WebSocketExtension;

#[cfg(feature = "deflate")]
pub mod async_deflate;

pub trait AsyncWebSocketExtension: WebSocketExtension + Unpin {}

impl<E> AsyncWebSocketExtension for E where E: WebSocketExtension + Unpin {}
