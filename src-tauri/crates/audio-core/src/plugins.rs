// Scan des plugins VST3 (spec §5.6) : logique pure, sans charger aucun binaire. Le chargement
// d'un module (bundles sans `moduleinfo.json`) se fait dans un processus enfant, côté
// `chelou-vst3-host`.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Classe exportée par un module VST3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassInfo {
    /// 32 caractères hex majuscules (octets du `TUID`, ou `CID` de `moduleinfo.json` tel quel).
    pub cid: String,
    pub category: String,
    pub name: String,
    pub vendor: String,
    pub sub_categories: Vec<String>,
}

/// Plugin proposé à l'utilisateur ; le WebView le désigne par `id` (jamais par son chemin).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    /// `cid` de la classe audio.
    pub id: String,
    pub name: String,
    pub vendor: String,
    /// Chemin du bundle `.vst3`.
    pub path: String,
    /// Nom de fichier du bundle.
    pub file: String,
}

const AUDIO_CLASS: &str = "Audio Module Class";

/// `moduleinfo.json` (JSON5 : les fichiers réels ont des virgules finales).
pub fn parse_module_info(text: &str) -> anyhow::Result<Vec<ClassInfo>> {
    let info: Value = json5::from_str(text).context("moduleinfo.json illisible")?;
    let text_of =
        |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let factory_vendor = info
        .get("Factory Info")
        .map(|f| text_of(f, "Vendor"))
        .unwrap_or_default();
    let classes = info
        .get("Classes")
        .and_then(Value::as_array)
        .context("moduleinfo.json sans « Classes »")?;
    Ok(classes
        .iter()
        .map(|c| {
            let vendor = text_of(c, "Vendor");
            ClassInfo {
                cid: text_of(c, "CID").to_uppercase(),
                category: text_of(c, "Category"),
                name: text_of(c, "Name"),
                vendor: if vendor.is_empty() {
                    factory_vendor.clone()
                } else {
                    vendor
                },
                sub_categories: c
                    .get("Sub Categories")
                    .and_then(Value::as_array)
                    .map(|subs| {
                        subs.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect())
}

/// Classes audio, sauf les instruments purs : un ampli mal étiqueté ne doit jamais disparaître.
pub fn is_listed(class: &ClassInfo) -> bool {
    let has = |sub: &str| class.sub_categories.iter().any(|s| s == sub);
    class.category == AUDIO_CLASS && !(has("Instrument") && !has("Fx"))
}

pub fn plugins_of(bundle: &Path, classes: &[ClassInfo]) -> Vec<PluginInfo> {
    let file = bundle
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    classes
        .iter()
        .filter(|c| is_listed(c))
        .map(|c| PluginInfo {
            id: c.cid.clone(),
            name: c.name.clone(),
            vendor: c.vendor.clone(),
            path: bundle.to_string_lossy().into_owned(),
            file: file.clone(),
        })
        .collect()
}

fn is_vst3(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("vst3"))
}

/// Entrées `*.vst3` (DLL à plat ou dossier bundle) sous `folders`, sans descendre dans un bundle.
pub fn find_bundles(folders: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_vst3(&path) {
                out.push(path);
            } else if path.is_dir() {
                walk(&path, out);
            }
        }
    }
    let mut out = Vec::new();
    for folder in folders {
        walk(folder, &mut out);
    }
    out.sort();
    out
}

pub fn module_info_path(bundle: &Path) -> Option<PathBuf> {
    let path = bundle
        .join("Contents")
        .join("Resources")
        .join("moduleinfo.json");
    path.is_file().then_some(path)
}

/// Binaire à charger : le fichier lui-même (DLL à plat), sinon celui du bundle pour cet OS.
pub fn binary_path(bundle: &Path) -> Option<PathBuf> {
    if bundle.is_file() {
        return Some(bundle.to_path_buf());
    }
    let contents = bundle.join("Contents");
    let stem = bundle.file_stem()?;
    let inner = if cfg!(windows) {
        contents.join("x86_64-win").join(bundle.file_name()?)
    } else if cfg!(target_os = "macos") {
        contents.join("MacOS").join(stem)
    } else {
        let mut name = stem.to_os_string();
        name.push(".so");
        contents.join("x86_64-linux").join(name)
    };
    inner.is_file().then_some(inner)
}

/// Dossiers standard de l'OS (spec §5.6).
pub fn standard_vst3_folders() -> Vec<PathBuf> {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
    };
    if cfg!(windows) {
        vec![PathBuf::from(r"C:\Program Files\Common Files\VST3")]
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/Library/Audio/Plug-Ins/VST3"),
            home().join("Library/Audio/Plug-Ins/VST3"),
        ]
    } else {
        vec![
            home().join(".vst3"),
            PathBuf::from("/usr/lib/vst3"),
            PathBuf::from("/usr/local/lib/vst3"),
        ]
    }
}

