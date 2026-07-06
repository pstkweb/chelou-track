// stream:// custom URI scheme handler.
// cf. ARCHITECTURE.md §4 — Rust generates AND consumes pCloud links (same IP = IP-binding solved).
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::commands::AppState;

// Tauri v2 passes UriSchemeContext as the first arg, not AppHandle directly.
// We clone the AppHandle out of it before spawning so the async block is 'static.
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        match handle_inner(&app, &request).await {
            Ok(response) => responder.respond(response),
            Err(e) => {
                let body = e.to_string().into_bytes();
                responder.respond(
                    tauri::http::Response::builder()
                        .status(500)
                        .header("Access-Control-Allow-Origin", "*")
                        .body(body)
                        .unwrap(),
                );
            }
        }
    });
}

async fn handle_inner<R: Runtime>(
    app: &AppHandle<R>,
    request: &tauri::http::Request<Vec<u8>>,
) -> anyhow::Result<tauri::http::Response<Vec<u8>>> {
    let uri = request.uri().to_string();
    let req = parse_uri(&uri).ok_or_else(|| anyhow::anyhow!("invalid stream URI: {uri}"))?;

    let state = app.state::<AppState>();
    let token = {
        let auth = state.auth.lock().unwrap();
        auth.token()
            .ok_or_else(|| anyhow::anyhow!("not authenticated"))?
            .to_owned()
    };

    let client = crate::pcloud::PCloudClient::new(token)
        .map_err(|e| anyhow::anyhow!("failed to build client: {e}"))?;

    let use_video_link = matches!(req.kind, StreamKind::Video) && req.transcoded;
    let pcloud_url = resolve_url(&state, req.file_id, use_video_link, &client).await?;

    let range_header = request
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    // Cap open-ended or oversized ranges so we never buffer more than 1 MB at once.
    // The browser sees a 206 + Content-Range and issues follow-up requests automatically.
    const CHUNK: u64 = 1024 * 1024;
    let effective_range = cap_range(range_header.as_deref(), CHUNK).or(range_header);

    let resp = client
        .fetch_range(&pcloud_url, effective_range.as_deref())
        .await?;

    let mut builder = tauri::http::Response::builder()
        .status(resp.status)
        .header("Content-Type", resp.content_type)
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*");

    if let Some(cr) = resp.content_range {
        builder = builder.header("Content-Range", cr);
    }

    Ok(builder.body(resp.body)?)
}

// --- URL cache ---

const URL_TTL: Duration = Duration::from_secs(600); // pCloud links are valid ~15 min; we refresh at 10

/// Return a cached pCloud download URL for `file_id`, refreshing if older than URL_TTL.
/// Eliminates one getfilelink API round-trip per Range chunk during media playback.
async fn resolve_url(
    state: &AppState,
    file_id: u64,
    use_video_link: bool,
    client: &crate::pcloud::PCloudClient,
) -> anyhow::Result<String> {
    // Check cache — drop the lock before any await.
    let cached = {
        let cache = state.url_cache.lock().unwrap();
        cache
            .get(&(file_id, use_video_link))
            .filter(|(_, fetched_at)| fetched_at.elapsed() < URL_TTL)
            .map(|(url, _)| url.clone())
    };
    if let Some(url) = cached {
        return Ok(url);
    }
    let url = if use_video_link {
        client.get_video_link(file_id).await?
    } else {
        client.get_file_link(file_id).await?
    };
    state.url_cache.lock().unwrap().insert(
        (file_id, use_video_link),
        (url.clone(), std::time::Instant::now()),
    );
    Ok(url)
}

// --- Range capping ---

/// Cap a Range header to at most `chunk` bytes.
/// - `bytes=X-`    → `bytes=X-{X+chunk-1}`  (open-ended → first chunk)
/// - `bytes=X-Y`   → kept as-is if ≤ chunk, capped otherwise
///
/// Returns `None` if the header is absent or unparseable (caller falls back to original).
fn cap_range(range: Option<&str>, chunk: u64) -> Option<String> {
    let r = range?;
    let rest = r.strip_prefix("bytes=")?;
    let (start_str, end_str) = rest.split_once('-')?;
    let start: u64 = start_str.trim().parse().ok()?;
    let capped_end = start.saturating_add(chunk - 1);

    if end_str.is_empty() {
        return Some(format!("bytes={start}-{capped_end}"));
    }
    let end: u64 = end_str.trim().parse().ok()?;
    if end.saturating_sub(start).saturating_add(1) > chunk {
        Some(format!("bytes={start}-{capped_end}"))
    } else {
        Some(r.to_owned())
    }
}

// --- URI parsing ---

pub struct StreamRequest {
    pub kind: StreamKind,
    pub file_id: u64,
    pub transcoded: bool,
}

pub enum StreamKind {
    Video,
    Audio,
    Tab,
    Doc,
}

/// Parse `stream://[localhost/]{kind}/{fileId}[?transcoded=true]`
/// Tauri normalises the Windows `http://stream.localhost/…` form to
/// `stream://localhost/…` before calling the handler.
fn parse_uri(uri: &str) -> Option<StreamRequest> {
    let rest = uri.strip_prefix("stream://")?;
    // drop optional `localhost/` host injected by Tauri on Windows
    let rest = rest.strip_prefix("localhost/").unwrap_or(rest);
    let (path, query) = rest.split_once('?').map_or((rest, ""), |p| p);
    let mut parts = path.splitn(2, '/');
    let kind_str = parts.next()?;
    let file_id: u64 = parts.next()?.parse().ok()?;
    let transcoded = query.contains("transcoded=true");
    let kind = match kind_str {
        "video" => StreamKind::Video,
        "audio" => StreamKind::Audio,
        "tab" => StreamKind::Tab,
        "doc" => StreamKind::Doc,
        _ => return None,
    };
    Some(StreamRequest {
        kind,
        file_id,
        transcoded,
    })
}
