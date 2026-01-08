#![cfg(feature = "proxy")]

use std::env;

use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{timeout, Duration},
};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};

#[tokio::test]
async fn proxy_http_and_socks5() {
    run_proxy_test("HTTP_PROXY", "http").await;
    run_proxy_test("ALL_PROXY", "socks5").await;
}

#[tokio::test]
async fn proxy_http_and_socks5_real() {
    let http_proxy = env::var("REAL_HTTP_PROXY").ok();
    let socks_proxy = env::var("REAL_SOCKS5_PROXY").ok();

    if let Some(http_proxy) = http_proxy {
        run_proxy_test_with_url("HTTP_PROXY", &http_proxy).await;
    }
    if let Some(socks_proxy) = socks_proxy {
        run_proxy_test_with_url("ALL_PROXY", &socks_proxy).await;
    }
}

async fn run_proxy_test(proxy_env_key: &str, proxy_scheme: &str) {
    let (target_port, target_handle) = spawn_ws_echo_server().await;
    let target_addr = format!("127.0.0.1:{target_port}");

    let (proxy_port, proxy_handle) = spawn_proxy(proxy_env_key, &target_addr).await;

    let prev_http_proxy = env::var("HTTP_PROXY").ok();
    let prev_https_proxy = env::var("HTTPS_PROXY").ok();
    let prev_all_proxy = env::var("ALL_PROXY").ok();
    let prev_no_proxy = env::var("NO_PROXY").ok();

    env::remove_var("HTTP_PROXY");
    env::remove_var("HTTPS_PROXY");
    env::remove_var("ALL_PROXY");
    env::remove_var("NO_PROXY");

    let proxy_url = format!("{proxy_scheme}://127.0.0.1:{proxy_port}");
    env::set_var(proxy_env_key, proxy_url);

    let url = format!("ws://{target_addr}");
    let (mut socket, _response) = connect_async(url).await.expect("proxy connect");
    socket.send(Message::Text("hello".into())).await.expect("send");
    let msg = socket.next().await.expect("read").expect("message");
    assert_eq!(msg, Message::Text("hello".into()));
    let _ = socket.close(None).await;

    restore_env("HTTP_PROXY", prev_http_proxy);
    restore_env("HTTPS_PROXY", prev_https_proxy);
    restore_env("ALL_PROXY", prev_all_proxy);
    restore_env("NO_PROXY", prev_no_proxy);

    proxy_handle.await.expect("proxy task");
    target_handle.await.expect("target task");
}

async fn run_proxy_test_with_url(proxy_env_key: &str, proxy_url: &str) {
    let (target_port, target_handle) = spawn_ws_echo_server().await;
    let target_addr = format!("127.0.0.1:{target_port}");

    let prev_http_proxy = env::var("HTTP_PROXY").ok();
    let prev_https_proxy = env::var("HTTPS_PROXY").ok();
    let prev_all_proxy = env::var("ALL_PROXY").ok();
    let prev_no_proxy = env::var("NO_PROXY").ok();

    env::remove_var("HTTP_PROXY");
    env::remove_var("HTTPS_PROXY");
    env::remove_var("ALL_PROXY");
    env::remove_var("NO_PROXY");

    env::set_var(proxy_env_key, proxy_url);

    let url = format!("ws://{target_addr}");
    let (mut socket, _response) = connect_async(url).await.expect("proxy connect");
    socket.send(Message::Text("hello".into())).await.expect("send");
    let msg = socket.next().await.expect("read").expect("message");
    assert_eq!(msg, Message::Text("hello".into()));
    let _ = socket.close(None).await;

    restore_env("HTTP_PROXY", prev_http_proxy);
    restore_env("HTTPS_PROXY", prev_https_proxy);
    restore_env("ALL_PROXY", prev_all_proxy);
    restore_env("NO_PROXY", prev_no_proxy);

    target_handle.await.expect("target task");
}

fn restore_env(key: &str, value: Option<String>) {
    match value {
        Some(value) => env::set_var(key, value),
        None => env::remove_var(key),
    }
}

async fn spawn_ws_echo_server() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind ws server");
    let port = listener.local_addr().expect("addr").port();
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut ws = accept_async(stream).await.expect("accept ws");
        if let Some(Ok(msg)) = ws.next().await {
            let _ = ws.send(msg).await;
        }
        let _ = ws.close(None).await;
    });
    (port, handle)
}

async fn spawn_proxy(
    proxy_env_key: &str,
    target_addr: &str,
) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind proxy");
    let port = listener.local_addr().expect("addr").port();
    let target_addr = target_addr.to_string();
    let proxy_env_key = proxy_env_key.to_string();
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        if proxy_env_key == "HTTP_PROXY" {
            handle_http_connect(stream, &target_addr).await;
        } else {
            handle_socks5(stream, &target_addr).await;
        }
    });
    (port, handle)
}

async fn handle_http_connect(mut client: TcpStream, target_addr: &str) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        let read = client.read(&mut chunk).await.expect("read");
        if read == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    if !std::str::from_utf8(&buf).unwrap_or("").starts_with("CONNECT") {
        return;
    }

    let mut upstream = TcpStream::connect(target_addr).await.expect("connect upstream");
    client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .expect("write");

    let _ = timeout(Duration::from_secs(2), io::copy_bidirectional(&mut client, &mut upstream)).await;
}

async fn handle_socks5(mut client: TcpStream, target_addr: &str) {
    let mut header = [0u8; 2];
    client.read_exact(&mut header).await.expect("read greeting");
    let methods_len = header[1] as usize;
    let mut methods = vec![0u8; methods_len];
    client.read_exact(&mut methods).await.expect("read methods");
    client.write_all(&[0x05, 0x00]).await.expect("write method");

    let mut req = [0u8; 4];
    client.read_exact(&mut req).await.expect("read request");
    if req[1] != 0x01 {
        return;
    }

    let _addr = match req[3] {
        0x01 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await.expect("read ip");
            std::net::Ipv4Addr::from(ip).to_string()
        }
        0x03 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await.expect("read len");
            let mut host = vec![0u8; len[0] as usize];
            client.read_exact(&mut host).await.expect("read host");
            String::from_utf8_lossy(&host).to_string()
        }
        0x04 => {
            let mut ip = [0u8; 16];
            client.read_exact(&mut ip).await.expect("read ip");
            std::net::Ipv6Addr::from(ip).to_string()
        }
        _ => return,
    };

    let mut port = [0u8; 2];
    client.read_exact(&mut port).await.expect("read port");
    let _port = u16::from_be_bytes(port);

    let mut upstream = TcpStream::connect(target_addr).await.expect("connect upstream");
    client
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .expect("write reply");

    let _ = timeout(Duration::from_secs(2), io::copy_bidirectional(&mut client, &mut upstream)).await;
}
