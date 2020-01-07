//! Convenience wrapper for streams to switch between plain TCP and TLS at runtime.
//!
//!  There is no dependency on actual TLS implementations. Everything like
//! `native_tls` or `openssl` will work as long as there is a TLS stream supporting standard
//! `Read + Write` traits.
use pin_project::{pin_project, project};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite};

/// Stream, either plain TCP or TLS.
#[pin_project]
pub enum Stream<S, T> {
    /// Unencrypted socket stream.
    Plain(#[pin] S),
    /// Encrypted socket stream.
    Tls(#[pin] T),
}

impl<S: AsyncRead + Unpin, T: AsyncRead + Unpin> AsyncRead for Stream<S, T> {
    #[project]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        #[project]
        match self.project() {
            Stream::Plain(ref mut s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(ref mut s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl<S: AsyncWrite + Unpin, T: AsyncWrite + Unpin> AsyncWrite for Stream<S, T> {
    #[project]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        #[project]
        match self.project() {
            Stream::Plain(ref mut s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(ref mut s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    #[project]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        #[project]
        match self.project() {
            Stream::Plain(ref mut s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(ref mut s) => Pin::new(s).poll_flush(cx),
        }
    }

    #[project]
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        #[project]
        match self.project() {
            Stream::Plain(ref mut s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(ref mut s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}