/// Un même plugin trouvé deux fois : le premier gagne (dossiers standard d'abord) ; tri par nom.
pub fn dedupe(plugins: Vec<PluginInfo>) -> Vec<PluginInfo> {
    let mut out: Vec<PluginInfo> = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        if !out.iter().any(|p| p.id == plugin.id) {
            out.push(plugin);
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn class(category: &str, subs: &[&str]) -> ClassInfo {
        ClassInfo {
            cid: "00".repeat(16),
            category: category.into(),
            name: "X".into(),
            vendor: "V".into(),
            sub_categories: subs.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chelou-plugins-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_tonehub_module_info() {
        let text = include_str!("../tests/data/moduleinfo-tonehub.json");
        let classes = parse_module_info(text).unwrap();
        assert_eq!(classes.len(), 3);
        assert_eq!(classes[0].cid, "5653545448504C73746C20746F6E6568");
        assert_eq!(classes[0].category, "Audio Module Class");
        assert_eq!(classes[0].name, "STL ToneHub");
        assert_eq!(classes[0].vendor, "STL Tones");
        assert_eq!(classes[0].sub_categories, vec!["Fx".to_string()]);
    }

    #[test]
    fn class_vendor_falls_back_to_factory_vendor() {
        let text = r#"{ "Factory Info": { "Vendor": "Acme", }, "Classes": [
            { "CID": "AB", "Category": "Audio Module Class", "Name": "Amp", "Vendor": "", }, ], }"#;
        assert_eq!(parse_module_info(text).unwrap()[0].vendor, "Acme");
    }

    #[test]
    fn lists_effects_and_mislabelled_amps() {
        assert!(is_listed(&class(
            "Audio Module Class",
            &["Fx", "Distortion"]
        )));
        assert!(is_listed(&class("Audio Module Class", &[])));
        assert!(!is_listed(&class(
            "Audio Module Class",
            &["Instrument", "Synth"]
        )));
        assert!(is_listed(&class(
            "Audio Module Class",
            &["Instrument", "Fx"]
        )));
        assert!(!is_listed(&class("Component Controller Class", &["Fx"])));
        assert!(!is_listed(&class("Plugin Compatibility Class", &[])));
    }

    #[test]
    fn plugins_of_keeps_listed_classes() {
        let bundle = Path::new("/vst3/Amp.vst3");
        let list = plugins_of(
            bundle,
            &[
                class("Audio Module Class", &["Fx"]),
                class("Component Controller Class", &[]),
            ],
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].file, "Amp.vst3");
        assert_eq!(list[0].path, bundle.to_string_lossy());
        assert_eq!(list[0].id, "00".repeat(16));
    }

    #[test]
    fn finds_flat_and_bundled_plugins() {
        let dir = temp_dir("find");
        fs::write(dir.join("A.vst3"), b"").unwrap();
        fs::create_dir_all(dir.join("Sub/B.vst3/Contents/x")).unwrap();
        fs::write(dir.join("Sub/B.vst3/Contents/x/C.vst3"), b"").unwrap();
        fs::write(dir.join("notes.txt"), b"").unwrap();
        let found = find_bundles(std::slice::from_ref(&dir));
        assert_eq!(
            found,
            vec![dir.join("A.vst3"), dir.join("Sub").join("B.vst3")]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_folder_is_ignored() {
        assert!(find_bundles(&[PathBuf::from("/definitely/not/here/vst3")]).is_empty());
    }

    #[test]
    fn binary_of_flat_and_bundle() {
        let dir = temp_dir("binary");
        let flat = dir.join("Flat.vst3");
        fs::write(&flat, b"").unwrap();
        assert_eq!(binary_path(&flat), Some(flat.clone()));

        let bundle = dir.join("Amp.vst3");
        let inner = if cfg!(windows) {
            bundle.join("Contents/x86_64-win/Amp.vst3")
        } else if cfg!(target_os = "macos") {
            bundle.join("Contents/MacOS/Amp")
        } else {
            bundle.join("Contents/x86_64-linux/Amp.so")
        };
        fs::create_dir_all(inner.parent().unwrap()).unwrap();
        fs::write(&inner, b"").unwrap();
        assert_eq!(binary_path(&bundle), Some(inner));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_binary_is_reported() {
        let dir = temp_dir("missing");
        let bundle = dir.join("Gone.vst3");
        fs::create_dir_all(bundle.join("Contents/Resources")).unwrap();
        assert_eq!(binary_path(&bundle), None);
        assert_eq!(binary_path(&dir.join("Nothing.vst3")), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn module_info_is_found_in_resources() {
        let dir = temp_dir("info");
        let bundle = dir.join("Amp.vst3");
        assert_eq!(module_info_path(&bundle), None);
        fs::create_dir_all(bundle.join("Contents/Resources")).unwrap();
        fs::write(bundle.join("Contents/Resources/moduleinfo.json"), b"{}").unwrap();
        assert_eq!(
            module_info_path(&bundle),
            Some(bundle.join("Contents/Resources/moduleinfo.json"))
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dedupe_keeps_first_and_sorts() {
        let p = |id: &str, name: &str, path: &str| PluginInfo {
            id: id.into(),
            name: name.into(),
            vendor: "V".into(),
            path: path.into(),
            file: "f".into(),
        };
        let list = dedupe(vec![
            p("1", "zeta", "std"),
            p("2", "Alpha", "a"),
            p("1", "zeta", "custom"),
        ]);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Alpha");
        assert_eq!(list[1].path, "std");
    }

    #[test]
    fn standard_folders_match_the_platform() {
        let folders = standard_vst3_folders();
        if cfg!(windows) {
            assert_eq!(
                folders,
                vec![PathBuf::from(r"C:\Program Files\Common Files\VST3")]
            );
        } else {
            assert!(!folders.is_empty());
        }
    }
}
