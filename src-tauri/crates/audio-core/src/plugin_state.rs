// Son réglé dans le plugin d'ampli (état VST3 composant + contrôleur), gardé sur disque pour le
// retrouver au redémarrage (spec §5.6, §5.9). Le fichier porte l'identifiant du plugin : l'état
// d'un autre plugin n'est jamais appliqué.
//
// Format : `b"CTPS"`, version `u32 LE`, longueur de l'id `u32 LE`, id UTF-8, état.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;

const MAGIC: &[u8; 4] = b"CTPS";
const VERSION: u32 = 1;

/// `<app_dir>/audio/plugin-state.bin` (dossier créé par `settings::settings_path`).
pub fn plugin_state_path(app_dir: &Path) -> PathBuf {
    app_dir.join("audio").join("plugin-state.bin")
}

/// Écriture atomique : un plantage en cours d'écriture laisse l'ancien état intact.
pub fn save_plugin_state(path: &Path, plugin_id: &str, state: &[u8]) -> anyhow::Result<()> {
    let mut bytes = Vec::with_capacity(12 + plugin_id.len() + state.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&(plugin_id.len() as u32).to_le_bytes());
    bytes.extend_from_slice(plugin_id.as_bytes());
    bytes.extend_from_slice(state);
    let tmp = path.with_extension("bin.tmp");
    let mut file =
        std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

fn parse<'a>(bytes: &'a [u8], plugin_id: &str) -> Result<&'a [u8], &'static str> {
    let u32_at = |at: usize| -> Result<u32, &'static str> {
        let b = bytes.get(at..at + 4).ok_or("tronqué")?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    };
    if bytes.get(..4) != Some(MAGIC.as_slice()) {
        return Err("pas un état de plugin");
    }
    if u32_at(4)? != VERSION {
        return Err("version inconnue");
    }
    let id_len = u32_at(8)? as usize;
    let id = bytes.get(12..12 + id_len).ok_or("tronqué")?;
    if id != plugin_id.as_bytes() {
        return Err("état d'un autre plugin");
    }
    Ok(&bytes[12 + id_len..])
}

/// État enregistré pour `plugin_id` ; absent, illisible, tronqué, d'une autre version ou d'un
/// autre plugin → `None`.
pub fn load_plugin_state(path: &Path, plugin_id: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    match parse(&bytes, plugin_id) {
        Ok(state) => Some(state.to_vec()),
        Err(why) => {
            eprintln!("plugin state: ignoring {}: {why}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_path() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "chelou-plugin-state-{}-{}.bin",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    const ID: &str = "01EFCDAB8291EBFA4E4453504154474A";

    #[test]
    fn plugin_state_path_is_in_the_audio_dir() {
        let app = Path::new("/app");
        assert_eq!(
            plugin_state_path(app),
            app.join("audio").join("plugin-state.bin")
        );
    }

    #[test]
    fn plugin_state_roundtrip() {
        let path = temp_path();
        save_plugin_state(&path, ID, &[1, 2, 3, 0, 255]).unwrap();
        assert_eq!(load_plugin_state(&path, ID), Some(vec![1, 2, 3, 0, 255]));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn state_of_another_plugin_is_ignored() {
        let path = temp_path();
        save_plugin_state(&path, ID, &[1, 2, 3]).unwrap();
        assert_eq!(load_plugin_state(&path, "OTHER"), None);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn truncated_state_file_is_ignored() {
        let path = temp_path();
        save_plugin_state(&path, ID, &[1, 2, 3]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // Coupé au milieu de l'id, puis au milieu de l'en-tête.
        std::fs::write(&path, &bytes[..12 + ID.len() / 2]).unwrap();
        assert_eq!(load_plugin_state(&path, ID), None);
        std::fs::write(&path, &bytes[..6]).unwrap();
        assert_eq!(load_plugin_state(&path, ID), None);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_state_file_gives_none() {
        assert_eq!(load_plugin_state(&temp_path(), ID), None);
    }

    #[test]
    fn newer_state_version_is_ignored() {
        let path = temp_path();
        save_plugin_state(&path, ID, &[1]).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[4] = 2;
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(load_plugin_state(&path, ID), None);
        std::fs::remove_file(path).unwrap();
    }
}
