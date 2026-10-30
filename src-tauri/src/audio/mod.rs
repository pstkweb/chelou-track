// Commandes du moteur audio natif (ARCHITECTURE.md §7).
// - Backing track : le track est téléchargé et décodé entièrement avant lecture, puis le WebView
//   suit la position via un `Channel` (horloge maître = moteur, AlphaTab piloté).
// - Carte son et son direct : réglages persistés (`audio-settings.json`), inventaire des cartes,
//   statut permanent pour la barre de titre et ses vumètres.
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;

use chelou_audio_core::decode::{decode_stereo, resample};
use chelou_audio_core::player::LoadGeneration;
use chelou_audio_core::settings::{latency_ms, save_settings, AudioSettings, DuplexConfig};
use chelou_audio_core::silence::scan_leading_silence;
use chelou_audio_engine::{Engine, EngineState, HostDevices};
use chelou_providers::ProviderId;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::commands::{public_err, AppState};
use crate::stream::{resolve_media, StreamKind};

/// Période d'envoi du statut pendant la lecture (~60 Hz).
const STATUS_PERIOD: Duration = Duration::from_millis(16);

/// Dernier chargement demandé : un chargement plus ancien qui se termine après ne remplace rien.
static LOAD_GENERATION: LoadGeneration = LoadGeneration::new();
const MAX_RESAMPLE_ATTEMPTS: usize = 3;

/// Abonnement au statut en cours : un nouvel appel à `backing_status` arrête le précédent.
static STATUS_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Période du statut de la carte (~30 Hz : vumètres).
const AUDIO_STATUS_PERIOD: Duration = Duration::from_millis(33);
/// Même mécanisme que `STATUS_GENERATION`, pour `audio_status`.
static AUDIO_STATUS_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackingInfo {
    /// Track auquel se rapportent ces infos (le WebView a pu en choisir un autre entre-temps).
    file_id: String,
    duration_ms: f64,
    silence_ms: Option<f64>,
    sample_rate: u32,
}

#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackingStatus {
    position_ms: f64,
    duration_ms: f64,
    playing: bool,
    ended: bool,
    loaded: bool,
    sample_rate: u32,
    error: Option<String>,
}

fn engine<'a>(state: &'a State<'_, AppState>) -> Result<&'a Engine, String> {
    state
        .audio
        .as_ref()
        .ok_or_else(|| "Aucune sortie audio disponible".to_string())
}

#[tauri::command]
pub async fn backing_load(
    state: State<'_, AppState>,
    provider: ProviderId,
    file_id: String,
) -> Result<BackingInfo, String> {
    let generation = LOAD_GENERATION.next();
    engine(&state)?;
    let response = resolve_media(
        state.clone(),
        provider,
        StreamKind::Audio,
        file_id.clone(),
        false,
        None,
    )
    .await
    .map_err(public_err)?;
    // Sans ce contrôle, un corps d'erreur (403, 404…) partirait au décodeur et finirait en
    // « format audio non reconnu », masquant la vraie cause.
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "téléchargement de la backing track refusé (HTTP {})",
            response.status
        ));
    }
    let (source, silence_ms) = tauri::async_runtime::spawn_blocking(move || {
        let pcm = decode_stereo(response.body, None)?;
        // Détection sur la fréquence d'origine, avant rééchantillonnage.
        let silence_ms = scan_leading_silence(&pcm.left(), pcm.sample_rate);
        anyhow::Ok((Arc::new(pcm), silence_ms))
    })
    .await
    .map_err(public_err)?
    .map_err(public_err)?;

    // La fréquence de sortie peut changer pendant le rééchantillonnage (autre périphérique) :
    // le lecteur refuse alors le track et on recommence à la nouvelle fréquence.
    for _ in 0..MAX_RESAMPLE_ATTEMPTS {
        let sample_rate = engine(&state)?.sample_rate();
        let src = source.clone();
        let pcm = tauri::async_runtime::spawn_blocking(move || resample(&src, sample_rate))
            .await
            .map_err(public_err)?
            .map_err(public_err)?;
        let duration_ms = pcm.duration_ms();
        // Même verrou que backing_unload : un déchargement ne peut pas se glisser entre la
        // vérification de génération et le chargement.
        let mut backing = engine(&state)?.backing();
        if !LOAD_GENERATION.is_current(generation) {
            return Err("chargement remplacé par un plus récent".into());
        }
        if backing.load(Arc::new(pcm)) {
            return Ok(BackingInfo {
                file_id,
                duration_ms,
                silence_ms,
                sample_rate,
            });
        }
    }
    Err("la sortie audio change de fréquence en continu".into())
}

#[tauri::command]
pub fn backing_unload(state: State<'_, AppState>) -> Result<(), String> {
    let mut backing = engine(&state)?.backing();
    // Invalide aussi un chargement encore en cours (vue quittée pendant le téléchargement).
    LOAD_GENERATION.next();
    backing.unload();
    Ok(())
}

