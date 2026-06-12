mod auth;
mod commands;
mod manifest;
mod pcloud;
mod stream;

use commands::AppState;
use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .register_asynchronous_uri_scheme_protocol("stream", |app, request, responder| {
            stream::handle(app, request, responder);
        })
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            let mut auth = auth::AuthStore::new();
            // Attempt to restore a previously saved token from the OS keychain.
            let _ = auth.load_from_keychain();

            app.manage(AppState {
                auth: Mutex::new(auth),
                manifest: manifest::ManifestStore::new(app_dir),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pcloud_login,
            commands::pcloud_logout,
            commands::get_auth_status,
            commands::list_methods,
            commands::list_folder,
            commands::scan_method,
            commands::save_method,
            commands::delete_method,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
