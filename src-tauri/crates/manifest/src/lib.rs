// Local JSON manifest — one file per Method in Tauri's app-data dir.
// Mirrors src/types/model.ts (serde renames snake_case → camelCase for the TS boundary).
// cf. ARCHITECTURE.md §8 + §9.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Method {
    pub id: String,
    pub title: String,
    pub source: MethodSource,
    #[serde(rename = "defaultCountInBars")]
    pub default_count_in_bars: u32,
    /// Ordered mix of lessons and sub-sections at the method root, in DFS natural sort.
    /// Use this to drive the catalogue and "next lesson" navigation.
    pub items: Vec<SectionItem>,
    pub documents: Vec<DocumentRef>,
    /// Per-lesson progress keyed by lesson `id`.
    /// Presence of an entry means the lesson has been seen at least once.
    #[serde(default)]
    pub progress: HashMap<String, LessonProgress>,
}

/// Viewing progress for a single lesson.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LessonProgress {
    /// Playback position in ms where the user stopped mid-video.
    /// Absent when the lesson was watched to completion or just marked seen manually.
    #[serde(rename = "resumeMs", skip_serializing_if = "Option::is_none")]
    pub resume_ms: Option<f64>,
}

impl Method {
    pub fn has_lessons(&self) -> bool {
        items_have_lessons(&self.items)
    }
}

/// One item inside a folder: either a video lesson or a structural sub-folder.
/// The `type` discriminant is serialized as `"lesson"` or `"section"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SectionItem {
    Lesson(Lesson),
    Section(Section),
}

/// A structural folder (chapter, episode, part…).  Arbitrarily nestable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    /// Ordered mix of lessons and sub-sections inside this folder.
    pub items: Vec<SectionItem>,
    pub documents: Vec<DocumentRef>,
}

fn items_have_lessons(items: &[SectionItem]) -> bool {
    items.iter().any(|item| match item {
        SectionItem::Lesson(_) => true,
        SectionItem::Section(s) => items_have_lessons(&s.items),
    })
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
    /// Global DFS viewing order across the whole method (1-based).
    pub order: u32,
    pub title: String,
    pub videos: Vec<FileRef>,
    pub tabs: Vec<TabSet>,
    #[serde(rename = "backingGroups")]
    pub backing_groups: Vec<BackingGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabFile {
    pub ext: String,
    pub file: FileRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSet {
    pub id: String,
    pub title: String,
    pub files: Vec<TabFile>,
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

    /// Mark a lesson as seen (inserts a default entry if not already present).
    pub fn mark_lesson_seen(&self, method_id: &str, lesson_id: &str) -> Result<()> {
        self.update_method(method_id, |m| {
            m.progress.entry(lesson_id.to_owned()).or_default();
        })
    }

    /// Remove a lesson from the progress map (mark as unseen).
    pub fn mark_lesson_unseen(&self, method_id: &str, lesson_id: &str) -> Result<()> {
        self.update_method(method_id, |m| {
            m.progress.remove(lesson_id);
        })
    }

    /// Update the resume timecode for a lesson (sets it as seen too).
    pub fn update_lesson_resume(
        &self,
        method_id: &str,
        lesson_id: &str,
        resume_ms: f64,
    ) -> Result<()> {
        self.update_method(method_id, |m| {
            m.progress
                .entry(lesson_id.to_owned())
                .or_default()
                .resume_ms = Some(resume_ms);
        })
    }

    fn update_method(&self, id: &str, f: impl FnOnce(&mut Method)) -> Result<()> {
        let path = self.base_dir.join(format!("{id}.json"));
        let json = std::fs::read_to_string(&path)?;
        let mut method: Method = serde_json::from_str(&json)?;
        f(&mut method);
        std::fs::write(path, serde_json::to_string_pretty(&method)?)?;
        Ok(())
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_lesson(order: u32) -> Lesson {
        Lesson {
            id: format!("lesson-{order}"),
            order,
            title: format!("Lesson {order}"),
            videos: vec![FileRef {
                file_id: 1,
                name: "video.mp4".into(),
            }],
            tabs: vec![TabSet {
                id: "tab-1".into(),
                title: "Tab 1".into(),
                files: vec![TabFile {
                    ext: "gp".into(),
                    file: FileRef {
                        file_id: 2,
                        name: "tab.gp".into(),
                    },
                }],
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
        }
    }

    fn sample_method() -> Method {
        Method {
            id: "test-method".into(),
            title: "Test Method".into(),
            source: MethodSource {
                provider: "pcloud".into(),
                root_folder_id: 123456789,
            },
            default_count_in_bars: 1,
            // Root: lesson 1, then CHAP 1 section, then lesson 3 — tests interleaving
            items: vec![
                SectionItem::Lesson(sample_lesson(1)),
                SectionItem::Section(Section {
                    id: "section-1".into(),
                    title: "CHAP 1 Intro".into(),
                    items: vec![SectionItem::Lesson(sample_lesson(2))],
                    documents: vec![],
                }),
                SectionItem::Lesson(sample_lesson(3)),
            ],
            documents: vec![DocumentRef {
                file: FileRef {
                    file_id: 4,
                    name: "sheet.pdf".into(),
                },
                kind: DocKind::Pdf,
                title: "Sheet".into(),
            }],
            progress: HashMap::new(),
        }
    }

    /// Verifies that the JSON field names match what the TypeScript side expects.
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

        let items = v["items"].as_array().expect("items must be an array");
        assert_eq!(
            items.len(),
            3,
            "root must have 3 items (lesson, section, lesson)"
        );

        let lesson_item = &items[0];
        assert_eq!(
            lesson_item["type"], "lesson",
            "first item must be tagged as lesson"
        );
        assert!(
            lesson_item.get("backingGroups").is_some(),
            "expected backingGroups"
        );
        assert!(lesson_item.get("backing_groups").is_none());

        let tab0 = &lesson_item["tabs"][0];
        let files = tab0["files"].as_array().expect("files must be an array");
        assert_eq!(files.len(), 1, "one format in test TabSet");
        assert_eq!(files[0]["ext"], "gp");
        assert!(files[0].get("file").is_some());

        let track0 = &lesson_item["backingGroups"][0]["tracks"][0];
        assert!(track0["audio"].get("fileId").is_some(), "expected fileId");
        assert!(
            track0.get("leadInMsOverride").is_none(),
            "must be absent when None"
        );
        assert!(
            track0.get("syncPoints").is_none(),
            "must be absent when None"
        );

        let section_item = &items[1];
        assert_eq!(
            section_item["type"], "section",
            "second item must be tagged as section"
        );
        assert!(
            section_item.get("items").is_some(),
            "section must have items array"
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
        assert_eq!(parsed.items.len(), 3);

        let SectionItem::Lesson(l1) = &parsed.items[0] else {
            panic!("expected lesson")
        };
        assert_eq!(l1.order, 1);
        assert_eq!(l1.backing_groups[0].tracks[0].bpm, 120);
        assert_eq!(l1.tabs[0].files.len(), 1);
        assert_eq!(l1.tabs[0].files[0].ext, "gp");

        let SectionItem::Section(s) = &parsed.items[1] else {
            panic!("expected section")
        };
        assert_eq!(s.title, "CHAP 1 Intro");
        assert_eq!(s.items.len(), 1);

        let SectionItem::Lesson(l3) = &parsed.items[2] else {
            panic!("expected lesson")
        };
        assert_eq!(l3.order, 3);
    }
}
