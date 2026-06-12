// Tauri commands — the only surface the frontend can call via invoke().
// Token never leaves Rust (cf. ARCHITECTURE.md §3 + §5).
use std::sync::Mutex;
use tauri::State;

use crate::auth::{AuthStore, StoredCredentials};
use crate::manifest::{ManifestStore, Method};
use crate::pcloud::PCloudClient;

pub struct AppState {
    pub auth: Mutex<AuthStore>,
    pub manifest: ManifestStore,
}

// --- Auth ---

#[tauri::command]
pub async fn pcloud_login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<(), String> {
    let client =
        PCloudClient::new(username.clone(), password.clone()).map_err(|e| e.to_string())?;

    // Validate credentials with a real API call before storing them.
    client
        .list_folder(0)
        .await
        .map_err(|e| format!("authentication failed: {e}"))?;

    let creds = StoredCredentials { username, password };
    state
        .auth
        .lock()
        .unwrap()
        .save_credentials(creds)
        .map_err(|e| e.to_string())?;

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

// --- Catalogue / manifest ---

#[tauri::command]
pub async fn list_folder(
    state: State<'_, AppState>,
    folder_id: u64,
) -> Result<crate::pcloud::FolderContents, String> {
    let (username, password) = {
        let auth = state.auth.lock().unwrap();
        let creds = auth.credentials().ok_or("not authenticated")?;
        (creds.username.clone(), creds.password.clone())
    };
    let client = PCloudClient::new(username, password).map_err(|e| e.to_string())?;
    client.list_folder(folder_id).await.map_err(|e| e.to_string())
}


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
    let (username, password) = {
        let auth = state.auth.lock().unwrap();
        let creds = auth.credentials().ok_or("not authenticated")?;
        (creds.username.clone(), creds.password.clone())
    };
    let client = PCloudClient::new(username, password).map_err(|e| e.to_string())?;
    crate::pcloud::scanner::scan_tree(&client, root_folder_id, &title)
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
