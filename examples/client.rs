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

use std::env;
use std::io::{self, Write};

use futures::{SinkExt, StreamExt};
use log::*;
use tungstenite::protocol::Message;

use tokio::io::AsyncReadExt;
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() {
    let _ = env_logger::try_init();

    // Specify the server address to which the client will be connecting.
    let connect_addr = env::args()
        .nth(1)
        .unwrap_or_else(|| panic!("this program requires at least one argument"));

    let url = url::Url::parse(&connect_addr).unwrap();

    // Right now Tokio doesn't support a handle to stdin running on the event
    // loop, so we farm out that work to a separate thread. This thread will
    // read data from stdin and then send it to the event loop over a standard
    // futures channel.
    let (stdin_tx, mut stdin_rx) = futures::channel::mpsc::unbounded();
    tokio::spawn(read_stdin(stdin_tx));

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
    let (mut ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    info!("WebSocket handshake has been successfully completed");

    while let Some(msg) = stdin_rx.next().await {
        ws_stream.send(msg).await.expect("Failed to send request");
        if let Some(msg) = ws_stream.next().await {
            let msg = msg.expect("Failed to get response");
            stdout.write_all(&msg.into_data()).unwrap();
        }
    }
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
async fn read_stdin(tx: futures::channel::mpsc::UnboundedSender<Message>) {
    let mut stdin = tokio::io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf).await {
            Err(_) | Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);
        tx.unbounded_send(Message::binary(buf)).unwrap();
    }
}
