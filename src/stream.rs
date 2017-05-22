//! Convenience wrapper for streams to switch between plain TCP and TLS at runtime.
//!
//!  There is no dependency on actual TLS implementations. Everything like
//! `native_tls` or `openssl` will work as long as there is a TLS stream supporting standard
//! `Read + Write` traits.

extern crate bytes;

use std::io::{Read, Write, Result as IoResult, Error as IoError};

use self::bytes::{Buf, BufMut};
use futures::Poll;
use tokio_io::{AsyncRead, AsyncWrite};

/// Stream, either plain TCP or TLS.
pub enum Stream<S, T> {
    Plain(S),
    Tls(T),
}

impl<S: Read, T: Read> Read for Stream<S, T> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match *self {
            Stream::Plain(ref mut s) => s.read(buf),
            Stream::Tls(ref mut s) => s.read(buf),
        }
    }
}

impl<S: Write, T: Write> Write for Stream<S, T> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        match *self {
            Stream::Plain(ref mut s) => s.write(buf),
            Stream::Tls(ref mut s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> IoResult<()> {
        match *self {
            Stream::Plain(ref mut s) => s.flush(),
            Stream::Tls(ref mut s) => s.flush(),
        }
    }
}

impl<S: AsyncRead, T: AsyncRead> AsyncRead for Stream<S, T> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        match *self {
            Stream::Plain(ref s) => s.prepare_uninitialized_buffer(buf),
            Stream::Tls(ref s) => s.prepare_uninitialized_buffer(buf),
        }
    }
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, IoError> {
        match *self {
            Stream::Plain(ref mut s) => s.read_buf(buf),
            Stream::Tls(ref mut s) => s.read_buf(buf),
        }
    }
}

impl<S: AsyncWrite, T: AsyncWrite> AsyncWrite for Stream<S, T> {
    fn shutdown(&mut self) -> Poll<(), IoError> {
        match *self {
            Stream::Plain(ref mut s) => s.shutdown(),
            Stream::Tls(ref mut s) => s.shutdown(),
        }
    }
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, IoError> {
        match *self {
            Stream::Plain(ref mut s) => s.write_buf(buf),
            Stream::Tls(ref mut s) => s.write_buf(buf),
        }
    }
}
