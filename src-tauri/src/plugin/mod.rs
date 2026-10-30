// Plugin d'ampli VST3 de la voie guitare (spec §5.6, ARCHITECTURE.md §7).
// Le plugin vit sur le **thread principal** (les plugins JUCE y rattachent leur thread de
// messages, et son éditeur y est pompé par la boucle de Tauri) ; seule sa partie temps réel est
// confiée au moteur (`Engine::install_fx`). Le WebView le désigne par son identifiant dans le dernier
// scan persisté, jamais par un chemin (spec §5.11).
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use chelou_audio_core::plugin_state::{load_plugin_state, save_plugin_state};
use chelou_audio_core::plugins::{binary_path, standard_vst3_folders, PluginInfo};
use chelou_audio_engine::Engine;
use chelou_vst3_host::{Editor, Vst3Plugin};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::audio::update_settings;
use crate::commands::{public_err, AppState};

/// Lu par `audio_status`.
#[derive(Default)]
pub struct PluginStatus {
    pub active: Option<PluginInfo>,
    /// Nom du plugin mémorisé introuvable (ou qui n'a pas pu se charger) au démarrage.
    pub missing: Option<String>,
    pub latency_samples: u32,
}

struct Loaded {
    // Ordre de libération : l'éditeur avant le plugin.
    editor: Option<Editor>,
    plugin: Vst3Plugin,
    info: PluginInfo,
}

thread_local! {
    /// Plugin chargé — n'existe que sur le thread principal.
    static LOADED: RefCell<Option<Loaded>> = const { RefCell::new(None) };
}

/// Exécute `f` sur le thread principal et attend son résultat.
async fn on_main<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&AppHandle) -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(&handle));
    })
    .map_err(public_err)?;
    rx.await.map_err(public_err)
}

fn engine(app: &AppHandle) -> Option<&Engine> {
    // `AppState` vit aussi longtemps que l'app : la référence ne pend pas.
    let state: State<'_, AppState> = app.state();
    state.inner().audio.as_ref()
}

/// Thread principal. Ordre imposé : partie temps réel retirée de la voie (libérée ici, hors
/// callback), éditeur fermé, puis plugin libéré.
fn unload_now(app: &AppHandle) -> Option<Loaded> {
    let loaded = LOADED.with(|l| l.borrow_mut().take())?;
    if let Some(engine) = engine(app) {
        drop(engine.take_fx());
    }
    Some(loaded)
}

/// Libère un plugin retiré de la voie (cf. `unload_now`), après avoir enregistré son son s'il a
/// changé depuis le dernier enregistrement : rien de ce que l'utilisateur a réglé n'est perdu.
fn drop_loaded(app: &AppHandle, loaded: Loaded) {
    if loaded.plugin.take_dirty() {
        save_now(app, &loaded);
    }
    let Loaded {
        editor,
        plugin,
        info: _,
    } = loaded;
    if let Some(editor) = editor {
        editor.close();
    }
    drop(plugin);
}

/// Flux rouverts à une autre fréquence pendant chaque chargement : on abandonne.
const LOAD_ATTEMPTS: usize = 3;

