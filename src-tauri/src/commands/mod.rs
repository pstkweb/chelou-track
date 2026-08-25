// Tauri commands — the only surface the frontend can call via invoke().
// Token never leaves Rust (cf. ARCHITECTURE.md §3 + §5).
use anyhow::{anyhow, Result};
use chelou_providers::oauth::{bind_loopback, wait_for_token};
use chelou_providers::{Entry, ProviderAuth, ProviderId, StorageProvider, StoredCredentials};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};

// Client credentials baked at compile time.
//
// LOCAL: put them in src-tauri/.cargo/config.toml (gitignored) — Cargo picks them
// up automatically, no manual export needed:
//
//   [env]
//   PCLOUD_CLIENT_ID   = "your-id"
//   PCLOUD_CLIENT_SECRET = "your-secret"
//
// RELEASE CI: inject via GitHub Actions secrets in the release workflow only.
//
// option_env! (instead of env!) lets clippy/test CI compile without the variables;
// an empty string causes a graceful auth failure at runtime, not a build failure.
const PCLOUD_CLIENT_ID: &str = match option_env!("PCLOUD_CLIENT_ID") {
    Some(v) => v,
    None => "",
};
const PCLOUD_CLIENT_SECRET: &str = match option_env!("PCLOUD_CLIENT_SECRET") {
    Some(v) => v,
    None => "",
};
const DROPBOX_CLIENT_ID: &str = match option_env!("DROPBOX_CLIENT_ID") {
    Some(v) => v,
    None => "",
};
const DROPBOX_CLIENT_SECRET: &str = match option_env!("DROPBOX_CLIENT_SECRET") {
    Some(v) => v,
    None => "",
};

use crate::auth::AuthStore;
use crate::manifest::{ManifestStore, Method};
use crate::providers::{make_auth, make_client};

type UrlCacheKey = (ProviderId, String, bool);
type UrlCacheEntry = (String, Instant);

pub struct AppState {
    pub auth: Mutex<AuthStore>,
    pub manifest: ManifestStore,
    pub http: reqwest::Client,
    /// Cache of pCloud download URLs: (file_id, is_video_link) → (url, fetched_at).
    /// Avoids one getfilelink API round-trip per Range chunk during media playback.
    pub url_cache: Mutex<HashMap<UrlCacheKey, UrlCacheEntry>>,
}

// --- Auth ---

