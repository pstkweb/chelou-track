// Local JSON manifest — one file per Method in Tauri's app-data dir.
// Mirrors src/types/model.ts (serde renames snake_case → camelCase for the TS boundary).
// cf. ARCHITECTURE.md §8 + §9.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Method {
    pub id: String,
    pub title: String,
    pub source: MethodSource,
    #[serde(rename = "defaultCountInBars")]
    pub default_count_in_bars: u32,
    pub lessons: Vec<Lesson>,
    pub documents: Vec<DocumentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodSource {
    pub provider: String,
    #[serde(rename = "rootFolderId")]
    pub root_folder_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub order: u32,
    pub title: String,
    pub videos: Vec<FileRef>,
    pub tabs: Vec<TabSet>,
    #[serde(rename = "backingGroups")]
    pub backing_groups: Vec<BackingGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSet {
    pub id: String,
    pub title: String,
    pub gp: Option<FileRef>,
    pub gpx: Option<FileRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackingGroup {
    pub label: String,
    pub tracks: Vec<BackingTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackingTrack {
    pub audio: FileRef,
    pub bpm: u32,
    #[serde(rename = "leadInMsOverride", skip_serializing_if = "Option::is_none")]
    pub lead_in_ms_override: Option<f64>,
    #[serde(rename = "syncPoints", skip_serializing_if = "Option::is_none")]
    pub sync_points: Option<Vec<SyncPoint>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRef {
    pub file: FileRef,
    pub kind: DocKind,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Pdf,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRef {
    #[serde(rename = "fileId")]
    pub file_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPoint {
    #[serde(rename = "audioMs")]
    pub audio_ms: f64,
    pub tick: u32,
}

// --- Persistence ---

pub struct ManifestStore {
    base_dir: PathBuf,
}

impl ManifestStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn load_all(&self) -> Result<Vec<Method>> {
        let mut methods = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.base_dir) else { return Ok(methods) };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let json = std::fs::read_to_string(&path)?;
                if let Ok(m) = serde_json::from_str::<Method>(&json) {
                    methods.push(m);
                }
            }
        }
        methods.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(methods)
    }

    pub fn save(&self, method: &Method) -> Result<()> {
        let path = self.base_dir.join(format!("{}.json", method.id));
        let json = serde_json::to_string_pretty(method)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.base_dir.join(format!("{id}.json"));
        if path.exists() { std::fs::remove_file(path)?; }
        Ok(())
    }
}
