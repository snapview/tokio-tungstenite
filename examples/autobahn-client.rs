use futures_util::{SinkExt, StreamExt};
use log::*;
use tokio_tungstenite::{tungstenite::Error, tungstenite::Result, connect_async_with_config, connect_async};
use url::Url;
use tokio_tungstenite::tungstenite::extensions::deflate::{DeflateConfigBuilder, DeflateExt};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

const AGENT: &str = "Tungstenite";

async fn get_case_count() -> Result<u32> {
    let (mut socket, _) = connect_async(
        Url::parse("ws://localhost:9001/getCaseCount").expect("Can't connect to case count URL"),
    )
    .await?;
    let msg = socket.next().await.expect("Can't fetch case count")?;
    socket.close(None).await?;
    Ok(msg
        .into_text()?
        .parse::<u32>()
        .expect("Can't parse case count"))
}

async fn update_reports() -> Result<()> {
    let (mut socket, _) = connect_async(
        Url::parse(&format!(
            "ws://localhost:9001/updateReports?agent={}",
            AGENT
        ))
        .expect("Can't update reports"),
    )
    .await?;
    socket.close(None).await?;
    Ok(())
}

async fn run_test(case: u32) -> Result<()> {
    info!("Running test case {}", case);
    let case_url = Url::parse(&format!(
        "ws://localhost:9001/runCase?case={}&agent={}",
        case, AGENT
    ))
    .expect("Bad testcase URL");
    let deflate_config = DeflateConfigBuilder::default()
        .max_message_size(None)
        .build();

    let (mut ws_stream, _) = connect_async_with_config(case_url, Some(
        WebSocketConfig {
            max_send_queue: None,
            max_frame_size: Some(16 << 20),
            encoder: DeflateExt::new(deflate_config),
        }
    )).await?;
    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        if msg.is_text() || msg.is_binary() {
            ws_stream.send(msg).await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let total = get_case_count().await.expect("Error getting case count");

    for case in 1..=total {
        if let Err(e) = run_test(case).await {
            match e {
                Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
                err => error!("Testcase failed: {}", err),
            }
        }
    }

    update_reports().await.expect("Error updating reports");
}
