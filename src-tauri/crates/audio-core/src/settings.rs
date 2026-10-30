// Réglages audio persistés (`audio-settings.json` dans l'app-data dir, ARCHITECTURE.md §9) et
// calculs purs affichés par l'écran de réglages.

use std::io::Write;
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

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
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            schema_version: SETTINGS_VERSION,
            duplex: None,
            engine_enabled: true,
            guitar_fader: 80.0,
            guitar_muted: false,
        }
    }
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

/// Latence aller-retour estimée (buffer d'entrée + buffer de sortie), en ms.
pub fn latency_ms(buffer: u32, sample_rate: u32) -> f64 {
    2.0 * buffer as f64 / sample_rate as f64 * 1000.0
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
        };
        save_settings(&path, &s).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("\"inputChannel\""), "camelCase : {json}");
        assert_eq!(load_settings(&path), s);
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
        assert!((latency_ms(128, 48000) - 5.333).abs() < 1e-3);
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
