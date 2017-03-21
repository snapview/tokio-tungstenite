//! A chat server that broadcasts a message to all connections.
//!
//! This is a simple line-based server which accepts WebSocket connections,
//! reads lines from those connections, and broadcasts the lines to all other
//! connected clients.
//!
//! You can test this out by running:
//!
//!     cargo run --example server 127.0.0.1:12345
//!
//! And then in another window run:
//!
//!     cargo run --example client ws://127.0.0.1:12345/
//!
//! You can run the second command in multiple windows and then chat between the
//! two, seeing the messages from the other client as they're received. For all
//! connected clients they'll all join the same room and see everyone else's
//! messages.

extern crate futures;
extern crate tokio_core;
extern crate tokio_tungstenite;
extern crate tungstenite;

use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::io::{Error, ErrorKind};
use std::rc::Rc;

use futures::stream::Stream;
use futures::Future;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tungstenite::protocol::Message;

use tokio_tungstenite::accept_async;

fn main() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse().unwrap();

    // Create the event loop and TCP listener we'll accept connections on.
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let socket = TcpListener::bind(&addr, &handle).unwrap();
    println!("Listening on: {}", addr);

    // This is a single-threaded server, so we can just use Rc and RefCell to
    // store the map of all connections we know about.
    let connections = Rc::new(RefCell::new(HashMap::new()));

    let srv = socket.incoming().for_each(|(stream, addr)| {

        // We have to clone both of these values, because the `and_then`
        // function billow constructs a new future, `and_then` requires
        // `FnOnce`, so we construct a move closure to move the
        // environment inside the future (AndThen future may overlive our
        // `for_each` future).
        let connections_inner = connections.clone();
        let handle_inner = handle.clone();

        accept_async(stream).and_then(move |ws_stream| {
            println!("New WebSocket connection: {}", addr);

            // Create a channel for our stream, which other sockets will use to
            // send us messages. Then register our address with the stream to send
            // data to us.
            let (tx, rx) = futures::sync::mpsc::unbounded();
            connections_inner.borrow_mut().insert(addr, tx);

            // Let's split the WebSocket stream, so we can work with the
            // reading and writing halves separately.
            let (sink, stream) = ws_stream.split();

            // Whenever we receive a message from the client, we print it and
            // send to other clients, excluding the sender.
            let connections = connections_inner.clone();
            let ws_reader = stream.for_each(move |message: Message| {
                println!("Received a message from {}: {}", addr, message);

                // For each open connection except the sender, send the
                // string via the channel.
                let mut conns = connections.borrow_mut();
                let iter = conns.iter_mut()
                                .filter(|&(&k, _)| k != addr)
                                .map(|(_, v)| v);
                for tx in iter {
                    tx.send(message.clone()).unwrap();
                }
                Ok(())
            });

            // Whenever we receive a string on the Receiver, we write it to
            // `WriteHalf<WebSocketStream>`.
            let ws_writer = rx.fold(sink, |mut sink, msg| {
                use futures::Sink;
                sink.start_send(msg).unwrap();
                Ok(sink)
            });

            // Now that we've got futures representing each half of the socket, we
            // use the `select` combinator to wait for either half to be done to
            // tear down the other. Then we spawn off the result.
            let connection = ws_reader.map(|_| ()).map_err(|_| ())
                                      .select(ws_writer.map(|_| ()).map_err(|_| ()));

            handle_inner.spawn(connection.then(move |_| {
                connections_inner.borrow_mut().remove(&addr);
                println!("Connection {} closed.", addr);
                Ok(())
            }));

            Ok(())
        }).map_err(|e| {
            println!("Error during the websocket handshake occurred: {}", e);
            Error::new(ErrorKind::Other, e)
        })
    });

    // Execute server.
    core.run(srv).unwrap();
}
