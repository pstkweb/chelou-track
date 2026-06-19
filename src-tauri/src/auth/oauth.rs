// RFC 8252 OAuth 2.0 for Native Apps — loopback redirect flow.
// The system browser handles the full auth (Google, Apple, etc.) and redirects
// to our loopback server. Register all three URIs in the pCloud developer console:
//   http://localhost:53682/callback
//   http://localhost:53683/callback
//   http://localhost:53684/callback
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::time::timeout;

const AUTH_URL: &str = "https://my.pcloud.com/oauth2/authorize";
const TOKEN_URL: &str = "https://eapi.pcloud.com/oauth2_token";
const REDIRECT_PORTS: [u16; 3] = [53682, 53683, 53684];
const AUTH_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
struct TokenResponse {
    /// pCloud convention: 0 = success, non-zero = error.
    result: Option<i32>,
    access_token: Option<String>,
    error: Option<String>,
}

/// Bind a loopback TCP listener on the first available port and return
/// (authorization URL, listener). The caller must open the URL in a browser
/// and then pass the listener to [`wait_for_token`].
pub async fn start_loopback_server(client_id: &str) -> Result<(String, TcpListener)> {
    let mut last_err = anyhow!("no ports available");
    for &port in &REDIRECT_PORTS {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                let redirect_uri = format!("http://localhost:{port}/callback");
                let auth_url = format!(
                    "{AUTH_URL}?client_id={client_id}&response_type=code&redirect_uri={}",
                    percent_encode(&redirect_uri),
                );
                return Ok((auth_url, listener));
            }
            Err(e) => last_err = e.into(),
        }
    }
    Err(last_err.context(format!(
        "all OAuth redirect ports are in use ({REDIRECT_PORTS:?}); free one and retry",
    )))
}

/// Wait for the browser to redirect to our loopback server, extract the code,
/// exchange it for an access token. Blocks until done or AUTH_TIMEOUT elapses.
pub async fn wait_for_token(
    listener: TcpListener,
    client_id: &str,
    client_secret: &str,
) -> Result<String> {
    let port = listener.local_addr()?.port();

    let (stream, _) = timeout(AUTH_TIMEOUT, listener.accept())
        .await
        .map_err(|_| anyhow!("OAuth timeout: authorization not completed within 5 minutes"))??;

    let mut reader = BufReader::new(stream);

    // Only the first line is needed: "GET /callback?code=XYZ HTTP/1.1"
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

    let redirect_uri = format!("http://localhost:{port}/callback");
    exchange_code(&code, &redirect_uri, client_id, client_secret).await
}

fn extract_code(request_line: &str) -> Option<String> {
    // "GET /callback?code=ABC&locationid=2 HTTP/1.1"
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    query
        .split('&')
        .find_map(|p| p.strip_prefix("code=").map(str::to_owned))
}

async fn exchange_code(
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<String> {
    let resp: TokenResponse = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if resp.result.is_some_and(|r| r != 0) {
        return Err(anyhow!(
            "pCloud token exchange error: {}",
            resp.error.unwrap_or_default()
        ));
    }
    resp.access_token
        .ok_or_else(|| anyhow!("no access_token in pCloud response"))
}

/// Percent-encode a string for use as a query-parameter value (RFC 3986).
fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