/// Thread principal : charge `info` à la fréquence du moteur, avec l'état `state` s'il y en a un,
/// puis remplace le plugin chargé ; rend la latence du plugin. Échec : l'ancien plugin reste en
/// place et joue toujours. Flux rouverts à une autre fréquence pendant le chargement (la voie
/// refuse alors le plugin) : il est rechargé à la nouvelle.
fn load_now(app: &AppHandle, info: PluginInfo, state: Option<&[u8]>) -> anyhow::Result<u32> {
    let engine = engine(app).ok_or_else(|| anyhow::anyhow!("Aucune sortie audio disponible"))?;
    let bundle = Path::new(&info.path);
    if binary_path(bundle).is_none() {
        anyhow::bail!("{} introuvable", info.name);
    }
    for _ in 0..LOAD_ATTEMPTS {
        let rate = engine.sample_rate();
        let mut plugin = Vst3Plugin::load(bundle, &info.id, &info.name, rate)?;
        if let Some(state) = state {
            if let Err(e) = plugin.restore_state(state) {
                eprintln!("vst3 : réglages de {} non restaurés : {e:#}", info.name);
            }
        }
        let latency = plugin.latency_samples();
        match engine.install_fx(plugin.take_rt(), rate) {
            Ok(previous) => {
                // Partie temps réel de l'ancien plugin libérée avant lui (cf. `unload_now`).
                drop(previous);
                let old = LOADED.with(|l| {
                    l.borrow_mut().replace(Loaded {
                        editor: None,
                        plugin,
                        info: info.clone(),
                    })
                });
                if let Some(old) = old {
                    drop_loaded(app, old);
                }
                return Ok(latency);
            }
            Err(rt) => {
                drop(rt);
                drop(plugin);
            }
        }
    }
    anyhow::bail!("la sortie audio change de fréquence en continu")
}

fn set_status(app: &AppHandle, change: impl FnOnce(&mut PluginStatus)) {
    change(&mut app.state::<AppState>().plugin.lock().unwrap());
}

/// Charge `info` avec son dernier son enregistré, s'il est à lui. Le plugin en place enregistre
/// d'abord ses réglages récents : recharger le même plugin retrouve exactement son son.
async fn load(app: &AppHandle, info: PluginInfo) -> Result<u32, String> {
    let latency = on_main(app, move |app| {
        LOADED.with(|l| {
            if let Some(loaded) = l.borrow().as_ref() {
                if loaded.plugin.take_dirty() {
                    save_now(app, loaded);
                }
            }
        });
        let state = load_plugin_state(&app.state::<AppState>().plugin_state_path, &info.id);
        load_now(app, info, state.as_deref())
    })
    .await?
    .map_err(public_err)?;
    Ok(latency)
}

/// Démarrage : recharge le plugin mémorisé. Introuvable ou en échec → son direct et bandeau
/// « introuvable » ; le choix reste mémorisé (une réinstallation le retrouve). Le précédent
/// lancement a planté en le rechargeant (`plugin_loading` resté vrai) → il est oublié, bandeau.
pub async fn restore_saved(app: AppHandle) {
    let (saved, crashed) = {
        let settings = app
            .state::<AppState>()
            .audio_settings
            .lock()
            .unwrap()
            .clone();
        (settings.plugin, settings.plugin_loading)
    };
    let Some(info) = saved else {
        return;
    };
    let state = app.state::<AppState>();
    if crashed {
        eprintln!(
            "vst3 : {} a planté au dernier lancement, non rechargé",
            info.name
        );
        let _ = update_settings(&state, |s| {
            s.plugin = None;
            s.plugin_loading = false;
        });
        set_status(&app, |s| s.missing = Some(info.name));
        return;
    }
    if let Err(e) = update_settings(&state, |s| s.plugin_loading = true) {
        eprintln!("vst3 : {e}");
    }
    let loaded = load(&app, info.clone()).await;
    let _ = update_settings(&state, |s| s.plugin_loading = false);
    match loaded {
        Ok(latency) => set_status(&app, |s| {
            s.active = Some(info);
            s.missing = None;
            s.latency_samples = latency;
        }),
        Err(e) => {
            eprintln!("vst3 : {} non rechargé : {e}", info.name);
            set_status(&app, |s| s.missing = Some(info.name));
        }
    }
}

