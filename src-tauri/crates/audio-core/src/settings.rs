// Réglages audio persistés (`audio/settings.json` dans l'app-data dir, ARCHITECTURE.md §9) et
// calculs purs affichés par l'écran de réglages.

use std::io::Write;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::plugins::PluginInfo;

pub const SETTINGS_VERSION: u32 = 1;

/// Carte son ouverte en duplex (une seule carte pour l'entrée et la sortie, spec §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplexConfig {
    /// `"asio" | "wasapi" | "coreaudio" | "alsa"`.
    pub host: String,
    /// Identiques sauf en WASAPI, où l'entrée et la sortie d'une carte ont deux noms.
    pub input_device: String,
    pub output_device: String,
    /// Canal d'entrée de la guitare (mono), 0-based.
    pub input_channel: u16,
    /// Paire de sortie, 0-based (0 = sorties 1–2).
    pub output_pair: u16,
    pub sample_rate: u32,
    /// Taille de buffer demandée, en frames (ignorée en WASAPI : buffer système).
    pub buffer: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioSettings {
    pub schema_version: u32,
    /// `None` : jamais configuré → sortie seule.
    pub duplex: Option<DuplexConfig>,
    /// `false` : l'utilisateur a arrêté la carte (« Arrêter »), sortie seule.
    pub engine_enabled: bool,
    /// 0-100, 80 = 0 dB.
    pub guitar_fader: f32,
    pub guitar_muted: bool,
    /// Plugin d'ampli choisi (gardé même s'il est introuvable au démarrage : une réinstallation
    /// le retrouve).
    pub plugin: Option<PluginInfo>,
    /// Dossiers VST3 ajoutés par l'utilisateur, en plus des dossiers standard.
    pub vst3_folders: Vec<String>,
    /// Dernier résultat de scan (`None` : jamais scanné).
    pub scan: Option<Vec<PluginInfo>>,
    /// Rechargement du plugin mémorisé en cours au démarrage. Encore vrai au lancement suivant :
    /// ce plugin a fait planter l'app en se chargeant, il n'est pas rechargé (sinon l'app
    /// planterait à chaque lancement).
    pub plugin_loading: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            schema_version: SETTINGS_VERSION,
            duplex: None,
            engine_enabled: true,
            guitar_fader: 80.0,
            guitar_muted: false,
            plugin: None,
            vst3_folders: Vec::new(),
            scan: None,
            plugin_loading: false,
        }
    }
}

/// Fichier des réglages : `<app_dir>/audio/settings.json` (dossier créé au besoin). Hors de
/// l'app dir lui-même, où chaque `.json` est un manifeste de méthode. Un ancien
/// `<app_dir>/audio-settings.json` y est déplacé s'il n'y a pas encore de réglages.
pub fn settings_path(app_dir: &Path) -> std::path::PathBuf {
    let dir = app_dir.join("audio");
    let path = dir.join("settings.json");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("audio settings: create {}: {e}", dir.display());
    }
    let legacy = app_dir.join("audio-settings.json");
    if legacy.exists() && !path.exists() {
        if let Err(e) = std::fs::rename(&legacy, &path) {
            eprintln!("audio settings: move {}: {e}", legacy.display());
        }
    }
    path
}

/// Absent, illisible ou écrit par une version plus récente → réglages par défaut, sans toucher au
/// fichier (même politique que le manifeste).
pub fn load_settings(path: &Path) -> AudioSettings {
    let read = || -> anyhow::Result<AudioSettings> {
        let bytes = std::fs::read(path)?;
        let settings: AudioSettings = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            settings.schema_version <= SETTINGS_VERSION,
            "schema v{} is newer than supported v{SETTINGS_VERSION}",
            settings.schema_version
        );
        Ok(settings)
    };
    match read() {
        Ok(settings) => settings,
        Err(e) => {
            if path.exists() {
                eprintln!("audio settings: ignoring {}: {e:#}", path.display());
            }
            AudioSettings::default()
        }
    }
}

