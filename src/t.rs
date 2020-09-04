use futures_util::{SinkExt, StreamExt};
use tungstenite::extensions::compression::CompressionConfig;
use tungstenite::extensions::deflate::DeflateConfig;
use tungstenite::protocol::WebSocketConfig;
use url::Url;

#[tokio::test]
async fn t() {
    let url = Url::parse("ws://127.0.0.1:9001").unwrap();
    let config = WebSocketConfig {
        max_send_queue: None,
        max_message_size: Some(64 << 20),
        max_frame_size: Some(16 << 20),
        compression_config: CompressionConfig::Deflate(DeflateConfig::default()),
    };

    let (stream, _response) = crate::connect_async_with_config(url, Some(config))
        .await
        .unwrap();
    let (mut sink, mut stream) = stream.split();
    sink.send("@sync(node:\"unit/foo\",lane:id)".into())
        .await
        .unwrap();

    if let Some(next) = stream.next().await {
        println!("{:?}", next);
    }

    if let Some(next) = stream.next().await {
        println!("{:?}", next);
    }

    if let Some(next) = stream.next().await {
        println!("{:?}", next);
    }
}
