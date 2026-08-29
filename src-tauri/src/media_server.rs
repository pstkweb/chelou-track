// Linux-only loopback HTTP server for <video>/<audio> src.
//
// WebKitGTK's GStreamer media backend cannot load <video>/<audio> from a custom registered URI
// scheme: GStreamer reports GST_CORE_ERROR_MISSING_PLUGIN "no url handler for custom protocol"
// even though WebKit's own page-level resource loader (fetch/img/embed) handles the same scheme
// fine. Confirmed upstream, unfixed WebKit bug: https://bugs.webkit.org/show_bug.cgi?id=146351
// — a WebKit/GStreamer maintainer states fixing it would need a custom GStreamer source element
// hooked into WebKit's internal scheme registry, which is out of reach from application code.
//
// Plain http:// works (GStreamer's native souphttpsrc), so on Linux only, video and audio are
// served from this small loopback server instead of stream://. Windows (WebView2) and macOS
// (AVFoundation) don't have this GStreamer split and keep using stream:// for media too — see
// src/lib/stream.ts for the platform switch. tab:// and doc:// stay on stream:// everywhere,
// since WebKit's normal resource loader (not GStreamer) already handles those fine.
//
// This is a genuine streaming proxy, not fetch_range's buffer-one-chunk-then-respond model:
// GStreamer's souphttpsrc probes a brand new resource with a plain GET (no Range header) before
// it ever issues a Range request. Two buffered-chunk approaches were tried and both broke
// playback:
//   - Forward the rangeless GET as-is → we buffer the *entire* remote file (some lesson videos
//     are 1-2 GB) before replying; souphttpsrc gives up and aborts the connection long before
//     that finishes.
//   - Answer a rangeless GET with an artificially capped 206 (first 1 MB) → RFC 7233 only
//     allows a 206 in response to an explicit Range request; souphttpsrc apparently takes the
//     unsolicited 206 as "this is the whole resource", plays that 1 MB, then treats the source
//     as ended (confirmed: the frontend's mark_lesson_seen fires right after).
// Streaming the upstream response through as it arrives — 200 with the true Content-Length for
// a rangeless request, 206 with the true Content-Range only for an explicit Range request —
// gets first bytes to the client immediately (no buffering wait) without ever lying about what
// was requested.
#![cfg(target_os = "linux")]

use crate::commands::AppState;
use crate::stream::{resolve_download_target, StreamKind};
use chelou_providers::ProviderId;
use futures_util::TryStreamExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Bytes, Frame, Incoming};
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

type ResponseBody = BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

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
) -> Result<Response<ResponseBody>, std::convert::Infallible> {
    Ok(match handle_inner(&app, req).await {
        Ok(resp) => resp,
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(
                Full::new(Bytes::from(e.to_string()))
                    .map_err(Box::<dyn std::error::Error + Send + Sync>::from)
                    .boxed(),
            )
            .expect("static status/body always builds a valid response"),
    })
}

/// Parse `/{video,audio}/{provider}/{fileId}[?transcoded=true]`, resolve the provider download
/// URL, then stream the upstream response straight through — see the module doc for why this
/// can't be a buffered chunk like `stream://`'s `resolve_media`.
async fn handle_inner(
    app: &AppHandle,
    req: Request<Incoming>,
) -> anyhow::Result<Response<ResponseBody>> {
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

    let range_header = req
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let state = app.state::<AppState>();
    let download_target =
        resolve_download_target(state.clone(), provider, kind, file_id, transcoded).await?;

    let mut upstream = state.http.get(&download_target.url);
    for (key, value) in &download_target.headers {
        upstream = upstream.header(key, value);
    }
    if let Some(r) = &range_header {
        upstream = upstream.header("Range", r);
    }

    let upstream_resp = upstream.send().await?;
    let status = upstream_resp.status().as_u16();
    let content_type = upstream_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .cloned();
    let content_range = upstream_resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .cloned();
    let content_length = upstream_resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .cloned();

    let body_stream = upstream_resp
        .bytes_stream()
        .map_ok(Frame::data)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
    let body = StreamBody::new(body_stream).boxed();

    let mut builder = Response::builder()
        .status(status)
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*");
    if let Some(v) = content_type {
        builder = builder.header("Content-Type", v);
    }
    if let Some(v) = content_range {
        builder = builder.header("Content-Range", v);
    }
    if let Some(v) = content_length {
        builder = builder.header("Content-Length", v);
    }

    Ok(builder.body(body)?)
}
