//! A simple example of connecting to a websocket server to send and receive messages.
//!
//! You MUST edit the code to replace "<ws://websocket_url>" with the url of
//! the websocket you want to connect to.
//!
//! You should also edit the message to the format expected by the websocket
//!

use futures_util::{ StreamExt, SinkExt };
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::connect_async;

#[tokio::main]
async fn main() {
    // url of websocket to connect to
    // EDIT THIS to the url of the websocket you want to connect to
    // e.g "ws://localhost:3000/socket"
    let url = "ws://<websocket_url>";

    // connect to websocket
    let (mut socket, _) = connect_async(url).await.expect("Failed to connect to websocket.\n");

    println!("Successfully connected to the WebSocket");

    // create message
    // edit this to the message you want to send
    let message = Message::from(r#"{"msg": "hello"}"#);

    // send message to websocket
    if let Err(e) = socket.send(message).await {
        eprintln!("Error sending message: {:?}", e);
    }

    // recieve and print the response
    if let Some(Ok(response)) = socket.next().await {
        println!("{response}");
    }
}
