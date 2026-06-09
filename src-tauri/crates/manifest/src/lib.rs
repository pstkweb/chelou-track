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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gp: Option<FileRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        let Ok(entries) = std::fs::read_dir(&self.base_dir) else {
            return Ok(methods);
        };
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
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_method() -> Method {
        Method {
            id: "test-method".into(),
            title: "Test Method".into(),
            source: MethodSource {
                provider: "pcloud".into(),
                root_folder_id: 123456789,
            },
            default_count_in_bars: 1,
            lessons: vec![Lesson {
                id: "lesson-1".into(),
                order: 0,
                title: "Lesson 1".into(),
                videos: vec![FileRef {
                    file_id: 1,
                    name: "video.mp4".into(),
                }],
                tabs: vec![TabSet {
                    id: "tab-1".into(),
                    title: "Tab 1".into(),
                    gp: Some(FileRef {
                        file_id: 2,
                        name: "tab.gp".into(),
                    }),
                    gpx: None,
                }],
                backing_groups: vec![BackingGroup {
                    label: "partie distorsion".into(),
                    tracks: vec![BackingTrack {
                        audio: FileRef {
                            file_id: 3,
                            name: "Backing track partie distorsion (120bpm).wav".into(),
                        },
                        bpm: 120,
                        lead_in_ms_override: None,
                        sync_points: None,
                    }],
                }],
            }],
            documents: vec![DocumentRef {
                file: FileRef {
                    file_id: 4,
                    name: "sheet.pdf".into(),
                },
                kind: DocKind::Pdf,
                title: "Sheet".into(),
            }],
        }
    }

    /// Verifies that the JSON field names match what the TypeScript side expects.
    /// Any change to serde attributes that breaks camelCase will fail here.
    #[test]
    fn serializes_camelcase_for_ts() {
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&sample_method()).unwrap()).unwrap();

        assert!(
            v.get("defaultCountInBars").is_some(),
            "expected defaultCountInBars"
        );
        assert!(
            v.get("default_count_in_bars").is_none(),
            "snake_case must not leak"
        );
        assert!(
            v["source"].get("rootFolderId").is_some(),
            "expected rootFolderId"
        );

        let l0 = &v["lessons"][0];
        assert!(l0.get("backingGroups").is_some(), "expected backingGroups");
        assert!(l0.get("backing_groups").is_none());

        let tab0 = &l0["tabs"][0];
        assert!(tab0.get("gp").is_some(), "expected gp");
        assert!(tab0.get("gpx").is_none(), "gpx must be absent when None");

        let track0 = &l0["backingGroups"][0]["tracks"][0];
        assert!(track0["audio"].get("fileId").is_some(), "expected fileId");
        assert!(
            track0.get("leadInMsOverride").is_none(),
            "leadInMsOverride must be absent when None"
        );
        assert!(
            track0.get("syncPoints").is_none(),
            "syncPoints must be absent when None"
        );

        assert_eq!(v["documents"][0]["kind"], "pdf");
    }

    /// Roundtrip: serialize then deserialize, values must be preserved.
    #[test]
    fn roundtrip() {
        let original = sample_method();
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Method = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.default_count_in_bars, 1);
        assert_eq!(parsed.lessons.len(), 1);
        assert_eq!(parsed.lessons[0].backing_groups[0].tracks[0].bpm, 120);
        assert!(parsed.lessons[0].tabs[0].gp.is_some());
        assert!(parsed.lessons[0].tabs[0].gpx.is_none());
    }
}
