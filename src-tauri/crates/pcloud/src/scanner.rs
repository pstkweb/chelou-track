// Folder scan — §6 rules: DFS + natural sort, keyword buckets, pool attachment.
// No tab↔backing matching by name (cf. ARCHITECTURE.md §6 + §14).
use anyhow::Result;
use async_recursion::async_recursion;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use uuid::Uuid;

use crate::{Entry, PCloudClient};
use chelou_manifest::{
    BackingGroup, BackingTrack, DocKind, DocumentRef, FileRef, Lesson, Method, MethodSource,
    Section, SectionItem, TabFile, TabSet,
};

/// Progress event emitted during a DFS scan — one per folder entered.
/// Payload of the Tauri "scan:progress" event.
#[derive(Debug, Clone, Serialize)]
pub struct ScanEvent {
    #[serde(rename = "currentFolder")]
    pub current_folder: String,
    #[serde(rename = "foldersVisited")]
    pub folders_visited: u32,
    #[serde(rename = "methodsFound")]
    pub methods_found: u32,
}

static BPM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?<bpm>\d+) ?bpm").unwrap());

// Strips leading numeric/CHAP prefixes from folder names used as section titles.
// Handles: "1 - Titre", "01. Titre", "CHAP 1 Titre", "1 CHAP 1 - Titre", etc.
// Falls back to the original name if stripping would produce an empty string.
static SECTION_PREFIX_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:\d+\s*[-.]?\s*|chap(?:itre)?(?:\s+\d+)?\s*[-.]?\s*)+").unwrap()
});

pub async fn scan_tree(
    client: &PCloudClient,
    root_folder_id: u64,
    on_progress: Arc<dyn Fn(ScanEvent) + Send + Sync>,
) -> Result<Method> {
    let root = client.list_folder(root_folder_id).await?;

    on_progress(ScanEvent {
        current_folder: root.name.clone(),
        folders_visited: 0,
        methods_found: 0,
    });

    let mut global_order = 0u32;
    let (items, documents) = scan_folder_contents(
        client,
        &root.contents,
        &[],
        &[],
        &mut global_order,
        &on_progress,
    )
    .await?;

    Ok(Method {
        id: Uuid::new_v4().to_string(),
        title: root.name,
        source: MethodSource {
            provider: "pcloud".into(),
            root_folder_id,
        },
        default_count_in_bars: 1,
        items,
        documents,
        progress: Default::default(),
    })
}

/// Scan each direct (non-archive) subfolder of `root_folder_id` as a separate Method.
pub async fn scan_methods_in_folder(
    client: &PCloudClient,
    root_folder_id: u64,
    on_progress: Arc<dyn Fn(ScanEvent) + Send + Sync>,
) -> Result<Vec<Method>> {
    let root = client.list_folder(root_folder_id).await?;

    let mut subfolders: Vec<(u64, String)> = root
        .contents
        .into_iter()
        .filter(|e| e.isfolder && !e.name.to_lowercase().starts_with("archive"))
        .filter_map(|e| e.folderid.map(|id| (id, e.name)))
        .collect();

    natural_sort_by(&mut subfolders, |(_, name)| name.clone());

    let mut methods = Vec::new();

    let folders_visited = Arc::new(AtomicU32::new(0));
    let mut methods_found = 0u32;

    for (folder_id, _) in subfolders {
        let prog = Arc::clone(&on_progress);
        let fv = Arc::clone(&folders_visited);
        let mf = methods_found;

        let wrapped: Arc<dyn Fn(ScanEvent) + Send + Sync> = Arc::new(move |event| {
            prog(ScanEvent {
                current_folder: event.current_folder,
                folders_visited: fv.fetch_add(1, Ordering::Relaxed) + 1,
                methods_found: mf,
            });
        });

        let method = scan_tree(client, folder_id, wrapped).await?;
        if method.has_lessons() {
            methods_found += 1;
        }
        methods.push(method);
    }

    Ok(methods)
}

