mod auth;
mod commands;
mod manifest;
#[cfg(target_os = "linux")]
mod media_server;
mod providers;
mod stream;

use commands::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .register_asynchronous_uri_scheme_protocol("stream", |app, request, responder| {
            stream::handle(app, request, responder);
        })
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            #[cfg(target_os = "windows")]
            keyring_core::set_default_store(keyring_dpapi_store::Store::new(app_dir.clone()));
            #[cfg(target_os = "macos")]
            keyring_core::set_default_store(
                apple_native_keyring_store::keychain::Store::new()
                    .expect("failed to init macOS keychain store"),
            );
            #[cfg(target_os = "linux")]
            keyring_core::set_default_store(
                zbus_secret_service_keyring_store::Store::new()
                    .expect("failed to init Secret Service store"),
            );

            let mut auth = auth::AuthStore::new();
            // Restore token from the credential store (normal path).
            let _ = auth.load_from_keychain();

            app.manage(AppState {
                auth: Mutex::new(auth),
                manifest: manifest::ManifestStore::new(app_dir),
                http: reqwest::Client::new(),
                url_cache: Mutex::new(HashMap::new()),
                client_cache: Mutex::new(None),
            });

            #[cfg(target_os = "linux")]
            media_server::spawn(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::oauth_start,
            commands::logout,
            commands::get_auth_status,
            commands::list_folder,
            commands::list_methods,
            commands::scan_method,
            commands::save_method,
            commands::delete_method,
            commands::mark_lesson_seen,
            commands::mark_lesson_unseen,
            commands::update_lesson_resume,
            commands::update_backing_track_lead_in_override,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
