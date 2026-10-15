use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{
    io::AsyncBufReadExt, io::AsyncWriteExt, io::BufReader, net::TcpListener, time::timeout,
};

use crate::StoredCredentials;

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
    /// Anti-CSRF `state` (RFC 6749 §10.12): sent in the authorize URL and checked on the
    /// callback, so a code injected by whatever else reaches the loopback port is rejected.
    pub state: String,
    /// PKCE verifier (RFC 7636): stays in Rust, only sent with the code exchange — a code
    /// intercepted on the redirect is useless without it.
    pub code_verifier: String,
}

impl LoopbackSession {
    /// S256 PKCE challenge derived from `code_verifier`, for the authorize URL.
    pub fn code_challenge(&self) -> String {
        pkce_challenge(&self.code_verifier)
    }
}

/// Bind a loopback TCP listener on the first available port. The caller must open the
/// authorization URL in a browser, then pass the session to [`wait_for_code`].
pub async fn bind_loopback() -> Result<LoopbackSession> {
    for &port in &REDIRECT_PORTS {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            return Ok(LoopbackSession {
                listener,
                redirect_uri: format!("http://localhost:{port}/callback"),
                state: crate::random_token(),
                // Two tokens → 64 hex chars, within RFC 7636's 43..=128 unreserved chars.
                code_verifier: format!("{}{}", crate::random_token(), crate::random_token()),
            });
        }
    }

    Err(anyhow!(
        "all OAuth redirect ports are in use ({REDIRECT_PORTS:?}); free one and retry"
    ))
}

/// Wait for the browser to redirect to our loopback server, check `state` and extract the
/// authorization code. Blocks until done or AUTH_TIMEOUT elapses.
pub async fn wait_for_code(session: &LoopbackSession) -> Result<String> {
    let (stream, _) = timeout(AUTH_TIMEOUT, session.listener.accept())
        .await
        .map_err(|_| anyhow!("OAuth timeout: authorization not completed within 5 minutes"))??;
    let mut reader = BufReader::new(stream);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    if query_param(&request_line, "state").as_deref() != Some(session.state.as_str()) {
        return Err(anyhow!("OAuth callback state mismatch — request rejected"));
    }
    let code =
        query_param(&request_line, "code").ok_or_else(|| anyhow!("OAuth callback missing code"))?;

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

/// Value of `name` in the query string of an HTTP request line, percent-decoded.
fn query_param(request_line: &str, name: &str) -> Option<String> {
    // "GET /callback?code=ABC&state=XYZ&locationid=2 HTTP/1.1"
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|p| {
        let (key, value) = p.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = (bytes[i] == b'%')
            .then(|| s.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match decoded {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// RFC 7636 S256: BASE64URL-ENCODE(SHA256(ASCII(code_verifier))), unpadded.
fn pkce_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
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

// --- Token endpoint (standard RFC 6749 responses: Dropbox, Google Drive) ---

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Present on a code exchange with offline access; usually absent on a refresh.
    refresh_token: Option<String>,
    expires_in: u64,
}

/// POST `form` to a standard OAuth token endpoint and map the response to credentials.
/// `previous_refresh_token` is kept when the response doesn't rotate it (refresh grant).
pub(crate) async fn request_token(
    token_url: &str,
    form: &[(&str, &str)],
    previous_refresh_token: Option<&str>,
) -> Result<StoredCredentials> {
    let resp: TokenResponse = reqwest::Client::new()
        .post(token_url)
        .form(form)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(to_credentials(resp, previous_refresh_token, unix_now()))
}

fn to_credentials(
    resp: TokenResponse,
    previous_refresh_token: Option<&str>,
    now: u64,
) -> StoredCredentials {
    StoredCredentials {
        access_token: resp.access_token,
        refresh_token: resp
            .refresh_token
            .or_else(|| previous_refresh_token.map(str::to_owned)),
        expires_at: Some(now + resp.expires_in),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_extracts_and_decodes() {
        let line = "GET /callback?code=AB%2FC&state=xyz&locationid=2 HTTP/1.1\r\n";
        assert_eq!(query_param(line, "code").as_deref(), Some("AB/C"));
        assert_eq!(query_param(line, "state").as_deref(), Some("xyz"));
        assert_eq!(query_param(line, "missing"), None);
    }

    #[test]
    fn query_param_does_not_match_key_prefix() {
        let line = "GET /callback?xcode=evil&code=good HTTP/1.1";
        assert_eq!(query_param(line, "code").as_deref(), Some("good"));
    }

    #[test]
    fn query_param_without_query_is_none() {
        assert_eq!(query_param("GET /callback HTTP/1.1", "code"), None);
        assert_eq!(query_param("", "code"), None);
    }

    #[test]
    fn percent_decode_handles_plus_and_invalid_escapes() {
        assert_eq!(percent_decode("a+b%20c"), "a b c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn percent_encode_round_trips() {
        let s = "http://localhost:53682/callback";
        assert_eq!(
            percent_encode(s),
            "http%3A%2F%2Flocalhost%3A53682%2Fcallback"
        );
        assert_eq!(percent_decode(&percent_encode(s)), s);
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_test_vector() {
        // RFC 7636 appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn to_credentials_computes_expiry_and_keeps_rotated_refresh_token() {
        let resp = TokenResponse {
            access_token: "a".into(),
            refresh_token: Some("new".into()),
            expires_in: 3600,
        };
        let creds = to_credentials(resp, Some("old"), 1_000);
        assert_eq!(creds.access_token, "a");
        assert_eq!(creds.refresh_token.as_deref(), Some("new"));
        assert_eq!(creds.expires_at, Some(4_600));
    }

    #[test]
    fn to_credentials_falls_back_to_previous_refresh_token() {
        let resp = TokenResponse {
            access_token: "a".into(),
            refresh_token: None,
            expires_in: 10,
        };
        let creds = to_credentials(resp, Some("old"), 0);
        assert_eq!(creds.refresh_token.as_deref(), Some("old"));
    }
}