/// Core logic: given a list of pCloud entries (one folder's contents), produce
/// the interleaved `Vec<SectionItem>` and `Vec<DocumentRef>` for that folder.
///
/// Two-pass approach:
///   Pass 1 — build the pool: collect direct audio/tab files and recurse into
///             `tab*` / `backing*` special folders.  This must happen first so
///             that direct video files at this level inherit the complete pool.
///   Pass 2 — emit items in natural sort order: video → Lesson, regular folder →
///             Section (recursive).  Special folders and audio/tab files are
///             skipped because they already contributed to the pool.
#[async_recursion]
async fn scan_folder_contents(
    client: &PCloudClient,
    entries: &'async_recursion [Entry],
    inherited_backing: &'async_recursion [BackingTrack],
    inherited_tabs: &'async_recursion [TabSet],
    global_order: &mut u32,
    on_progress: &Arc<dyn Fn(ScanEvent) + Send + Sync>,
) -> Result<(Vec<SectionItem>, Vec<DocumentRef>)> {
    // ── Pass 1: build local pool ──────────────────────────────────────────────

    let mut local_backing: Vec<BackingTrack> = inherited_backing.to_vec();
    let mut local_tabs: Vec<TabSet> = inherited_tabs.to_vec();
    // Raw tab files collected at this level — grouped into TabSets after Pass 1.
    let mut raw_tabs: Vec<(String, String, FileRef)> = Vec::new(); // (stem, ext, file_ref)
    let mut special_ids: HashSet<u64> = HashSet::new();
    let mut pool_docs: Vec<DocumentRef> = Vec::new();

    // Tab-file stems found at this level (used to skip PDF tab exports).
    let tab_stems: HashSet<String> = entries
        .iter()
        .filter(|e| e.is_tab())
        .map(|e| file_stem(&e.name))
        .collect();

    for entry in entries {
        let name_lc = entry.name.to_lowercase();
        if entry.isfolder {
            if name_lc.starts_with("tab") || name_lc.starts_with("backing") {
                if let Some(id) = entry.folderid {
                    special_ids.insert(id);
                    let sub = client.list_folder(id).await?;
                    let sub_tab_stems: HashSet<String> = sub
                        .contents
                        .iter()
                        .filter(|e| e.is_tab())
                        .map(|e| file_stem(&e.name))
                        .collect();
                    for e in &sub.contents {
                        if e.isfolder {
                            continue;
                        } else if e.is_audio() {
                            local_backing.push(backing_track(e));
                        } else if e.is_tab() {
                            raw_tabs.push((file_stem(&e.name), ext_of(&e.name), file_ref(e)));
                        } else if e.is_pdf() && !sub_tab_stems.contains(&file_stem(&e.name)) {
                            pool_docs.push(DocumentRef {
                                file: file_ref(e),
                                kind: DocKind::Pdf,
                                title: file_stem(&e.name),
                            });
                        } else if e.is_img() {
                            pool_docs.push(DocumentRef {
                                file: file_ref(e),
                                kind: DocKind::Image,
                                title: file_stem(&e.name),
                            });
                        }
                    }
                }
            } else if name_lc.contains("documents utile") || name_lc.contains("document utile") {
                if let Some(id) = entry.folderid {
                    special_ids.insert(id);
                    let sub = client.list_folder(id).await?;
                    for e in &sub.contents {
                        if e.isfolder {
                            continue;
                        } else if e.is_pdf() {
                            pool_docs.push(DocumentRef {
                                file: file_ref(e),
                                kind: DocKind::Pdf,
                                title: file_stem(&e.name),
                            });
                        } else if e.is_img() {
                            pool_docs.push(DocumentRef {
                                file: file_ref(e),
                                kind: DocKind::Image,
                                title: file_stem(&e.name),
                            });
                        }
                    }
                }
            }
        } else if entry.is_audio() {
            local_backing.push(backing_track(entry));
        } else if entry.is_tab() {
            raw_tabs.push((file_stem(&entry.name), ext_of(&entry.name), file_ref(entry)));
        }
    }

    local_tabs.extend(group_raw_tabs(raw_tabs));

    // ── Pass 2: emit items in natural sort order (files AND folders together) ─

    // Sort by index to avoid needing Clone on Entry.
    let mut sorted: Vec<usize> = (0..entries.len()).collect();
    sorted.sort_by(|&a, &b| natural_cmp(&entries[a].name, &entries[b].name));

    let mut items: Vec<SectionItem> = Vec::new();
    // Documents collected at this level (not from special folders).
    let mut local_docs: Vec<DocumentRef> = pool_docs;

    for idx in sorted {
        let entry = &entries[idx];
        let name_lc = entry.name.to_lowercase();

        if entry.isfolder {
            if name_lc.starts_with("archive") {
                continue;
            }
            let sub_id = match entry.folderid {
                Some(id) => id,
                None => continue,
            };
            if special_ids.contains(&sub_id) {
                continue; // already merged into pool in pass 1
            }
            // Regular folder → recurse as a Section.
            let sub = client.list_folder(sub_id).await?;
            on_progress(ScanEvent {
                current_folder: sub.name.clone(),
                folders_visited: 0,
                methods_found: 0,
            });
            let (sub_items, sub_docs) = scan_folder_contents(
                client,
                &sub.contents,
                &local_backing,
                &local_tabs,
                global_order,
                on_progress,
            )
            .await?;
            items.push(SectionItem::Section(Section {
                id: Uuid::new_v4().to_string(),
                title: clean_section_title(&entry.name),
                items: sub_items,
                documents: sub_docs,
            }));
        } else if entry.is_video() {
            *global_order += 1;
            items.push(SectionItem::Lesson(Lesson {
                id: Uuid::new_v4().to_string(),
                order: *global_order,
                title: file_stem(&entry.name),
                video: file_ref(entry),
                tabs: local_tabs.clone(),
                backing_groups: group_by_radical(&local_backing),
            }));
        } else if entry.is_pdf() && !tab_stems.contains(&file_stem(&entry.name)) {
            local_docs.push(DocumentRef {
                file: file_ref(entry),
                kind: DocKind::Pdf,
                title: file_stem(&entry.name),
            });
        } else if entry.is_img() {
            local_docs.push(DocumentRef {
                file: file_ref(entry),
                kind: DocKind::Image,
                title: file_stem(&entry.name),
            });
        }
        // audio, tab files: skip in pass 2 (handled in pass 1)
    }

    Ok((items, local_docs))
}

