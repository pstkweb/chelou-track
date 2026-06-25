mod auth;
mod commands;
mod manifest;
mod pcloud;
mod stream;

use commands::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    keyring_core::set_default_store(
        windows_native_keyring_store::Store::new()
            .expect("failed to init Windows credential store"),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .register_asynchronous_uri_scheme_protocol("stream", |app, request, responder| {
            stream::handle(app, request, responder);
        })
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            let mut auth = auth::AuthStore::new();
            // Restore token from OS keychain (normal path).
            let _ = auth.load_from_keychain();

            app.manage(AppState {
                auth: Mutex::new(auth),
                manifest: manifest::ManifestStore::new(app_dir),
                url_cache: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pcloud_oauth_start,
            commands::pcloud_logout,
            commands::get_auth_status,
            commands::list_folder,
            commands::list_methods,
            commands::scan_method,
            commands::save_method,
            commands::delete_method,
            commands::mark_lesson_seen,
            commands::mark_lesson_unseen,
            commands::update_lesson_resume,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
