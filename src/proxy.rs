//! Proxy support for async client connections.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use tungstenite::{
    error::{Error, UrlError},
    proxy::{build_http_connect_request, parse_http_connect_response, ProxyAuth, ProxyConfig},
    Result,
};

const MAX_CONNECT_RESPONSE_SIZE: usize = 8192;

/// Establish a proxy tunnel over an async stream before WebSocket handshake.
pub async fn connect_via_proxy<S>(
    mut stream: S,
    proxy: &ProxyConfig,
    host: &str,
    port: u16,
) -> Result<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match proxy.scheme {
        tungstenite::proxy::ProxyScheme::Http => {
            http_connect(&mut stream, host, port, proxy.auth.as_ref()).await?;
        }
        tungstenite::proxy::ProxyScheme::Socks5 | tungstenite::proxy::ProxyScheme::Socks5h => {
            socks5_handshake(&mut stream, host, port, proxy.auth.as_ref()).await?;
        }
    }

    Ok(stream)
}

async fn http_connect<S>(
    stream: &mut S,
    host: &str,
    port: u16,
    auth: Option<&ProxyAuth>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let authority = format!("{host}:{port}");
    let request = build_http_connect_request(&authority, auth)?;
    stream.write_all(&request).await.map_err(Error::Io)?;
    stream.flush().await.map_err(Error::Io)?;

    let response = read_connect_response(stream).await?;
    let status = parse_http_connect_response(&response)?;
    if !(200..300).contains(&status) {
        return Err(Error::Url(UrlError::ProxyConnect(format!(
            "HTTP CONNECT failed with status {status}"
        ))));
    }

    Ok(())
}

async fn read_connect_response<S>(stream: &mut S) -> Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        if buf.len() >= MAX_CONNECT_RESPONSE_SIZE {
            return Err(Error::Url(UrlError::ProxyConnect(
                "HTTP CONNECT response too large".into(),
            )));
        }

        let read = stream.read(&mut chunk).await.map_err(Error::Io)?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    Ok(buf)
}

async fn socks5_handshake<S>(
    stream: &mut S,
    host: &str,
    port: u16,
    auth: Option<&ProxyAuth>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut methods = vec![0x00];
    if auth.is_some() {
        methods.push(0x02);
    }

    stream.write_all(&[0x05, methods.len() as u8]).await.map_err(Error::Io)?;
    stream.write_all(&methods).await.map_err(Error::Io)?;
    stream.flush().await.map_err(Error::Io)?;

    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await.map_err(Error::Io)?;
    if choice[0] != 0x05 {
        return Err(Error::Url(UrlError::ProxyConnect(
            "SOCKS5: invalid response version".into(),
        )));
    }

    match choice[1] {
        0x00 => {}
        0x02 => {
            let auth = auth.ok_or_else(|| {
                Error::Url(UrlError::ProxyConnect(
                    "SOCKS5: proxy requested auth, but none provided".into(),
                ))
            })?;
            socks5_userpass_auth(stream, auth).await?;
        }
        0xFF => {
            return Err(Error::Url(UrlError::ProxyConnect(
                "SOCKS5: no acceptable authentication method".into(),
            )));
        }
        _ => {
            return Err(Error::Url(UrlError::ProxyConnect(
                "SOCKS5: unsupported authentication method".into(),
            )));
        }
    }

    send_socks5_connect(stream, host, port).await?;
    Ok(())
}

async fn socks5_userpass_auth<S>(stream: &mut S, auth: &ProxyAuth) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let username = auth.username.as_bytes();
    let password = auth.password.as_bytes();

    if username.len() > u8::MAX as usize || password.len() > u8::MAX as usize {
        return Err(Error::Url(UrlError::ProxyConnect(
            "SOCKS5 auth credentials too long".into(),
        )));
    }

    let mut buf = Vec::with_capacity(3 + username.len() + password.len());
    buf.push(0x01);
    buf.push(username.len() as u8);
    buf.extend_from_slice(username);
    buf.push(password.len() as u8);
    buf.extend_from_slice(password);

    stream.write_all(&buf).await.map_err(Error::Io)?;
    stream.flush().await.map_err(Error::Io)?;

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.map_err(Error::Io)?;
    if response[0] != 0x01 || response[1] != 0x00 {
        return Err(Error::Url(UrlError::ProxyConnect(
            "SOCKS5 authentication failed".into(),
        )));
    }

    Ok(())
}

