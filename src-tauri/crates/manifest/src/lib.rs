// Local JSON manifest — one file per Method in Tauri's app-data dir.
// Mirrors src/types/model.ts (serde renames snake_case → camelCase for the TS boundary).
// cf. ARCHITECTURE.md §8 + §9.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Version of the on-disk manifest format, bumped on any breaking change to `Method`'s shape.
/// Files written before versioning existed have no field and deserialize as 0.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Method {
    /// Always rewritten to [`SCHEMA_VERSION`] by [`ManifestStore::save`].
    #[serde(rename = "schemaVersion", default)]
    pub schema_version: u32,
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

/// Finds a `Lesson` by id anywhere in the (possibly nested) items tree.
fn find_lesson_mut<'a>(items: &'a mut [SectionItem], lesson_id: &str) -> Option<&'a mut Lesson> {
    for item in items {
        match item {
            SectionItem::Lesson(l) if l.id == lesson_id => return Some(l),
            SectionItem::Section(s) => {
                if let Some(l) = find_lesson_mut(&mut s.items, lesson_id) {
                    return Some(l);
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodSource {
    pub provider: String,
    #[serde(rename = "rootFolderId")]
    pub root_folder_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    /// Global DFS viewing order across the whole method (1-based).
    pub order: u32,
    pub title: String,
    pub video: FileRef,
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
    /// Final lead-in offset (ms) used to anchor tablature sync to this track's real start.
    /// Populated automatically by TabScreen's background detection the first time a track
    /// is selected, or manually if a track needs correcting. Once set, nothing recomputes it.
    #[serde(rename = "leadInMsOverride", skip_serializing_if = "Option::is_none")]
    pub lead_in_ms_override: Option<f64>,
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
    pub file_id: String,
    pub name: String,
}

// --- Persistence ---

pub struct ManifestStore {
    base_dir: PathBuf,
    /// Serialises writers: `update_method` is a read-modify-write, and concurrent commands
    /// (periodic resume updates + lead-in caching) would otherwise lose each other's changes.
    write_lock: Mutex<()>,
}

impl ManifestStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            write_lock: Mutex::new(()),
        }
    }

    /// Load every readable manifest. A file that can't be read or parsed (corrupted, or written
    /// by a newer app version) is skipped and reported on stderr rather than failing the whole
    /// library — and left untouched on disk, so a newer version can still read it.
    pub fn load_all(&self) -> Result<Vec<Method>> {
        let mut methods = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.base_dir) else {
            return Ok(methods);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match read_method(&path) {
                Ok(m) => methods.push(m),
                Err(e) => eprintln!("manifest: skipping {}: {e:#}", path.display()),
            }
        }
        methods.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(methods)
    }

    pub fn save(&self, method: &Method) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut method = method.clone();
        method.schema_version = SCHEMA_VERSION;
        write_atomic(&self.path_for(&method.id)?, &method)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.path_for(id)?;
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// `{base_dir}/{id}.json`, refusing any id that could escape `base_dir` — ids come from
    /// the WebView (uuid v4 in practice, cf. scanner).
    fn path_for(&self, id: &str) -> Result<PathBuf> {
        if !is_valid_id(id) {
            bail!("invalid method id: {id:?}");
        }
        Ok(self.base_dir.join(format!("{id}.json")))
    }

    /// Mark a lesson as seen and clear any resume position (lesson watched to completion).
    pub fn mark_lesson_seen(&self, method_id: &str, lesson_id: &str) -> Result<()> {
        self.update_method(method_id, |m| {
            m.progress
                .entry(lesson_id.to_owned())
                .or_default()
                .resume_ms = None;
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

    /// Cache the final lead-in offset (ms) for a backing track, computed by the leading-
    /// silence detection (real beatsPerBar from AlphaTab, done in TabScreen.tsx). No-op if
    /// the lesson or the track's file_id isn't found — never errors for that reason, only
    /// for I/O failures.
    pub fn update_backing_track_lead_in_override(
        &self,
        method_id: &str,
        lesson_id: &str,
        file_id: String,
        lead_in_ms: f64,
    ) -> Result<()> {
        self.update_method(method_id, |m| {
            if let Some(lesson) = find_lesson_mut(&mut m.items, lesson_id) {
                for group in &mut lesson.backing_groups {
                    for track in &mut group.tracks {
                        if track.audio.file_id == file_id {
                            track.lead_in_ms_override = Some(lead_in_ms);
                        }
                    }
                }
            }
        })
    }

    fn update_method(&self, id: &str, f: impl FnOnce(&mut Method)) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.path_for(id)?;
        let mut method = read_method(&path)?;
        f(&mut method);
        method.schema_version = SCHEMA_VERSION;
        write_atomic(&path, &method)
    }
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn read_method(path: &std::path::Path) -> Result<Method> {
    let json = std::fs::read_to_string(path)?;
    let method: Method = serde_json::from_str(&json).context("invalid manifest JSON")?;
    if method.schema_version > SCHEMA_VERSION {
        bail!(
            "manifest schema v{} is newer than supported v{SCHEMA_VERSION} (app downgraded?)",
            method.schema_version
        );
    }
    Ok(method)
}

/// Write to a sibling temp file, fsync, then rename over the target: a crash or full disk
/// mid-write leaves the previous manifest intact instead of a truncated one. `rename` replaces
/// an existing file atomically on every target OS (MoveFileEx REPLACE_EXISTING on Windows).
fn write_atomic(path: &std::path::Path, method: &Method) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(serde_json::to_string_pretty(method)?.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    Ok(())
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
            video: FileRef {
                file_id: "1".into(),
                name: "video.mp4".into(),
            },
            tabs: vec![TabSet {
                id: "tab-1".into(),
                title: "Tab 1".into(),
                files: vec![TabFile {
                    ext: "gp".into(),
                    file: FileRef {
                        file_id: "2".into(),
                        name: "tab.gp".into(),
                    },
                }],
            }],
            backing_groups: vec![BackingGroup {
                label: "partie distorsion".into(),
                tracks: vec![BackingTrack {
                    audio: FileRef {
                        file_id: "3".into(),
                        name: "Backing track partie distorsion (120bpm).wav".into(),
                    },
                    bpm: 120,
                    lead_in_ms_override: None,
                }],
            }],
        }
    }

    fn sample_method() -> Method {
        Method {
            schema_version: SCHEMA_VERSION,
            id: "test-method".into(),
            title: "Test Method".into(),
            source: MethodSource {
                provider: "pcloud".into(),
                root_folder_id: "123456789".into(),
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
                    file_id: "4".into(),
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
        assert_eq!(v["schemaVersion"], SCHEMA_VERSION, "expected schemaVersion");
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

    #[test]
    fn update_backing_track_lead_in_override_updates_top_level_lesson() {
        let dir = std::env::temp_dir().join(format!(
            "chelou-manifest-test-toplevel-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ManifestStore::new(dir.clone());
        store.save(&sample_method()).unwrap();

        store
            .update_backing_track_lead_in_override(
                "test-method",
                "lesson-1",
                "3".to_owned(),
                1490.0,
            )
            .unwrap();

        let reloaded = store.load_all().unwrap();
        let loaded = reloaded.iter().find(|m| m.id == "test-method").unwrap();
        let SectionItem::Lesson(lesson) = &loaded.items[0] else {
            panic!("expected lesson at index 0");
        };
        assert_eq!(
            lesson.backing_groups[0].tracks[0].lead_in_ms_override,
            Some(1490.0)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_backing_track_lead_in_override_updates_lesson_nested_in_section() {
        let dir = std::env::temp_dir().join(format!(
            "chelou-manifest-test-nested-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ManifestStore::new(dir.clone());
        store.save(&sample_method()).unwrap();

        // lesson-2 lives inside the "CHAP 1 Intro" section — exercises the recursive lookup.
        store
            .update_backing_track_lead_in_override(
                "test-method",
                "lesson-2",
                "3".to_owned(),
                2990.0,
            )
            .unwrap();

        let reloaded = store.load_all().unwrap();
        let loaded = reloaded.iter().find(|m| m.id == "test-method").unwrap();
        let SectionItem::Section(section) = &loaded.items[1] else {
            panic!("expected section at index 1");
        };
        let SectionItem::Lesson(lesson) = &section.items[0] else {
            panic!("expected lesson inside section");
        };
        assert_eq!(
            lesson.backing_groups[0].tracks[0].lead_in_ms_override,
            Some(2990.0)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_backing_track_lead_in_override_is_noop_for_unknown_lesson() {
        let dir = std::env::temp_dir().join(format!(
            "chelou-manifest-test-unknown-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ManifestStore::new(dir.clone());
        store.save(&sample_method()).unwrap();

        // Must not error even though "does-not-exist" isn't a real lesson id.
        store
            .update_backing_track_lead_in_override(
                "test-method",
                "does-not-exist",
                "3".to_owned(),
                999.0,
            )
            .unwrap();

        let reloaded = store.load_all().unwrap();
        let loaded = reloaded.iter().find(|m| m.id == "test-method").unwrap();
        let SectionItem::Lesson(lesson) = &loaded.items[0] else {
            panic!("expected lesson at index 0");
        };
        assert_eq!(lesson.backing_groups[0].tracks[0].lead_in_ms_override, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn temp_store(name: &str) -> (PathBuf, ManifestStore) {
        let dir = std::env::temp_dir().join(format!(
            "chelou-manifest-test-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (dir.clone(), ManifestStore::new(dir))
    }

    #[test]
    fn rejects_ids_that_could_escape_the_manifest_dir() {
        let (dir, store) = temp_store("ids");
        for id in ["../evil", "a/b", "a\\b", "", ".."] {
            assert!(store.delete(id).is_err(), "{id:?}");
            let mut m = sample_method();
            m.id = id.to_owned();
            assert!(store.save(&m).is_err(), "{id:?}");
        }
        assert!(store.delete("3f2b1c4e-0000-4000-8000-000000000000").is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_all_skips_corrupt_and_newer_files_but_keeps_the_rest() {
        let (dir, store) = temp_store("corrupt");
        store.save(&sample_method()).unwrap();
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();
        let mut future = serde_json::to_value(sample_method()).unwrap();
        future["id"] = "future".into();
        future["schemaVersion"] = (SCHEMA_VERSION + 1).into();
        std::fs::write(dir.join("future.json"), future.to_string()).unwrap();

        let loaded = store.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "test-method");
        // Unreadable files are left on disk untouched.
        assert!(dir.join("broken.json").exists());
        assert!(dir.join("future.json").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_file_without_schema_version_loads_and_is_upgraded_on_write() {
        let (dir, store) = temp_store("legacy");
        let mut legacy = serde_json::to_value(sample_method()).unwrap();
        legacy.as_object_mut().unwrap().remove("schemaVersion");
        std::fs::write(dir.join("test-method.json"), legacy.to_string()).unwrap();

        assert_eq!(store.load_all().unwrap()[0].schema_version, 0);
        store.mark_lesson_seen("test-method", "lesson-1").unwrap();
        assert_eq!(store.load_all().unwrap()[0].schema_version, SCHEMA_VERSION);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let (dir, store) = temp_store("atomic");
        store.save(&sample_method()).unwrap();
        store.save(&sample_method()).unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, vec!["test-method.json".to_owned()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
