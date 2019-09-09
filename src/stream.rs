//! Convenience wrapper for streams to switch between plain TCP and TLS at runtime.
//!
//!  There is no dependency on actual TLS implementations. Everything like
//! `native_tls` or `openssl` will work as long as there is a TLS stream supporting standard
//! `Read + Write` traits.
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite};

/// Stream, either plain TCP or TLS.
pub enum Stream<S, T> {
    /// Unencrypted socket stream.
    Plain(S),
    /// Encrypted socket stream.
    Tls(T),
}

impl<S: AsyncRead + Unpin, T: AsyncRead + Unpin> AsyncRead for Stream<S, T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        match *self {
            Stream::Plain(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_read(cx, buf)
            }
            Stream::Tls(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_read(cx, buf)
            }
        }
    }
}

impl<S: AsyncWrite + Unpin, T: AsyncWrite + Unpin> AsyncWrite for Stream<S, T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match *self {
            Stream::Plain(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_write(cx, buf)
            }
            Stream::Tls(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_write(cx, buf)
            }
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match *self {
            Stream::Plain(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_flush(cx)
            }
            Stream::Tls(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_flush(cx)
            }
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match *self {
            Stream::Plain(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_shutdown(cx)
            }
            Stream::Tls(ref mut s) => {
                let pinned = unsafe { Pin::new_unchecked(s) };
                pinned.poll_shutdown(cx)
            }
        }
    }
}
