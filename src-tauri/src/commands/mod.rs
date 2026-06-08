// Tauri commands — the only surface the frontend can call via invoke().
// Token never leaves Rust (cf. ARCHITECTURE.md §3 + §5).
use std::sync::Mutex;
use tauri::State;

use crate::auth::AuthStore;
use crate::manifest::{ManifestStore, Method};
use crate::pcloud::PCloudClient;

pub struct AppState {
    pub auth: Mutex<AuthStore>,
    /// Constructed lazily when authenticated; None when logged out.
    pub client: Mutex<Option<PCloudClient>>,
    pub manifest: ManifestStore,
}

// --- Auth ---

#[tauri::command]
pub async fn pcloud_login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<(), String> {
    let (token_str, uid) = PCloudClient::login(&username, &password)
        .await
        .map_err(|e| e.to_string())?;

    let auth_token = crate::auth::AuthToken { token: token_str.clone(), uid };

    state.auth.lock().unwrap().save_token(auth_token).map_err(|e| e.to_string())?;
    *state.client.lock().unwrap() = Some(PCloudClient::new(token_str));
    Ok(())
}

#[tauri::command]
pub async fn pcloud_logout(state: State<'_, AppState>) -> Result<(), String> {
    state.auth.lock().unwrap().clear().map_err(|e| e.to_string())?;
    *state.client.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub async fn get_auth_status(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.auth.lock().unwrap().is_authenticated())
}

// --- Catalogue / manifest ---

#[tauri::command]
pub async fn list_methods(state: State<'_, AppState>) -> Result<Vec<Method>, String> {
    state.manifest.load_all().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_method(
    state: State<'_, AppState>,
    root_folder_id: u64,
    title: String,
) -> Result<Method, String> {
    let token = {
        let auth = state.auth.lock().unwrap();
        auth.token().map(|t| t.token.clone()).ok_or("not authenticated")?
    };
    let client = PCloudClient::new(token);
    crate::pcloud::scanner::scan_tree(&client, root_folder_id, &title)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_method(
    state: State<'_, AppState>,
    method: Method,
) -> Result<(), String> {
    state.manifest.save(&method).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_method(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.manifest.delete(&id).map_err(|e| e.to_string())
}
