// Folder scan — §6 rules: DFS + natural sort, keyword buckets, pool attachment.
// No tab↔backing matching by name (cf. ARCHITECTURE.md §6 + §14).
use anyhow::Result;
use async_recursion::async_recursion;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use uuid::Uuid;

use crate::{Entry, PCloudClient};
use chelou_manifest::{
    BackingGroup, BackingTrack, DocKind, DocumentRef, FileRef, Lesson, Method, MethodSource, TabSet,
};

/// Progress event emitted during a DFS scan — one per folder entered.
/// Payload of the Tauri "scan:progress" event.
#[derive(Debug, Clone, Serialize)]
pub struct ScanEvent {
    /// Name of the cloud folder currently being visited.
    #[serde(rename = "currentFolder")]
    pub current_folder: String,
    /// Total cloud folders visited so far (DFS, all candidates combined).
    #[serde(rename = "foldersVisited")]
    pub folders_visited: u32,
    /// Top-level candidate folders confirmed as methods (had ≥1 video) so far.
    #[serde(rename = "methodsFound")]
    pub methods_found: u32,
}

pub async fn scan_tree(
    client: &PCloudClient,
    root_folder_id: u64,
    on_progress: Arc<dyn Fn(ScanEvent) + Send + Sync>,
) -> Result<Method> {
    let root_folder = client.list_folder(root_folder_id).await?;

    let mut lessons = Vec::new();
    let mut documents = Vec::new();
    let mut order = 0u32;
    let mut output = ScanOutput {
        lessons: &mut lessons,
        documents: &mut documents,
        order: &mut order,
    };

    scan_node(client, root_folder_id, &[], &[], &mut output, on_progress).await?;

    Ok(Method {
        id: Uuid::new_v4().to_string(),
        title: root_folder.name,
        source: MethodSource {
            provider: "pcloud".into(),
            root_folder_id,
        },
        default_count_in_bars: 1,
        lessons,
        documents,
    })
}

/// Scan each direct (non-archive) subfolder of `root_folder_id` as a separate Method.
/// Returns one Method per subfolder, in natural sort order.
/// Use this when the picked folder is a container (e.g. "Méthodes guitare").
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

    // Shared atomic — Fn closure (not FnMut) can't hold &mut, so we use AtomicU32.
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
        if !method.lessons.is_empty() {
            methods_found += 1;
        }
        methods.push(method);
    }

    Ok(methods)
}

struct ScanOutput<'a> {
    lessons: &'a mut Vec<Lesson>,
    documents: &'a mut Vec<DocumentRef>,
    order: &'a mut u32,
}