// --- Helpers ---

fn clean_section_title(name: &str) -> String {
    let cleaned = SECTION_PREFIX_RE.replace(name, "");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        name.trim().to_string()
    } else {
        cleaned.to_string()
    }
}

fn file_stem(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_owned())
}

fn file_ref(entry: &Entry) -> FileRef {
    FileRef {
        file_id: entry.fileid.unwrap_or(0),
        name: entry.name.clone(),
    }
}

fn parse_bpm(name: &str) -> u32 {
    let lower = name.to_lowercase();

    if let Some(captures) = BPM_RE.captures(&lower) {
        if let Ok(n) = captures["bpm"].parse() {
            return n;
        }
    }

    0
}

fn radical_of(name: &str) -> String {
    BPM_RE.replace(&file_stem(name), "").trim().to_string()
}

fn backing_track(entry: &Entry) -> BackingTrack {
    BackingTrack {
        audio: file_ref(entry),
        bpm: parse_bpm(&entry.name),
        lead_in_ms_override: None,
        sync_points: None,
    }
}

fn ext_of(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn group_raw_tabs(raw: Vec<(String, String, FileRef)>) -> Vec<TabSet> {
    let mut by_stem: BTreeMap<String, Vec<TabFile>> = BTreeMap::new();
    for (stem, ext, file) in raw {
        by_stem.entry(stem).or_default().push(TabFile { ext, file });
    }
    by_stem
        .into_iter()
        .map(|(title, files)| TabSet {
            id: Uuid::new_v4().to_string(),
            title,
            files,
        })
        .collect()
}

fn group_by_radical(tracks: &[BackingTrack]) -> Vec<BackingGroup> {
    let mut map: HashMap<String, Vec<BackingTrack>> = HashMap::new();
    for track in tracks {
        let label = radical_of(&track.audio.name);
        map.entry(label).or_default().push(track.clone());
    }
    let mut groups: Vec<BackingGroup> = map
        .into_iter()
        .map(|(label, mut tracks)| {
            tracks.sort_by_key(|t| t.bpm);
            BackingGroup { label, tracks }
        })
        .collect();
    groups.sort_by(|a, b| a.label.cmp(&b.label));
    groups
}

fn natural_sort_by<T>(items: &mut [T], key: impl Fn(&T) -> String) {
    items.sort_by(|a, b| natural_cmp(&key(a), &key(b)));
}

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();
    loop {
        match (a_chars.peek().copied(), b_chars.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let na = consume_number(&mut a_chars);
                let nb = consume_number(&mut b_chars);
                let ord = na.cmp(&nb);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (Some(ac), Some(bc)) => {
                a_chars.next();
                b_chars.next();
                let al = ac.to_lowercase().next().unwrap();
                let bl = bc.to_lowercase().next().unwrap();
                let ord = al.cmp(&bl);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

fn consume_number(iter: &mut std::iter::Peekable<std::str::Chars>) -> u64 {
    let mut s = String::new();
    while iter.peek().is_some_and(|c| c.is_ascii_digit()) {
        s.push(iter.next().unwrap());
    }
    s.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::clean_section_title;

    #[test]
    fn strips_chap_prefix() {
        assert_eq!(clean_section_title("CHAP 1 Introduction"), "Introduction");
        assert_eq!(clean_section_title("CHAP 1 - Introduction"), "Introduction");
        assert_eq!(clean_section_title("chap 3 Les gammes"), "Les gammes");
        assert_eq!(clean_section_title("CHAPITRE 2 - Les gammes"), "Les gammes");
        assert_eq!(clean_section_title("CHAPITRE 2 Les gammes"), "Les gammes");
        assert_eq!(
            clean_section_title("1 - CHAPITRE - Les gammes"),
            "Les gammes"
        );
        assert_eq!(clean_section_title("CHAPITRE - Les gammes"), "Les gammes");
    }

    #[test]
    fn strips_numeric_prefix() {
        assert_eq!(clean_section_title("1 - Introduction"), "Introduction");
        assert_eq!(clean_section_title("01 - Introduction"), "Introduction");
        assert_eq!(clean_section_title("1. Introduction"), "Introduction");
        assert_eq!(clean_section_title("1 Introduction"), "Introduction");
    }

    #[test]
    fn strips_combined_prefix() {
        assert_eq!(clean_section_title("1 CHAP 1 Introduction"), "Introduction");
        assert_eq!(
            clean_section_title("01 - CHAP 1 - Introduction"),
            "Introduction"
        );
    }

    #[test]
    fn leaves_clean_titles_unchanged() {
        assert_eq!(clean_section_title("Introduction"), "Introduction");
        assert_eq!(
            clean_section_title("Les gammes pentatoniques"),
            "Les gammes pentatoniques"
        );
    }

    #[test]
    fn fallback_when_stripping_leaves_empty() {
        assert_eq!(clean_section_title("CHAP 1"), "CHAP 1");
        assert_eq!(clean_section_title("1"), "1");
    }
}
