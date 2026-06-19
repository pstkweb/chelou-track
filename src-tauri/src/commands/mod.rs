// Tauri commands — the only surface the frontend can call via invoke().
// Token never leaves Rust (cf. ARCHITECTURE.md §3 + §5).
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Emitter, Manager, State};

// Client credentials baked at compile time.
// Set PCLOUD_CLIENT_ID and PCLOUD_CLIENT_SECRET in the environment before building,
// e.g. via src-tauri/.cargo/config.toml (gitignored).
const PCLOUD_CLIENT_ID: &str = env!("PCLOUD_CLIENT_ID");
const PCLOUD_CLIENT_SECRET: &str = env!("PCLOUD_CLIENT_SECRET");

use crate::auth::AuthStore;
use crate::manifest::{ManifestStore, Method};
use crate::pcloud::PCloudClient;

pub struct AppState {
    pub auth: Mutex<AuthStore>,
    pub manifest: ManifestStore,
    /// Cache of pCloud download URLs: (file_id, is_video_link) → (url, fetched_at).
    /// Avoids one getfilelink API round-trip per Range chunk during media playback.
    pub url_cache: Mutex<HashMap<(u64, bool), (String, Instant)>>,
}

// --- Auth ---

/// Start the pCloud OAuth2 authorization code flow (RFC 8252 loopback redirect).
/// Opens the system browser with the pCloud authorization page, then waits in the
/// background for the callback. On success emits `oauth:complete`; on error `oauth:error`.
#[tauri::command]
pub async fn pcloud_oauth_start(app: tauri::AppHandle) -> Result<(), String> {
    let (auth_url, listener) = crate::auth::oauth::start_loopback_server(PCLOUD_CLIENT_ID)
        .await
        .map_err(|e| e.to_string())?;

    open::that(&auth_url).map_err(|e| format!("failed to open browser: {e}"))?;

    // Block until the user completes auth in the browser or the 5-minute timeout elapses.
    let token =
        crate::auth::oauth::wait_for_token(listener, PCLOUD_CLIENT_ID, PCLOUD_CLIENT_SECRET)
            .await
            .map_err(|e| e.to_string())?;

    {
        let state = app.state::<AppState>();
        let r = state.auth.lock().unwrap().save_token(token);
        r
    }
    .map_err(|e| e.to_string())?;

    let _ = app.emit("oauth:complete", ());
    Ok(())
}

#[tauri::command]
pub async fn pcloud_logout(state: State<'_, AppState>) -> Result<(), String> {
    state
        .auth
        .lock()
        .unwrap()
        .clear()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_auth_status(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.auth.lock().unwrap().is_authenticated())
}

// --- pCloud folder browsing ---

#[tauri::command]
pub async fn list_folder(
    state: State<'_, AppState>,
    folder_id: u64,
) -> Result<crate::pcloud::FolderContents, String> {
    let token = {
        let auth = state.auth.lock().unwrap();
        auth.token().ok_or("not authenticated")?.to_owned()
    };
    let client = PCloudClient::new(token).map_err(|e| e.to_string())?;
    client
        .list_folder(folder_id)
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
    root_folder_id: u64,
) -> Result<Vec<Method>, String> {
    let token = {
        let auth = state.auth.lock().unwrap();
        auth.token().ok_or("not authenticated")?.to_owned()
    };
    let client = PCloudClient::new(token).map_err(|e| e.to_string())?;
    let on_progress = Arc::new(move |event: crate::pcloud::ScanEvent| {
        let _ = app.emit("scan:progress", event);
    });
    crate::pcloud::scan_methods_in_folder(&client, root_folder_id, on_progress)
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
