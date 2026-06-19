// Tauri commands — the only surface the frontend can call via invoke().
// Token never leaves Rust (cf. ARCHITECTURE.md §3 + §5).
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Emitter, State};

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