#[tauri::command]
pub fn backing_play(state: State<'_, AppState>) -> Result<(), String> {
    engine(&state)?.backing().play();
    Ok(())
}

#[tauri::command]
pub fn backing_pause(state: State<'_, AppState>) -> Result<(), String> {
    engine(&state)?.backing().pause();
    Ok(())
}

#[tauri::command]
pub fn backing_seek(state: State<'_, AppState>, ms: f64) -> Result<(), String> {
    engine(&state)?.backing().seek_ms(ms);
    Ok(())
}

#[tauri::command]
pub fn backing_set_volume(state: State<'_, AppState>, gain: f32) -> Result<(), String> {
    engine(&state)?.backing().set_gain(gain);
    Ok(())
}

/// Pousse le statut dans `channel` : toutes les 16 ms pendant la lecture, et à chaque
/// changement sinon (pause, seek, chargement, fin, erreur). S'arrête quand le WebView ne
/// reçoit plus ou qu'un nouvel abonnement le remplace.
#[tauri::command]
pub fn backing_status(app: AppHandle, channel: Channel<BackingStatus>) {
    let generation = STATUS_GENERATION.fetch_add(1, Relaxed) + 1;
    tauri::async_runtime::spawn(async move {
        let mut last: Option<BackingStatus> = None;
        while STATUS_GENERATION.load(Relaxed) == generation {
            let status = current_status(&app.state::<AppState>());
            if status.playing || last.as_ref() != Some(&status) {
                if channel.send(status.clone()).is_err() {
                    break;
                }
                last = Some(status);
            }
            tokio::time::sleep(STATUS_PERIOD).await;
        }
    });
}

fn current_status(state: &AppState) -> BackingStatus {
    let Some(engine) = state.audio.as_ref() else {
        return BackingStatus {
            position_ms: 0.0,
            duration_ms: 0.0,
            playing: false,
            ended: false,
            loaded: false,
            sample_rate: 0,
            error: Some("Aucune sortie audio disponible".into()),
        };
    };
    let mut backing = engine.backing();
    // Libère hors thread audio les tracks remplacés.
    backing.collect_garbage();
    let s = backing.status();
    BackingStatus {
        position_ms: s.position_ms,
        duration_ms: s.duration_ms,
        playing: s.playing,
        ended: s.ended,
        loaded: s.loaded,
        sample_rate: backing.sample_rate(),
        // Une carte en échec avec une sortie de repli n'empêche pas la backing track de jouer :
        // l'erreur est montrée par la barre de titre, pas par la vue tablature.
        error: match engine.state() {
            EngineState::Error(e) if !engine.has_output() => Some(e),
            _ => None,
        },
    }
}

// --- Carte son, son direct ---

/// Configuration à ouvrir : la carte choisie, sauf si l'utilisateur l'a arrêtée.
fn wanted(settings: &AudioSettings) -> Option<DuplexConfig> {
    settings
        .engine_enabled
        .then(|| settings.duplex.clone())
        .flatten()
}

/// Démarre le moteur en sortie seule (rapide) avec le volume et le mute persistés ; la carte
/// choisie est ouverte ensuite par `open_saved_card`, hors du lancement de la fenêtre.
pub fn start_engine(settings: &AudioSettings) -> Option<Engine> {
    let engine = chelou_audio_engine::start(None)
        .inspect_err(|e| eprintln!("audio : sortie indisponible au démarrage : {e:#}"))
        .ok()?;
    let mut guitar = engine.guitar();
    guitar.set_fader(settings.guitar_fader);
    guitar.set_muted(settings.guitar_muted);
    drop(guitar);
    Some(engine)
}

/// Ouvre la carte des réglages persistés en arrière-plan : un pilote qui ne répond pas
/// (matériel USB débranché) ne doit pas empêcher la fenêtre de s'afficher.
pub fn open_saved_card(app: &AppHandle) {
    let settings = app
        .state::<AppState>()
        .audio_settings
        .lock()
        .unwrap()
        .clone();
    if wanted(&settings).is_some() {
        tauri::async_runtime::spawn(apply_settings(app.clone(), settings));
    }
}

/// Modifie les réglages puis les persiste ; rend la copie à jour.
fn update_settings(
    state: &AppState,
    change: impl FnOnce(&mut AudioSettings),
) -> Result<AudioSettings, String> {
    let mut settings = state.audio_settings.lock().unwrap();
    change(&mut settings);
    save_settings(&state.audio_settings_path, &settings).map_err(public_err)?;
    Ok(settings.clone())
}

/// Rouvre la carte (thread bloquant : `Engine::apply` attend le thread moteur). Un échec
/// d'ouverture n'est pas une erreur de commande : il est porté par `audio_status`.
async fn apply_settings(app: AppHandle, settings: AudioSettings) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if let Some(engine) = state.audio.as_ref() {
            if let Err(e) = engine.apply(wanted(&settings)) {
                eprintln!("audio : ouverture de la carte : {e:#}");
            }
        }
    })
    .await
    .map_err(public_err)
}

