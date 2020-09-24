use tungstenite::extensions::WebSocketExtension;

#[cfg(feature = "deflate")]
pub use tungstenite::extensions::deflate::{self, *};

/// An async WebSocket Extension. See `WebSocketExtension` for more information.
pub trait AsyncWebSocketExtension: WebSocketExtension + Unpin {}

impl<E> AsyncWebSocketExtension for E where E: WebSocketExtension + Unpin {}
