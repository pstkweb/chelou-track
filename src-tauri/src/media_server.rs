// Linux-only loopback HTTP server for <video>/<audio> src.
//
// WebKitGTK's GStreamer media backend cannot load <video>/<audio> from a custom registered URI
// scheme: GStreamer reports GST_CORE_ERROR_MISSING_PLUGIN "no url handler for custom protocol"
// even though WebKit's own page-level resource loader (fetch/img/embed) handles the same scheme
// fine. Confirmed upstream, unfixed WebKit bug: https://bugs.webkit.org/show_bug.cgi?id=146351
// — a WebKit/GStreamer maintainer states fixing it would need a custom GStreamer source element
// hooked into WebKit's internal scheme registry, which is out of reach from application code.
#![cfg(target_os = "linux")]

use crate::commands::AppState;
use crate::stream::{resolve_media, StreamKind};
use chelou_providers::ProviderId;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use tauri::{AppHandle, Manager};
use tokio::net::TcpListener;

/// Fixed loopback port. Keep in sync with MEDIA_PORT in src/lib/stream.ts.
/// Fixed (not OS-assigned) because the frontend builds `<video src>`/`<audio src>` synchronously,
/// with no async round-trip to ask Rust which port got picked.
const PORT: u16 = 47812;

/// Bind the loopback server and accept connections for the lifetime of the app.
/// Failures (bind or per-connection) are logged to stderr and don't crash the app — video/audio
/// just won't load, same as if the network were down.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("media_server: failed to bind {addr}: {e}");
                return;
            }
        };

        loop {
            let (stream, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(e) => {
                    eprintln!("media_server: accept failed: {e}");
                    continue;
                }
            };

            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let io = TokioIo::new(stream);
                let service = service_fn(move |req| handle(app.clone(), req));
                if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                    eprintln!("media_server: connection error: {e}");
                }
            });
        }
    });
}

async fn handle(
    app: AppHandle,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    Ok(match handle_inner(&app, req).await {
        Ok(resp) => resp,
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::new(Bytes::from(e.to_string())))
            .expect("static status/body always builds a valid response"),
    })
}

/// Parse `/{video,audio}/{provider}/{fileId}[?transcoded=true]` — the path form matches
/// stream.ts's mediaBase(), just without the `stream://localhost` prefix.
async fn handle_inner(
    app: &AppHandle,
    req: Request<Incoming>,
) -> anyhow::Result<Response<Full<Bytes>>> {
    let path = req.uri().path().to_owned();
    let mut parts = path.trim_start_matches('/').splitn(3, '/');
    let kind_str = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("bad media path: {path}"))?;
    let provider_str = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("bad media path: {path}"))?;
    let file_id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("bad media path: {path}"))?
        .to_string();

    let provider: ProviderId = provider_str
        .parse()
        .map_err(|_| anyhow::anyhow!("bad provider in media path: {provider_str}"))?;
    let kind = match kind_str {
        "video" => StreamKind::Video,
        "audio" => StreamKind::Audio,
        _ => anyhow::bail!("unsupported media kind: {kind_str}"),
    };
    let transcoded = req
        .uri()
        .query()
        .is_some_and(|q| q.contains("transcoded=true"));

    let range_header = req.headers().get("Range").and_then(|v| v.to_str().ok());

    let state = app.state::<AppState>();
    let resp = resolve_media(state, provider, kind, file_id, transcoded, range_header).await?;

    let mut builder = Response::builder()
        .status(resp.status)
        .header("Content-Type", resp.content_type)
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*");

    if let Some(cr) = resp.content_range {
        builder = builder.header("Content-Range", cr);
    }

    Ok(builder.body(Full::new(Bytes::from(resp.body)))?)
}