/// Cartes de chaque pilote (charge les pilotes ASIO un à un : hors thread async).
#[tauri::command]
pub async fn audio_devices(app: AppHandle) -> Result<Vec<HostDevices>, String> {
    tauri::async_runtime::spawn_blocking(move || match app.state::<AppState>().audio.as_ref() {
        Some(engine) => engine.devices(),
        None => chelou_audio_engine::list_devices(),
    })
    .await
    .map_err(public_err)
}

#[tauri::command]
pub fn audio_settings(state: State<'_, AppState>) -> AudioSettings {
    state.audio_settings.lock().unwrap().clone()
}

/// Ouvre la carte choisie (`None` : oublie la carte, sortie seule) et la mémorise, même si
/// l'ouverture échoue : le choix de l'utilisateur est gardé, l'erreur est dans le statut.
#[tauri::command]
pub async fn audio_configure(app: AppHandle, duplex: Option<DuplexConfig>) -> Result<(), String> {
    let settings = update_settings(&app.state::<AppState>(), |s| {
        s.duplex = duplex;
        s.engine_enabled = true;
    })?;
    apply_settings(app, settings).await
}

/// « Arrêter » (`false`) / « Démarrer », « Redémarrer » (`true`) — spec §5.7.
#[tauri::command]
pub async fn audio_set_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let settings = update_settings(&app.state::<AppState>(), |s| s.engine_enabled = enabled)?;
    apply_settings(app, settings).await
}

#[tauri::command]
pub fn guitar_set_fader(state: State<'_, AppState>, fader: f32) -> Result<(), String> {
    if let Some(engine) = state.audio.as_ref() {
        engine.guitar().set_fader(fader);
    }
    update_settings(&state, |s| s.guitar_fader = fader.clamp(0.0, 100.0)).map(drop)
}

#[tauri::command]
pub fn guitar_set_muted(state: State<'_, AppState>, muted: bool) -> Result<(), String> {
    if let Some(engine) = state.audio.as_ref() {
        engine.guitar().set_muted(muted);
    }
    update_settings(&state, |s| s.guitar_muted = muted).map(drop)
}

#[tauri::command]
pub fn audio_reset_xruns(state: State<'_, AppState>) {
    if let Some(engine) = state.audio.as_ref() {
        engine.reset_xruns();
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    /// `"off"` (sortie seule, aucune carte), `"running"`, `"stopped"` (carte arrêtée par
    /// l'utilisateur), `"error"`.
    state: &'static str,
    error: Option<String>,
    sample_rate: u32,
    /// `None` hors duplex et en WASAPI (buffer système).
    buffer: Option<u32>,
    latency_ms: Option<f64>,
    xruns: u64,
    input_level: f32,
    output_level: f32,
    guitar_fader: f32,
    guitar_muted: bool,
    device_name: Option<String>,
}

fn audio_status_now(state: &AppState) -> AudioStatus {
    let settings = state.audio_settings.lock().unwrap().clone();
    let mut status = AudioStatus {
        state: "error",
        error: Some("Aucune sortie audio disponible".into()),
        sample_rate: 0,
        buffer: None,
        latency_ms: None,
        xruns: 0,
        input_level: 0.0,
        output_level: 0.0,
        guitar_fader: settings.guitar_fader,
        guitar_muted: settings.guitar_muted,
        device_name: settings.duplex.as_ref().map(|d| d.input_device.clone()),
    };
    let Some(engine) = state.audio.as_ref() else {
        return status;
    };
    status.sample_rate = engine.sample_rate();
    status.xruns = engine.xruns();
    let levels = engine.guitar().levels();
    status.input_level = levels.input;
    status.output_level = levels.output;
    (status.state, status.error) = match engine.state() {
        EngineState::Running => ("running", None),
        EngineState::Error(e) => ("error", Some(e)),
        EngineState::OutputOnly if settings.duplex.is_some() && !settings.engine_enabled => {
            ("stopped", None)
        }
        EngineState::OutputOnly => ("off", None),
    };
    if status.state == "running" {
        let duplex = settings.duplex.as_ref();
        status.buffer = duplex.filter(|d| d.host != "wasapi").map(|d| d.buffer);
        status.latency_ms = status.buffer.map(|b| latency_ms(b, status.sample_rate));
    }
    status
}

/// Pousse le statut de la carte toutes les 33 ms, en permanence (vumètres de la barre de titre).
/// Un nouvel abonnement remplace le précédent ; les crêtes sont remises à zéro à chaque envoi.
#[tauri::command]
pub fn audio_status(app: AppHandle, channel: Channel<AudioStatus>) {
    let generation = AUDIO_STATUS_GENERATION.fetch_add(1, Relaxed) + 1;
    tauri::async_runtime::spawn(async move {
        while AUDIO_STATUS_GENERATION.load(Relaxed) == generation {
            let status = audio_status_now(&app.state::<AppState>());
            if channel.send(status).is_err() {
                break;
            }
            tokio::time::sleep(AUDIO_STATUS_PERIOD).await;
        }
    });
}