/// Après une réouverture des flux à une autre fréquence (l'effet a été retiré de la voie) :
/// recharge le plugin à la nouvelle fréquence avec les mêmes réglages, et rouvre son interface
/// si elle l'était.
pub async fn reload_for_new_rate(app: &AppHandle) {
    let reloaded = on_main(app, |app| {
        let old = unload_now(app)?;
        let state = old.plugin.save_state();
        let was_open = old.editor.as_ref().is_some_and(Editor::is_open);
        let info = old.info.clone();
        // Ce son est aussi celui à retrouver au prochain lancement.
        let path = &app.state::<AppState>().plugin_state_path;
        if let Err(e) = save_plugin_state(path, &info.id, &state) {
            eprintln!("vst3 : son de {} non enregistré : {e:#}", info.name);
        }
        old.plugin.take_dirty();
        drop_loaded(app, old);
        let result = load_now(app, info.clone(), Some(&state));
        if was_open && result.is_ok() {
            let _ = open_editor_now(app);
        }
        Some((info, result))
    })
    .await;
    match reloaded {
        Ok(Some((_, Ok(latency)))) => set_status(app, |s| s.latency_samples = latency),
        Ok(Some((info, Err(e)))) => {
            eprintln!("vst3 : {} non rechargé : {e:#}", info.name);
            set_status(app, |s| {
                s.active = None;
                s.missing = Some(info.name);
                s.latency_samples = 0;
            });
        }
        _ => {}
    }
}

/// Fin de l'app (thread principal) : démontage ordonné avant la sortie du processus.
pub fn shutdown(app: &AppHandle) {
    if let Some(loaded) = unload_now(app) {
        save_now(app, &loaded);
        loaded.plugin.take_dirty();
        drop_loaded(app, loaded);
    }
}

/// Vérification de l'état du plugin : son est-il à enregistrer ?
const STATE_SAVE_PERIOD: std::time::Duration = std::time::Duration::from_secs(2);

/// Thread principal : enregistre le son du plugin chargé (spec §5.6 : survit à un plantage).
fn save_now(app: &AppHandle, loaded: &Loaded) {
    let path = &app.state::<AppState>().plugin_state_path;
    if let Err(e) = save_plugin_state(path, &loaded.info.id, &loaded.plugin.save_state()) {
        eprintln!("vst3 : son de {} non enregistré : {e:#}", loaded.info.name);
    }
}

/// Toutes les `STATE_SAVE_PERIOD`, sur le thread principal : enregistre le son du plugin si un
/// réglage a changé ou si son interface vient d'être fermée.
pub fn start_state_saver(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(STATE_SAVE_PERIOD).await;
            let saved = on_main(&app, |app| {
                LOADED.with(|l| {
                    let mut l = l.borrow_mut();
                    let Some(loaded) = l.as_mut() else {
                        return;
                    };
                    let closed = loaded.editor.as_ref().is_some_and(|e| !e.is_open());
                    if closed {
                        loaded.editor = None;
                    }
                    if loaded.plugin.take_dirty() || closed {
                        save_now(app, loaded);
                    }
                })
            })
            .await;
            if saved.is_err() {
                return; // boucle d'évènements arrêtée : l'app se ferme
            }
        }
    });
}

/// Thème de l'app (celui de sa fenêtre, qui suit le système) : sombre sauf thème clair connu.
fn dark_theme(app: &AppHandle) -> bool {
    app.webview_windows()
        .values()
        .next()
        .and_then(|w| w.theme().ok())
        != Some(tauri::Theme::Light)
}

fn open_editor_now(app: &AppHandle) -> anyhow::Result<()> {
    let dark = dark_theme(app);
    LOADED.with(|l| {
        let mut l = l.borrow_mut();
        let loaded = l
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Aucun plugin chargé"))?;
        match &loaded.editor {
            Some(editor) if editor.is_open() => editor.focus(),
            _ => {
                let title = format!("{} — Chelou Track", loaded.info.name);
                loaded.editor = Some(Editor::open(&loaded.plugin, &title, dark)?);
            }
        }
        Ok(())
    })
}

/// Mode `--vst3-probe <bundle>` du binaire : imprime les classes du bundle en JSON (processus
/// enfant du scan, cf. `chelou_vst3_host::scan`). Code de sortie 0 si lu, 1 sinon.
pub fn probe_main(bundle: Option<&std::ffi::OsStr>) -> i32 {
    let Some(bundle) = bundle else {
        return 1;
    };
    match chelou_vst3_host::probe(Path::new(bundle)) {
        Ok(classes) => match serde_json::to_string(&classes) {
            Ok(json) => {
                println!("{json}");
                0
            }
            Err(_) => 1,
        },
        Err(e) => {
            eprintln!("{e:#}");
            1
        }
    }
}

