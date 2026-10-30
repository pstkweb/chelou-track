mod audio;
mod auth;
mod commands;
mod manifest;
#[cfg(target_os = "linux")]
mod media_server;
mod plugin;
mod providers;
mod stream;

use commands::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    // AppRun (AppImage) sets GST_PLUGIN_SYSTEM_PATH{,_1_0} to only the bundled
    // usr/lib/gstreamer-1.0 dir (from bundle.linux.appimage.bundleMediaFramework). Unlike
    // GST_PLUGIN_PATH (additive — extra dirs on top of the defaults), GST_PLUGIN_SYSTEM_PATH
    // *replaces* the default system search entirely when set, hiding the system's own
    // GStreamer plugins — including basic video sinks (confirmed: "GStreamer element
    // fakevideosink not found" — video fails while audio, needing no video sink, plays fine).
    // Clearing it restores GStreamer's compiled-in default system paths (correct on any distro,
    // no hardcoded path needed) while GST_PLUGIN_PATH keeps the bundled decoders available.
    // Only this pair, deliberately: GST_PLUGIN_PATH / GST_PLUGIN_SCANNER / GST_PTP_HELPER must
    // keep pointing into the bundle. WebKitWebProcess loads the *bundled* GStreamer core (via
    // LD_LIBRARY_PATH, which we can't touch without breaking WebKit) and that core's built-in
    // default plugin dir is the Ubuntu build path — missing elsewhere, so clearing them too
    // leaves it with no plugins at all (appsrc/appsink not found, app hangs).
    // Gated on $APPIMAGE so .deb/.rpm/pacman installs (which never set it) are untouched.
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_some() {
        std::env::remove_var("GST_PLUGIN_SYSTEM_PATH");
        std::env::remove_var("GST_PLUGIN_SYSTEM_PATH_1_0");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("stream", |app, request, responder| {
            stream::handle(app, request, responder);
        })
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            #[cfg(target_os = "windows")]
            keyring_core::set_default_store(keyring_dpapi_store::Store::new(app_dir.clone()));
            // No panic if the OS store is unavailable (e.g. Linux without a Secret Service on
            // D-Bus): the app still starts, and the login attempt surfaces keyring-core's
            // "no default store" error in the UI instead of the process aborting at launch.
            #[cfg(target_os = "macos")]
            match apple_native_keyring_store::keychain::Store::new() {
                Ok(store) => keyring_core::set_default_store(store),
                Err(e) => eprintln!("keyring: macOS keychain unavailable, login disabled: {e}"),
            }
            #[cfg(target_os = "linux")]
            match zbus_secret_service_keyring_store::Store::new() {
                Ok(store) => keyring_core::set_default_store(store),
                Err(e) => eprintln!(
                    "keyring: Secret Service unavailable (is gnome-keyring/KWallet running?), \
                    login disabled: {e}"
                ),
            }

            let audio_settings_path = chelou_audio_core::settings::settings_path(&app_dir);
            let audio_settings = chelou_audio_core::settings::load_settings(&audio_settings_path);
            let audio = audio::start_engine(&audio_settings);

            let mut auth = auth::AuthStore::new();
            // Restore token from the credential store (normal path).
            let _ = auth.load_from_keychain();

            app.manage(AppState {
                auth: Mutex::new(auth),
                manifest: manifest::ManifestStore::new(app_dir),
                http: reqwest::Client::new(),
                url_cache: Mutex::new(HashMap::new()),
                client_cache: Mutex::new(None),
                media_token: cfg!(target_os = "linux").then(chelou_providers::random_token),
                audio,
                audio_settings: Mutex::new(audio_settings),
                audio_settings_path,
                plugin: Mutex::new(plugin::PluginStatus::default()),
            });

            audio::open_saved_card(app.handle());

            #[cfg(target_os = "linux")]
            media_server::spawn(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::oauth_start,
            commands::connect_local,
            commands::logout,
            commands::get_auth_status,
            commands::get_media_token,
            commands::list_folder,
            commands::list_methods,
            commands::scan_method,
            commands::save_method,
            commands::delete_method,
            commands::mark_lesson_seen,
            commands::mark_lesson_unseen,
            commands::update_lesson_resume,
            commands::update_backing_track_lead_in_override,
            audio::backing_load,
            audio::backing_unload,
            audio::backing_play,
            audio::backing_pause,
            audio::backing_seek,
            audio::backing_set_volume,
            audio::backing_status,
            audio::audio_devices,
            audio::audio_settings,
            audio::audio_configure,
            audio::audio_set_enabled,
            audio::guitar_set_fader,
            audio::guitar_set_muted,
            audio::audio_reset_xruns,
            audio::audio_status,
            plugin::plugin_folders,
            plugin::plugin_add_folder,
            plugin::plugin_remove_folder,
            plugin::plugin_scan,
            plugin::plugin_load,
            plugin::plugin_unload,
            plugin::plugin_open_editor,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Le plugin doit être démonté (partie temps réel retirée de la voie, éditeur fermé)
            // avant la sortie du processus : sa DLL ne doit pas disparaître sous le callback.
            if let tauri::RunEvent::Exit = event {
                plugin::shutdown(app);
            }
        });
}

/// Mode `--vst3-probe <bundle>` (processus enfant du scan des plugins) : cf. `plugin::probe_main`.
pub fn vst3_probe(bundle: Option<&std::ffi::OsStr>) -> i32 {
    plugin::probe_main(bundle)
}
