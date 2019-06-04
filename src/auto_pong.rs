//! This module contains `AutoPongExt` that allows you to
//! wrap `WebSocketStream` with `AutoPong` instance that
//! will send `Pong` message for every incoming `Ping` to
//! prevent disconnection by ping timeout.

use futures::{try_ready, Poll, Async, AsyncSink, Stream, Sink, StartSend};
use tungstenite::error::Error;
use tungstenite::protocol::Message;

/// Adds `auto_pong` method to streams of messages.
pub trait AutoPongExt {
    /// Wraps a stream with automatic answering to ping messages.
    fn auto_pong(self) -> AutoPong<Self>
        where
            Self: Sized;
}

impl<S> AutoPongExt for S
where
    S: Stream<Item = Message, Error = Error> + Sink<SinkItem = Message, SinkError = Error>,
{
    fn auto_pong(self) -> AutoPong<Self>
    where
        Self: Sized,
    {
        AutoPong {
            do_pong: None,
            stream: self,
        }
    }
}

/// The struct that implements automatic answering to pings.
pub struct AutoPong<S> {
    do_pong: Option<Vec<u8>>,
    stream: S,
}

impl<S> Stream for AutoPong<S>
where
    S: Stream<Item = Message, Error = Error> + Sink<SinkItem = Message, SinkError = Error>,
{
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            if let Some(data) = self.do_pong.take() {
                let answer = Message::Pong(data);
                let res = self.start_send(answer)?;
                match res {
                    AsyncSink::Ready => {
                    }
                    AsyncSink::NotReady(Message::Pong(data)) => {
                        self.do_pong = Some(data);

                    }
                    AsyncSink::NotReady(_) => {
                        panic!("message was modified by Sink");
                    }
                }
                self.poll_complete()?;
            }
            let msg = try_ready!(self.stream.poll());
            match msg {
                Some(Message::Ping(data)) => {
                    self.do_pong = Some(data);
                }
                Some(Message::Pong(_)) => {
                    // Skip incoming pongs
                }
                Some(msg) => {
                    return Ok(Async::Ready(Some(msg)));
                }
                None => {
                    return Ok(Async::Ready(None));
                }
            }
        }
    }
}

impl<S> Sink for AutoPong<S>
where
    S: Stream<Item = Message, Error = Error> + Sink<SinkItem = Message, SinkError = Error>,
{
    type SinkItem = S::SinkItem;
    type SinkError = S::SinkError;

    fn start_send(
        &mut self,
        item: Self::SinkItem
    ) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.stream.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.stream.poll_complete()
    }
}