#[async_recursion]
async fn scan_node(
    client: &PCloudClient,
    folder_id: u64,
    inherited_backing: &'async_recursion [BackingTrack],
    inherited_tabs: &'async_recursion [TabSet],
    output: &mut ScanOutput,
    on_progress: Arc<dyn Fn(ScanEvent) + Send + Sync>,
) -> Result<()> {
    let contents = client.list_folder(folder_id).await?;

    // §6: archive* → skip entirely
    if contents.name.to_lowercase().starts_with("archive") {
        return Ok(());
    }

    // folders_visited and methods_found are placeholders — the wrapper in
    // scan_methods_in_folder replaces them with the real global values.
    on_progress(ScanEvent {
        current_folder: contents.name.clone(),
        folders_visited: 0,
        methods_found: 0,
    });

    let mut local_backing: Vec<BackingTrack> = inherited_backing.to_vec();
    let mut local_tabs: Vec<TabSet> = inherited_tabs.to_vec();
    let mut videos: Vec<FileRef> = Vec::new();
    // §6 special keyword folders: contents are merged into the parent pool, not recursed.
    let mut special_subfolders: Vec<u64> = Vec::new();
    let mut regular_subfolders: Vec<(u64, String)> = Vec::new();

    // PDF stems that have a sibling tab file → they are tab exports, ignore them (§6)
    let tab_stems: std::collections::HashSet<String> = contents
        .contents
        .iter()
        .filter(|e| e.is_tab())
        .map(|e| file_stem(&e.name))
        .collect();

    for entry in &contents.contents {
        let name_lc = entry.name.to_lowercase();
        if entry.isfolder {
            let sub_id = match entry.folderid {
                Some(id) => id,
                None => continue,
            };
            if name_lc.starts_with("archive") {
                // skip
            } else if name_lc.starts_with("tab") || name_lc.starts_with("backing") {
                // §6 special folder: merge contents into parent pool before building lessons
                special_subfolders.push(sub_id);
            } else {
                regular_subfolders.push((sub_id, entry.name.clone()));
            }
        } else if entry.is_video() {
            videos.push(file_ref(entry));
        } else if entry.is_audio() {
            local_backing.push(backing_track(entry));
        } else if entry.is_tab() {
            local_tabs.push(tab_set(entry));
        } else if entry.is_pdf() && !tab_stems.contains(&file_stem(&entry.name)) {
            output.documents.push(DocumentRef {
                file: file_ref(entry),
                kind: DocKind::Pdf,
                title: file_stem(&entry.name),
            });
        }
    }

    // Merge special subfolder contents into local pools (§6: tab*, backing*).
    // Done before creating lessons so videos at the same level get the full pool.
    for sub_id in special_subfolders {
        let sub = client.list_folder(sub_id).await?;
        let sub_tab_stems: std::collections::HashSet<String> = sub
            .contents
            .iter()
            .filter(|e| e.is_tab())
            .map(|e| file_stem(&e.name))
            .collect();
        for entry in &sub.contents {
            if entry.isfolder {
                continue; // nested folders inside special dirs are ignored
            } else if entry.is_audio() {
                local_backing.push(backing_track(entry));
            } else if entry.is_tab() {
                local_tabs.push(tab_set(entry));
            } else if entry.is_pdf() && !sub_tab_stems.contains(&file_stem(&entry.name)) {
                output.documents.push(DocumentRef {
                    file: file_ref(entry),
                    kind: DocKind::Pdf,
                    title: file_stem(&entry.name),
                });
            }
        }
    }

    // Videos in this folder → lessons with the now-complete pool, natural-sorted
    natural_sort_by(&mut videos, |f| f.name.clone());
    for video in videos {
        *(output.order) += 1;
        output.lessons.push(Lesson {
            id: Uuid::new_v4().to_string(),
            order: *(output.order),
            title: file_stem(&video.name),
            videos: vec![video],
            tabs: local_tabs.clone(),
            backing_groups: group_by_radical(&local_backing),
        });
    }

    // Recurse into regular subfolders with the completed pool, natural-sorted
    natural_sort_by(&mut regular_subfolders, |(_, name)| name.clone());
    for (sub_id, _) in regular_subfolders {
        scan_node(
            client,
            sub_id,
            &local_backing,
            &local_tabs,
            output,
            Arc::clone(&on_progress),
        )
        .await?;
    }

    Ok(())
}

// --- Helpers ---

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
    if let Some(start) = lower.rfind('(') {
        if let Some(end) = lower[start..].find("bpm)") {
            if let Ok(n) = lower[start + 1..start + end].trim().parse() {
                return n;
            }
        }
    }
    0
}

fn radical_of(name: &str) -> String {
    let stem = file_stem(name);
    let lower = stem.to_lowercase();
    if let Some(pos) = lower.rfind('(') {
        if lower[pos..].contains("bpm)") {
            return stem[..pos].trim().to_owned();
        }
    }
    stem
}

fn backing_track(entry: &Entry) -> BackingTrack {
    BackingTrack {
        audio: file_ref(entry),
        bpm: parse_bpm(&entry.name),
        lead_in_ms_override: None,
        sync_points: None,
    }
}

fn tab_set(entry: &Entry) -> TabSet {
    let ext = std::path::Path::new(&entry.name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let r = file_ref(entry);
    TabSet {
        id: Uuid::new_v4().to_string(),
        title: file_stem(&entry.name),
        gp: if ext == "gp" { Some(r.clone()) } else { None },
        gpx: if ext == "gpx" { Some(r) } else { None },
    }
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