/// Start the pCloud OAuth2 authorization code flow (RFC 8252 loopback redirect).
/// Opens the system browser with the pCloud authorization page, then waits in the
/// background for the callback. On success emits `oauth:complete`; on error `oauth:error`.
#[tauri::command]
pub async fn oauth_start(app: tauri::AppHandle, provider: ProviderId) -> Result<(), String> {
    let (client_id, client_secret) = client_credentials(provider);
    let session = bind_loopback().await.map_err(|e| e.to_string())?;
    let auth_url = make_auth(provider).authorize_url(client_id, &session.redirect_uri);

    open::that(&auth_url).map_err(|e| format!("failed to open browser: {e}"))?;

    // Block until the user completes auth in the browser or the 5-minute timeout elapses.
    let redirect_uri = session.redirect_uri.clone();
    let token = wait_for_token(session).await.map_err(|e| e.to_string());

    let credentials = make_auth(provider)
        .exchange_code(
            token.as_ref().map_err(|e| e.to_string())?,
            &redirect_uri,
            client_id,
            client_secret,
        )
        .await
        .map_err(|e| e.to_string())?;

    app.state::<AppState>()
        .auth
        .lock()
        .unwrap()
        .save(provider, credentials)
        .map_err(|e| e.to_string())?;

    let _ = app.emit("oauth:complete", ());

    Ok(())
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    state
        .auth
        .lock()
        .unwrap()
        .clear()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_auth_status(state: State<'_, AppState>) -> Result<Option<ProviderId>, String> {
    Ok(state.auth.lock().unwrap().active_provider())
}

// --- pCloud folder browsing ---

#[tauri::command]
pub async fn list_folder(
    state: State<'_, AppState>,
    provider: ProviderId,
    folder_id: String,
) -> Result<Vec<Entry>, String> {
    let credentials = get_or_refresh_credentials(state)
        .await
        .map_err(|e| e.to_string())?;
    let client = make_client(provider, &credentials).map_err(|e| e.to_string())?;

    client
        .list_folder(folder_id.as_str())
        .await
        .map_err(|e| e.to_string())
}

// --- Catalogue / manifest ---

#[tauri::command]
pub async fn list_methods(state: State<'_, AppState>) -> Result<Vec<Method>, String> {
    state.manifest.load_all().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_method(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    provider: ProviderId,
    root_folder_id: String,
) -> Result<Vec<Method>, String> {
    let credentials = get_or_refresh_credentials(state)
        .await
        .map_err(|e| e.to_string())?;
    let client = make_client(provider, &credentials).map_err(|e| e.to_string())?;

    let on_progress = Arc::new(move |event: chelou_providers::ScanEvent| {
        let _ = app.emit("scan:progress", event);
    });

    chelou_providers::scan_methods_in_folder(&client, root_folder_id.as_str(), on_progress)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_method(state: State<'_, AppState>, method: Method) -> Result<(), String> {
    state.manifest.save(&method).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_method(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manifest.delete(&id).map_err(|e| e.to_string())
}

// --- Progress ---

#[tauri::command]
pub async fn mark_lesson_seen(
    state: State<'_, AppState>,
    method_id: String,
    lesson_id: String,
) -> Result<(), String> {
    state
        .manifest
        .mark_lesson_seen(&method_id, &lesson_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mark_lesson_unseen(
    state: State<'_, AppState>,
    method_id: String,
    lesson_id: String,
) -> Result<(), String> {
    state
        .manifest
        .mark_lesson_unseen(&method_id, &lesson_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_lesson_resume(
    state: State<'_, AppState>,
    method_id: String,
    lesson_id: String,
    resume_ms: f64,
) -> Result<(), String> {
    state
        .manifest
        .update_lesson_resume(&method_id, &lesson_id, resume_ms)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_backing_track_lead_in_override(
    state: State<'_, AppState>,
    method_id: String,
    lesson_id: String,
    file_id: String,
    lead_in_ms: f64,
) -> Result<(), String> {
    state
        .manifest
        .update_backing_track_lead_in_override(&method_id, &lesson_id, file_id, lead_in_ms)
        .map_err(|e| e.to_string())
}

pub async fn get_or_refresh_credentials(state: State<'_, AppState>) -> Result<StoredCredentials> {
    let (provider, credentials) = {
        let auth = state.auth.lock().unwrap();
        let provider = auth
            .active_provider()
            .ok_or_else(|| anyhow!("not authenticated"))?;
        let credentials = auth
            .credentials()
            .cloned()
            .ok_or_else(|| anyhow!("not authenticated"))?;
        (provider, credentials)
    };

    if credentials.expires_at.is_some_and(|exp| {
        exp < (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64)
            .try_into()
            .unwrap()
    }) {
        let (client_id, client_secret) = client_credentials(provider);
        let refreshed = make_auth(provider)
            .refresh(&credentials, client_id, client_secret)
            .await?;

        state
            .auth
            .lock()
            .unwrap()
            .save(provider, refreshed.clone())?;

        return Ok(refreshed);
    }

    Ok(credentials)
}

fn client_credentials(provider: ProviderId) -> (&'static str, &'static str) {
    match provider {
        ProviderId::PCloud => (PCLOUD_CLIENT_ID, PCLOUD_CLIENT_SECRET),
        ProviderId::Dropbox => (DROPBOX_CLIENT_ID, DROPBOX_CLIENT_SECRET),
    }
}
