//! Regression test for [`WebSocketStream::into_inner_with_read_buffer`].

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_tungstenite::{tungstenite::protocol::Role, WebSocketStream};

/// A no-op async stream: reads report EOF and writes are accepted and
/// discarded. Enough to construct a `WebSocketStream` without any real
/// handshake I/O, and it is never read by `into_inner_with_read_buffer`.
struct NullStream;

impl AsyncRead for NullStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for NullStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn into_inner_with_read_buffer_recovers_coalesced_bytes() {
    // Model a server that coalesced a WebSocket frame into the same read as the
    // `101 Switching Protocols` response: `from_partially_read` seeds those
    // bytes into the codec read buffer. They must survive a raw takeover of the
    // stream, which plain `into_inner()` discards.
    let coalesced = vec![0x82u8, 0x03, 0x01, 0x02, 0x03];
    let ws =
        WebSocketStream::from_partially_read(NullStream, coalesced.clone(), Role::Client, None)
            .await;
    let (_stream, buffer) = ws.into_inner_with_read_buffer();
    assert_eq!(&buffer[..], &coalesced[..]);
}
