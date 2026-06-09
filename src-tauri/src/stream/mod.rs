// stream:// custom URI scheme handler.
// cf. ARCHITECTURE.md §4 — Rust generates AND consumes pCloud links (same IP = IP-binding solved).
//
// ⚠️  Verify `register_asynchronous_uri_scheme_protocol` exact signature and Range support
//     against the installed Tauri version before wiring (cf. ARCHITECTURE.md §13).
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
    let (username, password) = {
        let auth = state.auth.lock().unwrap();
        let creds = auth
            .credentials()
            .ok_or_else(|| anyhow::anyhow!("not authenticated"))?;
        (creds.username.clone(), creds.password.clone())
    };

    let client = crate::pcloud::PCloudClient::new(username, password)
        .map_err(|e| anyhow::anyhow!("failed to build client: {e}"))?;

    let pcloud_url = match (req.kind, req.transcoded) {
        (StreamKind::Video, true) => client.get_video_link(req.file_id).await?,
        (StreamKind::Video, false)
        | (StreamKind::Audio, _)
        | (StreamKind::Tab, _)
        | (StreamKind::Doc, _) => client.get_file_link(req.file_id).await?,
    };

    let range_header = request
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let resp = client
        .fetch_range(&pcloud_url, range_header.as_deref())
        .await?;

    let mut builder = tauri::http::Response::builder()
        .status(resp.status)
        .header("Content-Type", resp.content_type)
        .header("Accept-Ranges", "bytes");

    if let Some(cr) = resp.content_range {
        builder = builder.header("Content-Range", cr);
    }

    Ok(builder.body(resp.body)?)
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

/// Parse `stream://{kind}/{fileId}[?transcoded=true]`
fn parse_uri(uri: &str) -> Option<StreamRequest> {
    // strip scheme
    let rest = uri.strip_prefix("stream://")?;
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
