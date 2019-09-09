use log::*;
use std::io::{Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite};
use tungstenite::{Error as WsError, WebSocket};

pub(crate) trait HasContext {
    fn set_context(&mut self, context: (bool, *mut ()));
}
#[derive(Debug)]
pub(crate) struct AllowStd<S> {
    pub(crate) inner: S,
    pub(crate) context: (bool, *mut ()),
}

impl<S> HasContext for AllowStd<S> {
    fn set_context(&mut self, context: (bool, *mut ())) {
        self.context = context;
    }
}

pub(crate) struct Guard<'a, S>(pub(crate) &'a mut WebSocket<AllowStd<S>>);

impl<S> Drop for Guard<'_, S> {
    fn drop(&mut self) {
        trace!("{}:{} Guard.drop", file!(), line!());
        (self.0).get_mut().context = (true, std::ptr::null_mut());
    }
}

// *mut () context is neither Send nor Sync
unsafe impl<S: Send> Send for AllowStd<S> {}
unsafe impl<S: Sync> Sync for AllowStd<S> {}

impl<S> AllowStd<S>
where
    S: Unpin,
{
    fn with_context<F, R>(&mut self, f: F) -> Poll<std::io::Result<R>>
    where
        F: FnOnce(&mut Context<'_>, Pin<&mut S>) -> Poll<std::io::Result<R>>,
    {
        trace!("{}:{} AllowStd.with_context", file!(), line!());
        unsafe {
            if !self.context.0 {
                //was called by start_send without context
                return Poll::Pending;
            }
            assert!(!self.context.1.is_null());
            let waker = &mut *(self.context.1 as *mut _);
            f(waker, Pin::new(&mut self.inner))
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    pub(crate) fn get_ref(&self) -> &S {
        &self.inner
    }
}

impl<S> Read for AllowStd<S>
where
    S: AsyncRead + Unpin,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        trace!("{}:{} Read.read", file!(), line!());
        match self.with_context(|ctx, stream| {
            trace!(
                "{}:{} Read.with_context read -> poll_read",
                file!(),
                line!()
            );
            stream.poll_read(ctx, buf)
        }) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }
}

impl<S> Write for AllowStd<S>
where
    S: AsyncWrite + Unpin,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        trace!("{}:{} Write.write", file!(), line!());
        match self.with_context(|ctx, stream| {
            trace!(
                "{}:{} Write.with_context write -> poll_write",
                file!(),
                line!()
            );
            stream.poll_write(ctx, buf)
        }) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        trace!("{}:{} Write.flush", file!(), line!());
        match self.with_context(|ctx, stream| {
            trace!(
                "{}:{} Write.with_context flush -> poll_flush",
                file!(),
                line!()
            );
            stream.poll_flush(ctx)
        }) {
            Poll::Ready(r) => r,
            Poll::Pending => Err(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
        }
    }
}

pub(crate) fn cvt<T>(r: Result<T, WsError>) -> Poll<Result<T, WsError>> {
    match r {
        Ok(v) => Poll::Ready(Ok(v)),
        Err(WsError::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
            debug!("WouldBlock");
            Poll::Pending
        },
        Err(e) => Poll::Ready(Err(e)),
    }
}
