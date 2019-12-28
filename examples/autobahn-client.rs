use futures::{SinkExt, StreamExt};
use log::*;
use tokio_tungstenite::{connect_async, tungstenite::Result};
use url::Url;

const AGENT: &str = "Tungstenite";

async fn get_case_count() -> Result<u32> {
    let (mut socket, _) =
        connect_async(Url::parse("ws://localhost:9001/getCaseCount").unwrap()).await?;
    let msg = socket.next().await.unwrap()?;
    socket.close(None).await?;
    Ok(msg.into_text()?.parse::<u32>().unwrap())
}

async fn update_reports() -> Result<()> {
    let (mut socket, _) = connect_async(
        Url::parse(&format!(
            "ws://localhost:9001/updateReports?agent={}",
            AGENT
        ))
        .unwrap(),
    )
    .await?;
    socket.close(None).await?;
    Ok(())
}

async fn run_test(case: u32) {
    info!("Running test case {}", case);
    let case_url = Url::parse(&format!(
        "ws://localhost:9001/runCase?case={}&agent={}",
        case, AGENT
    ))
    .unwrap();

    let (mut ws_stream, _) = connect_async(case_url).await.expect("Connect error");
    while let Some(msg) = ws_stream.next().await {
        let msg = msg.expect("Failed to get message");
        if msg.is_text() || msg.is_binary() {
            ws_stream.send(msg).await.expect("Write error");
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let total = get_case_count().await.unwrap();

    for case in 1..=total {
        run_test(case).await
    }

    update_reports().await.unwrap();
}
