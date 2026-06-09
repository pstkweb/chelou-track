// Folder scan — §6 rules: DFS + natural sort, keyword buckets, pool attachment.
// No tab↔backing matching by name (cf. ARCHITECTURE.md §6 + §14).
use anyhow::Result;
use async_recursion::async_recursion;
use std::collections::HashMap;
use uuid::Uuid;

use crate::{Entry, PCloudClient};
use chelou_manifest::{
    BackingGroup, BackingTrack, DocKind, DocumentRef, FileRef, Lesson, Method, MethodSource, TabSet,
};

pub async fn scan_tree(client: &PCloudClient, root_folder_id: u64, title: &str) -> Result<Method> {
    let mut lessons: Vec<Lesson> = Vec::new();
    let mut documents: Vec<DocumentRef> = Vec::new();
    let mut order = 0u32;

    scan_node(
        client,
        root_folder_id,
        &[],
        &[],
        &mut lessons,
        &mut documents,
        &mut order,
    )
    .await?;

    Ok(Method {
        id: Uuid::new_v4().to_string(),
        title: title.to_owned(),
        source: MethodSource {
            provider: "pcloud".into(),
            root_folder_id,
        },
        default_count_in_bars: 1,
        lessons,
        documents,
    })
}

#[async_recursion]
async fn scan_node(
    client: &PCloudClient,
    folder_id: u64,
    inherited_backing: &'async_recursion [BackingTrack],
    inherited_tabs: &'async_recursion [TabSet],
    lessons: &mut Vec<Lesson>,
    documents: &mut Vec<DocumentRef>,
    order: &mut u32,
) -> Result<()> {
    let contents = client.list_folder(folder_id).await?;

    // §6: archive* → skip entirely
    if contents.name.to_lowercase().starts_with("archive") {
        return Ok(());
    }

    let mut local_backing: Vec<BackingTrack> = inherited_backing.to_vec();
    let mut local_tabs: Vec<TabSet> = inherited_tabs.to_vec();
    let mut videos: Vec<FileRef> = Vec::new();
    let mut subfolders: Vec<(u64, String)> = Vec::new();

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
            if !name_lc.starts_with("archive") {
                subfolders.push((sub_id, entry.name.clone()));
            }
        } else if entry.is_video() {
            videos.push(file_ref(entry));
        } else if entry.is_audio() {
            local_backing.push(backing_track(entry));
        } else if entry.is_tab() {
            local_tabs.push(tab_set(entry));
        } else if entry.is_pdf() && !tab_stems.contains(&file_stem(&entry.name)) {
            documents.push(DocumentRef {
                file: file_ref(entry),
                kind: DocKind::Pdf,
                title: file_stem(&entry.name),
            });
        }
    }

    // Videos in this folder → lessons, natural-sorted
    natural_sort_by(&mut videos, |f| f.name.clone());
    for video in videos {
        *order += 1;
        lessons.push(Lesson {
            id: Uuid::new_v4().to_string(),
            order: *order,
            title: file_stem(&video.name),
            videos: vec![video],
            tabs: local_tabs.clone(),
            backing_groups: group_by_radical(&local_backing),
        });
    }

    // Recurse into subfolders, natural-sorted
    natural_sort_by(&mut subfolders, |(_, name)| name.clone());
    for (sub_id, _) in subfolders {
        scan_node(
            client,
            sub_id,
            &local_backing,
            &local_tabs,
            lessons,
            documents,
            order,
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