// --- Commandes ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFolders {
    standard: Vec<String>,
    custom: Vec<String>,
}

fn folder_strings(folders: Vec<PathBuf>) -> Vec<String> {
    folders
        .into_iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
pub fn plugin_folders(state: State<'_, AppState>) -> PluginFolders {
    PluginFolders {
        standard: folder_strings(standard_vst3_folders()),
        custom: state.audio_settings.lock().unwrap().vst3_folders.clone(),
    }
}

/// Ajoute un dossier choisi dans la boîte de dialogue native ; `false` si annulé ou déjà présent.
#[tauri::command]
pub async fn plugin_add_folder(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Dossier de plugins VST3")
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
    let Some(folder) = rx.await.map_err(public_err)? else {
        return Ok(false);
    };
    let folder = folder
        .into_path()
        .map_err(public_err)?
        .to_string_lossy()
        .into_owned();
    let mut added = false;
    update_settings(&app.state::<AppState>(), |s| {
        if !s.vst3_folders.contains(&folder) {
            s.vst3_folders.push(folder);
            added = true;
        }
    })?;
    Ok(added)
}

#[tauri::command]
pub fn plugin_remove_folder(state: State<'_, AppState>, path: String) -> Result<(), String> {
    update_settings(&state, |s| s.vst3_folders.retain(|f| *f != path)).map(drop)
}

/// Scanne les dossiers standard puis ceux de l'utilisateur, et mémorise le résultat.
#[tauri::command]
pub async fn plugin_scan(app: AppHandle) -> Result<Vec<PluginInfo>, String> {
    let custom = app
        .state::<AppState>()
        .audio_settings
        .lock()
        .unwrap()
        .vst3_folders
        .clone();
    let mut folders = standard_vst3_folders();
    folders.extend(custom.into_iter().map(PathBuf::from));
    let exe = std::env::current_exe().map_err(public_err)?;
    let plugins =
        tauri::async_runtime::spawn_blocking(move || chelou_vst3_host::scan::scan(&folders, &exe))
            .await
            .map_err(public_err)?;
    let found = plugins.clone();
    update_settings(&app.state::<AppState>(), |s| s.scan = Some(found))?;
    Ok(plugins)
}

/// Charge le plugin `id` du dernier scan et le mémorise.
#[tauri::command]
pub async fn plugin_load(app: AppHandle, id: String) -> Result<PluginInfo, String> {
    let info = app
        .state::<AppState>()
        .audio_settings
        .lock()
        .unwrap()
        .scan
        .iter()
        .flatten()
        .find(|p| p.id == id)
        .cloned()
        .ok_or("Plugin inconnu : relance le scan")?;
    let latency = load(&app, info.clone()).await?;
    set_status(&app, |s| {
        s.active = Some(info.clone());
        s.missing = None;
        s.latency_samples = latency;
    });
    let saved = info.clone();
    update_settings(&app.state::<AppState>(), |s| s.plugin = Some(saved))?;
    Ok(info)
}

/// Retire le plugin (son direct) et l'oublie ; masque aussi le bandeau « introuvable ».
#[tauri::command]
pub async fn plugin_unload(app: AppHandle) -> Result<(), String> {
    on_main(&app, |app| {
        if let Some(loaded) = unload_now(app) {
            drop_loaded(app, loaded);
        }
    })
    .await?;
    set_status(&app, |s| *s = PluginStatus::default());
    update_settings(&app.state::<AppState>(), |s| s.plugin = None).map(drop)
}

/// Ouvre l'interface du plugin, ou la ramène au premier plan si elle est déjà ouverte.
#[tauri::command]
pub async fn plugin_open_editor(app: AppHandle) -> Result<(), String> {
    on_main(&app, open_editor_now).await?.map_err(public_err)
}
