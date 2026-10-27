// Commandes de la backing track jouée par le moteur natif (ARCHITECTURE.md §7) : le track est
// téléchargé et décodé entièrement avant lecture, puis le WebView suit la position via un
// `Channel` (horloge maître = moteur, AlphaTab piloté).
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;

use chelou_audio_core::decode::{decode_stereo, resample};
use chelou_audio_core::player::LoadGeneration;
use chelou_audio_core::silence::scan_leading_silence;
use chelou_audio_engine::{Engine, EngineState};
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
        error: match engine.state() {
            EngineState::Running => None,
            EngineState::Error(e) => Some(e),
        },
    }
}