async fn send_socks5_connect<S>(stream: &mut S, host: &str, port: u16) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut request = Vec::new();
    request.push(0x05);
    request.push(0x01);
    request.push(0x00);

    if let Ok(addr) = host.parse::<std::net::Ipv4Addr>() {
        request.push(0x01);
        request.extend_from_slice(&addr.octets());
    } else if let Ok(addr) = host.parse::<std::net::Ipv6Addr>() {
        request.push(0x04);
        request.extend_from_slice(&addr.octets());
    } else {
        let host_bytes = host.as_bytes();
        if host_bytes.len() > u8::MAX as usize {
            return Err(Error::Url(UrlError::ProxyConnect(
                "SOCKS5 domain name too long".into(),
            )));
        }
        request.push(0x03);
        request.push(host_bytes.len() as u8);
        request.extend_from_slice(host_bytes);
    }

    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await.map_err(Error::Io)?;
    stream.flush().await.map_err(Error::Io)?;

    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await.map_err(Error::Io)?;
    if header[0] != 0x05 {
        return Err(Error::Url(UrlError::ProxyConnect(
            "SOCKS5: invalid response version".into(),
        )));
    }

    if header[1] != 0x00 {
        return Err(Error::Url(UrlError::ProxyConnect(format!(
            "SOCKS5: connection failed with code {}",
            header[1]
        ))));
    }

    let addr_len = match header[3] {
        0x01 => 4,
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await.map_err(Error::Io)?;
            len[0] as usize
        }
        0x04 => 16,
        _ => {
            return Err(Error::Url(UrlError::ProxyConnect(
                "SOCKS5: invalid address type".into(),
            )))
        }
    };

    let mut discard = vec![0u8; addr_len + 2];
    stream.read_exact(&mut discard).await.map_err(Error::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::connect_via_proxy;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};
    use tungstenite::proxy::{ProxyAuth, ProxyConfig, ProxyScheme};

    #[tokio::test]
    async fn http_connect_handshake() {
        let (client, mut server) = duplex(1024);
        let proxy = ProxyConfig {
            scheme: ProxyScheme::Http,
            host: "proxy.local".into(),
            port: 3128,
            auth: Some(ProxyAuth { username: "user".into(), password: "pass".into() }),
        };

        let client_task = tokio::spawn(async move {
            connect_via_proxy(client, &proxy, "example.com", 443).await.unwrap();
        });

        let mut buf = vec![0u8; 256];
        let n = server.read(&mut buf).await.unwrap();
        let request = &buf[..n];
        assert!(std::str::from_utf8(request).unwrap().contains("CONNECT example.com:443 HTTP/1.1"));
        assert!(std::str::from_utf8(request).unwrap().contains("Proxy-Authorization: Basic dXNlcjpwYXNz"));

        server.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.unwrap();
        client_task.await.unwrap();
    }

    #[tokio::test]
    async fn socks5_handshake() {
        let (client, mut server) = duplex(1024);
        let proxy = ProxyConfig {
            scheme: ProxyScheme::Socks5,
            host: "proxy.local".into(),
            port: 1080,
            auth: None,
        };

        let client_task = tokio::spawn(async move {
            connect_via_proxy(client, &proxy, "example.com", 443).await.unwrap();
        });

        let mut buf = [0u8; 16];
        let n = server.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], &[0x05, 0x01, 0x00]);

        server.write_all(&[0x05, 0x00]).await.unwrap();

        let mut connect_buf = [0u8; 64];
        let _n = server.read(&mut connect_buf).await.unwrap();
        assert_eq!(connect_buf[0], 0x05);
        assert_eq!(connect_buf[1], 0x01);
        assert_eq!(connect_buf[2], 0x00);

        server.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await.unwrap();
        client_task.await.unwrap();
    }
}