/// Écriture atomique : un plantage en cours d'écriture laisse l'ancien fichier intact.
pub fn save_settings(path: &Path, s: &AudioSettings) -> anyhow::Result<()> {
    let settings = AudioSettings {
        schema_version: SETTINGS_VERSION,
        ..s.clone()
    };
    let tmp = path.with_extension("json.tmp");
    let mut file =
        std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    file.write_all(&serde_json::to_vec_pretty(&settings)?)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

/// Latence aller-retour estimée (buffer d'entrée + buffer de sortie + latence du plugin, en
/// échantillons), en ms.
pub fn latency_ms(buffer: u32, sample_rate: u32, plugin_latency: u32) -> f64 {
    (2.0 * buffer as f64 + plugin_latency as f64) / sample_rate as f64 * 1000.0
}

/// cpal (ASIO) répond « Sample rate N Hz is not supported » quand le pilote est tenu par une
/// autre application, même si N est supporté (spike, RESULTATS.md) : le dire clairement. Le
/// pilote occupé répond aussi « non » à toutes les fréquences de l'inventaire, d'où une liste
/// vide dans ce cas.
pub fn explain_open_error(message: &str, requested_rate: u32, supported_rates: &[u32]) -> String {
    let busy = supported_rates.is_empty() || supported_rates.contains(&requested_rate);
    if message.contains("is not supported") && busy {
        "Pilote audio déjà utilisé par une autre application".to_string()
    } else {
        message.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_path() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "chelou-audio-settings-{}-{}.json",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn temp_app_dir() -> PathBuf {
        let dir = temp_path().with_extension("dir");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_live_in_the_audio_dir() {
        let app = temp_app_dir();
        let path = settings_path(&app);
        assert_eq!(path, app.join("audio").join("settings.json"));
        assert!(path.parent().unwrap().is_dir());
        std::fs::remove_dir_all(app).unwrap();
    }

    #[test]
    fn legacy_settings_file_is_moved_out_of_the_manifest_dir() {
        // Les manifestes de méthodes sont tous les `.json` de l'app dir : les réglages audio
        // n'ont rien à y faire (ils y étaient lus comme un manifeste invalide).
        let app = temp_app_dir();
        std::fs::write(app.join("audio-settings.json"), r#"{"guitarFader":42}"#).unwrap();
        let path = settings_path(&app);
        assert!(!app.join("audio-settings.json").exists());
        assert_eq!(load_settings(&path).guitar_fader, 42.0);
        std::fs::remove_dir_all(app).unwrap();
    }

    #[test]
    fn legacy_file_never_overwrites_current_settings() {
        let app = temp_app_dir();
        std::fs::create_dir_all(app.join("audio")).unwrap();
        std::fs::write(app.join("audio/settings.json"), r#"{"guitarFader":10}"#).unwrap();
        std::fs::write(app.join("audio-settings.json"), r#"{"guitarFader":42}"#).unwrap();
        assert_eq!(load_settings(&settings_path(&app)).guitar_fader, 10.0);
        std::fs::remove_dir_all(app).unwrap();
    }

    #[test]
    fn missing_file_gives_defaults() {
        let s = load_settings(&temp_path());
        assert_eq!(s, AudioSettings::default());
        assert!(s.duplex.is_none());
        assert!(s.engine_enabled);
        assert_eq!(s.guitar_fader, 80.0);
        assert!(!s.guitar_muted);
        assert_eq!(s.schema_version, SETTINGS_VERSION);
    }

    #[test]
    fn roundtrip_keeps_every_field() {
        let path = temp_path();
        let s = AudioSettings {
            schema_version: SETTINGS_VERSION,
            duplex: Some(DuplexConfig {
                host: "asio".into(),
                input_device: "Focusrite USB ASIO".into(),
                output_device: "Focusrite USB ASIO".into(),
                input_channel: 1,
                output_pair: 0,
                sample_rate: 48000,
                buffer: 128,
            }),
            engine_enabled: false,
            guitar_fader: 62.5,
            guitar_muted: true,
            plugin: Some(amp()),
            vst3_folders: vec![r"D:\Audio\VST3".into()],
            scan: Some(vec![amp()]),
            plugin_loading: true,
        };
        save_settings(&path, &s).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("\"inputChannel\""), "camelCase : {json}");
        assert_eq!(load_settings(&path), s);
        std::fs::remove_file(path).unwrap();
    }

    fn amp() -> PluginInfo {
        PluginInfo {
            id: "5653545448504C73746C20746F6E6568".into(),
            name: "STL ToneHub".into(),
            vendor: "STL Tones".into(),
            path: r"C:\Program Files\Common Files\VST3\STL ToneHub.vst3".into(),
            file: "STL ToneHub.vst3".into(),
        }
    }

    #[test]
    fn v1_file_without_plugin_fields_loads() {
        let path = temp_path();
        std::fs::write(
            &path,
            r#"{"schemaVersion":1,"duplex":null,"engineEnabled":true,"guitarFader":70,"guitarMuted":false}"#,
        )
        .unwrap();
        let s = load_settings(&path);
        assert_eq!(s.guitar_fader, 70.0);
        assert_eq!(s.plugin, None);
        assert_eq!(s.scan, None);
        assert!(s.vst3_folders.is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn newer_schema_is_ignored_and_left_untouched() {
        let path = temp_path();
        let content =
            r#"{"schemaVersion":99,"engineEnabled":false,"guitarFader":10,"guitarMuted":true}"#;
        std::fs::write(&path, content).unwrap();
        assert_eq!(load_settings(&path), AudioSettings::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let path = temp_path();
        std::fs::write(&path, "{ pas du json").unwrap();
        assert_eq!(load_settings(&path), AudioSettings::default());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn latency_at_128_48k() {
        assert!((latency_ms(128, 48000, 0) - 5.333).abs() < 1e-3);
    }

    #[test]
    fn latency_includes_plugin() {
        assert!((latency_ms(128, 48000, 106) - 7.542).abs() < 1e-3);
    }

    #[test]
    fn busy_driver_message() {
        let msg = "The requested stream configuration is not supported by the device: Sample rate 48000 Hz is not supported";
        assert_eq!(
            explain_open_error(msg, 48000, &[44100, 48000]),
            "Pilote audio déjà utilisé par une autre application"
        );
    }

    #[test]
    fn busy_driver_reports_no_rate_at_all() {
        // Pilote tenu ailleurs : cpal ne voit aucune fréquence supportée (même appel ASIO).
        let msg = "Sample rate 48000 Hz is not supported";
        assert_eq!(
            explain_open_error(msg, 48000, &[]),
            "Pilote audio déjà utilisé par une autre application"
        );
    }

    #[test]
    fn real_unsupported_rate_keeps_message() {
        let msg = "Sample rate 96000 Hz is not supported";
        assert_eq!(explain_open_error(msg, 96000, &[44100, 48000]), msg);
    }
}
