#[macro_use] extern crate log;
extern crate env_logger;
extern crate futures;
extern crate tokio;
extern crate tokio_tungstenite;
extern crate url;

use url::Url;
use futures::{Future, Stream};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        connect,
        Result,
        Error as WsError,
    },
};

const AGENT: &'static str = "Tungstenite";

fn get_case_count() -> Result<u32> {
    let (mut socket, _) = connect(
        Url::parse("ws://localhost:9001/getCaseCount").unwrap(),
    )?;
    let msg = socket.read_message()?;
    socket.close(None)?;
    Ok(msg.into_text()?.parse::<u32>().unwrap())
}

fn update_reports() -> Result<()> {
    let (mut socket, _) = connect(
        Url::parse(&format!("ws://localhost:9001/updateReports?agent={}", AGENT)).unwrap(),
    )?;
    socket.close(None)?;
    Ok(())
}

fn run_test(case: u32) {
    info!("Running test case {}", case);
    let case_url = Url::parse(
        &format!("ws://localhost:9001/runCase?case={}&agent={}", case, AGENT)
    ).unwrap();

    let job = connect_async(case_url)
        .map_err(|err| error!("Connect error: {}", err))
        .and_then(|(ws_stream, _)| {
            let (sink, stream) = ws_stream.split();
            stream
                .filter(|msg| msg.is_text() || msg.is_binary())
                .forward(sink)
                .and_then(|(_stream, _sink)| Ok(()))
                .map_err(|err| {
                    match err {
                        WsError::ConnectionClosed => (),
                        err => info!("WS error {}", err),
                    }
                })
        });

    tokio::run(job)
}

fn main() {
    env_logger::init();

    let total = get_case_count().unwrap();

    for case in 1..(total + 1) {
        run_test(case)
    }

    update_reports().unwrap();
}
