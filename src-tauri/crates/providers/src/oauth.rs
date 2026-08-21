use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::{
    io::AsyncBufReadExt, io::AsyncWriteExt, io::BufReader, net::TcpListener, time::timeout,
};

// RFC 8252 OAuth 2.0 for Native Apps — loopback redirect flow.
// The system browser handles the full auth (Google, Apple, etc.) and redirects
// to our loopback server. Register all three URIs in the pCloud developer console:
//   http://localhost:53682/callback
//   http://localhost:53683/callback
//   http://localhost:53684/callback
const REDIRECT_PORTS: [u16; 3] = [53682, 53683, 53684];
const AUTH_TIMEOUT: Duration = Duration::from_secs(300);

pub struct LoopbackSession {
    listener: TcpListener,
    pub redirect_uri: String,
}

/// Bind a loopback TCP listener on the first available port and return
/// (authorization URL, listener). The caller must open the URL in a browser
/// and then pass the listener to [`wait_for_token`].
pub async fn bind_loopback() -> Result<LoopbackSession> {
    for &port in &REDIRECT_PORTS {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            return Ok(LoopbackSession {
                listener,
                redirect_uri: format!("http://localhost:{port}/callback"),
            });
        }
    }

    Err(anyhow!(
        "all OAuth redirect ports are in use ({REDIRECT_PORTS:?}); free one and retry"
    ))
}

/// Wait for the browser to redirect to our loopback server, extract the code,
/// exchange it for an access token. Blocks until done or AUTH_TIMEOUT elapses.
pub async fn wait_for_token(session: LoopbackSession) -> Result<String> {
    let (stream, _) = timeout(AUTH_TIMEOUT, session.listener.accept())
        .await
        .map_err(|_| anyhow!("OAuth timeout: authorization not completed within 5 minutes"))??;
    let mut reader = BufReader::new(stream);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    let code = extract_code(&request_line)
        .ok_or_else(|| anyhow!("OAuth callback missing code: {request_line}"))?;

    // Respond so the browser tab closes cleanly.
    let html = "<html><body style='font-family:sans-serif;padding:2rem'>\
        <h2>Connexion r\u{00e9}ussie \u{2713}</h2>\
        <p>Vous pouvez fermer cet onglet et revenir sur Chelou Track.</p>\
        </body></html>";

    let _ = reader
        .get_mut()
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                Content-Length: {}\r\nConnection: close\r\n\r\n{html}",
                html.len(),
            )
            .as_bytes(),
        )
        .await;

    Ok(code)
}

fn extract_code(request_line: &str) -> Option<String> {
    // "GET /callback?code=ABC&locationid=2 HTTP/1.1"
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    query
        .split('&')
        .find_map(|p| p.strip_prefix("code=").map(str::to_owned))
}

/// Percent-encode a string for use as a query-parameter value (RFC 3986).
pub fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
