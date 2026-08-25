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
    let html = r#"<!doctype html><html lang="fr"><meta charset="utf-8"><title>Connexion réussie — Chelou Track</title><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin><link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@700&family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap" rel="stylesheet"><style>
:root{--bg:#0a0b0d;--card:#101216;--line:rgba(255,255,255,.08);--fg:#e7eaee;--fg2:#8b939d;--fg3:#565d66;--accent:#c8f24a}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;background:var(--bg);color:var(--fg);font:400 15px/1.6 "IBM Plex Sans",system-ui,sans-serif;-webkit-font-smoothing:antialiased}
.c{width:min(400px,100%);background:var(--card);border:1px solid var(--line);border-radius:5px;box-shadow:0 12px 36px rgba(0,0,0,.6);padding:34px 30px;text-align:center}
.b{width:52px;height:52px;margin:0 auto 20px;border-radius:50%;display:grid;place-items:center;background:var(--accent);color:#0a0b0d}
h1{font:700 25px/1.1 "Space Grotesk",system-ui,sans-serif;letter-spacing:-.01em;margin:0 0 8px}
p{color:var(--fg2);margin:0;font-size:13.5px}
.k{display:block;margin-top:24px;padding-top:16px;border-top:1px solid var(--line);color:var(--fg3);font:400 11px/1 "IBM Plex Mono",ui-monospace,monospace;letter-spacing:.08em;text-transform:uppercase}
</style><div class="c"><div class="b"><svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg></div><h1>Connexion réussie</h1><p>Tu peux fermer cet onglet et revenir sur Chelou&nbsp;Track.</p><span class="k">Chelou Track · OAuth 2.0</span></div></html>"#;

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
