//! A simple example of hooking up stdin/stdout to a WebSocket stream.
//!
//! This example will connect to a server specified in the argument list and
//! then forward all data read on stdin to the server, printing out all data
//! received on stdout.
//!
//! Note that this is not currently optimized for performance, especially around
//! buffer management. Rather it's intended to show an example of working with a
//! client.
//!
//! You can use this example together with the `server` example.

extern crate futures;
extern crate tokio_core;
extern crate tokio_tungstenite;
extern crate tungstenite;
extern crate url;

use std::env;
use std::io::{self, Read, Write};
use std::thread;

use futures::sync::mpsc;
use futures::{Future, Sink, Stream};
use tokio_core::reactor::Core;
use tungstenite::protocol::Message;

use tokio_tungstenite::connect_async;

fn main() {
    // Specify the server address to which the client will be connecting.
    let connect_addr = env::args().nth(1).unwrap_or_else(|| {
        panic!("this program requires at least one argument")
    });

    let url = url::Url::parse(&connect_addr).unwrap();

    // Create the event loop and initiate the connection to the remote server.
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    // Right now Tokio doesn't support a handle to stdin running on the event
    // loop, so we farm out that work to a separate thread. This thread will
    // read data from stdin and then send it to the event loop over a standard
    // futures channel.
    let (stdin_tx, stdin_rx) = mpsc::channel(0);
    thread::spawn(|| read_stdin(stdin_tx));
    let stdin_rx = stdin_rx.map_err(|_| panic!()); // errors not possible on rx

    // After the TCP connection has been established, we set up our client to
    // start forwarding data.
    //
    // First we do a WebSocket handshake on a TCP stream, i.e. do the upgrade
    // request.
    //
    // Half of the work we're going to do is to take all data we receive on
    // stdin (`stdin_rx`) and send that along the WebSocket stream (`sink`).
    // The second half is to take all the data we receive (`stream`) and then
    // write that to stdout. Currently we just write to stdout in a synchronous
    // fashion.
    //
    // Finally we set the client to terminate once either half of this work
    // finishes. If we don't have any more data to read or we won't receive any
    // more work from the remote then we can exit.
    let mut stdout = io::stdout();
    let client = connect_async(url, handle.remote().clone()).and_then(|ws_stream| {
        println!("WebSocket handshake has been successfully completed");

        // `sink` is the stream of messages going out.
        // `stream` is the stream of incoming messages.
        let (sink, stream) = ws_stream.split();

        // We forward all messages, composed out of the data, entered to
        // the stdin, to the `sink`.
        let send_stdin = stdin_rx.forward(sink);
        let write_stdout = stream.for_each(|message| {
            stdout.write_all(&message.into_data()).unwrap();
            Ok(())
        });

        // Wait for either of futures to complete.
        send_stdin.map(|_| ())
                  .select(write_stdout.map(|_| ()))
                  .then(|_| Ok(()))
    }).map_err(|e| {
        println!("Error during the websocket handshake occurred: {}", e);
        io::Error::new(io::ErrorKind::Other, e)
    });

    // And now that we've got our client, we execute it in the event loop!
    core.run(client).unwrap();
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
fn read_stdin(mut tx: mpsc::Sender<Message>) {
    let mut stdin = io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf) {
            Err(_) |
            Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);
        tx = tx.send(Message::binary(buf)).wait().unwrap();
    }
}
