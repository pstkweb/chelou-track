pub mod scanner;
pub use scanner::{scan_methods_in_folder, ScanEvent};

use anyhow::{anyhow, Result};
use serde::Serialize;

pub struct PCloudClient {
    client: pcloud::Client,
    http: reqwest::Client,
}

impl PCloudClient {
    /// Uses EU endpoint — never the US one (cf. ARCHITECTURE.md §3).
    /// `token` is the OAuth access token obtained via pCloud OAuth flow.
    pub fn new(token: String) -> Result<Self> {
        let client = pcloud::Client::new(
            pcloud::EU_REGION,
            pcloud::Credentials::AccessToken {
                access_token: token,
            },
        )
        .map_err(|e| anyhow!("failed to build pCloud client: {e}"))?;
        Ok(Self {
            client,
            http: reqwest::Client::new(),
        })
    }

    /// List a folder (shallow). Use scanner::scan_tree for recursive DFS.
    pub async fn list_folder(&self, folder_id: u64) -> Result<FolderContents> {
        let folder = self
            .client
            .list_folder(folder_id)
            .await
            .map_err(|e| anyhow!("listfolder: {e}"))?;
        Ok(map_folder(folder))
    }

    /// Direct download link. Same IP generates and consumes — cf. ARCHITECTURE.md §3.
    pub async fn get_file_link(&self, file_id: u64) -> Result<String> {
        let links = self
            .client
            .get_file_link(file_id)
            .await
            .map_err(|e| anyhow!("getfilelink: {e}"))?;
        links
            .first_link()
            .map(|l| l.to_string())
            .ok_or_else(|| anyhow!("getfilelink: no hosts in response"))
    }

    /// Transcoded video link — fallback when H.265 fails to decode (cf. §4 + §11).
    pub async fn get_video_link(&self, file_id: u64) -> Result<String> {
        let links = self
            .client
            .get_video_link(file_id)
            .await
            .map_err(|e| anyhow!("getvideolink: {e}"))?;
        links
            .first_link()
            .map(|l| l.to_string())
            .ok_or_else(|| anyhow!("getvideolink: no hosts in response"))
    }

    /// Fetch bytes from a pCloud direct link, forwarding Range if present.
    /// Returns a typed struct so callers don't need a direct reqwest dependency.
    pub async fn fetch_range(
        &self,
        url: &str,
        range_header: Option<&str>,
    ) -> Result<RangeResponse> {
        let mut req = self.http.get(url);
        if let Some(r) = range_header {
            req = req.header("Range", r);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let content_range = resp
            .headers()
            .get("Content-Range")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let body = resp.bytes().await?.to_vec();
        Ok(RangeResponse {
            status,
            content_type,
            content_range,
            body,
        })
    }
}

/// Structured response from fetch_range, decoupled from reqwest.
pub struct RangeResponse {
    pub status: u16,
    pub content_type: String,
    pub content_range: Option<String>,
    pub body: Vec<u8>,
}

// --- Mapping: pcloud crate types → scanner types ---

fn map_folder(folder: pcloud::folder::Folder) -> FolderContents {
    FolderContents {
        folderid: folder.folder_id,
        name: folder.base.name,
        contents: folder
            .contents
            .unwrap_or_default()
            .into_iter()
            .map(map_entry)
            .collect(),
    }
}

fn map_entry(entry: pcloud::entry::Entry) -> Entry {
    match entry {
        pcloud::entry::Entry::File(f) => Entry {
            name: f.base.name,
            isfolder: false,
            fileid: Some(f.file_id),
            folderid: None,
            size: f.size.map(|s| s as u64),
        },
        pcloud::entry::Entry::Folder(f) => Entry {
            name: f.base.name,
            isfolder: true,
            fileid: None,
            folderid: Some(f.folder_id),
            size: None,
        },
    }
}

// --- Types (used by scanner) ---

#[derive(Debug, Serialize)]
pub struct FolderContents {
    pub folderid: u64,
    pub name: String,
    pub contents: Vec<Entry>,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub name: String,
    pub isfolder: bool,
    pub fileid: Option<u64>,
    pub folderid: Option<u64>,
    pub size: Option<u64>,
}

impl Entry {
    pub fn is_video(&self) -> bool {
        !self.isfolder
            && matches!(
                std::path::Path::new(&self.name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("mp4" | "mkv" | "avi" | "mov")
            )
    }

    pub fn is_audio(&self) -> bool {
        !self.isfolder
            && matches!(
                std::path::Path::new(&self.name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("wav" | "mp3" | "flac")
            )
    }

    pub fn is_tab(&self) -> bool {
        !self.isfolder
            && matches!(
                std::path::Path::new(&self.name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("gp" | "gpx" | "gp5" | "gp4" | "gp3")
            )
    }

    pub fn is_pdf(&self) -> bool {
        !self.isfolder
            && std::path::Path::new(&self.name)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            isfolder: false,
            fileid: Some(1),
            folderid: None,
            size: None,
        }
    }

    fn folder(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            isfolder: true,
            fileid: None,
            folderid: Some(1),
            size: None,
        }
    }

    // --- Entry extension methods ---

    #[test]
    fn is_video_matches_expected_extensions() {
        assert!(file("lesson.mp4").is_video());
        assert!(file("lesson.mov").is_video());
        assert!(file("lesson.mkv").is_video());
        assert!(!file("audio.wav").is_video());
        assert!(!file("tab.gp").is_video());
        assert!(!file("doc.pdf").is_video());
    }

    #[test]
    fn is_audio_matches_expected_extensions() {
        assert!(file("track.wav").is_audio());
        assert!(file("track.mp3").is_audio());
        assert!(file("track.flac").is_audio());
        assert!(!file("video.mp4").is_audio());
    }

    #[test]
    fn is_tab_matches_expected_extensions() {
        assert!(file("score.gp").is_tab());
        assert!(file("score.gpx").is_tab());
        assert!(file("score.gp5").is_tab());
        assert!(file("score.gp4").is_tab());
        assert!(file("score.gp3").is_tab());
        assert!(!file("score.pdf").is_tab());
    }

    #[test]
    fn is_pdf_is_case_insensitive() {
        assert!(file("doc.pdf").is_pdf());
        assert!(file("doc.PDF").is_pdf());
        assert!(!file("tab.gp").is_pdf());
    }

    #[test]
    fn folder_is_never_a_media_file() {
        let f = folder("Backing tracks");
        assert!(!f.is_video());
        assert!(!f.is_audio());
        assert!(!f.is_tab());
        assert!(!f.is_pdf());
    }

    // --- map_entry / map_folder ---

    const ENTRY_BASE: &str = r#"
        "created": "Fri, 23 Jul 2021 19:39:09 +0000",
        "modified": "Fri, 23 Jul 2021 19:39:09 +0000",
        "ismine": true, "thumb": false,
        "id": "x", "isshared": false, "icon": "file"
    "#;

    #[test]
    fn map_entry_file() {
        let json =
            format!(r#"{{ {ENTRY_BASE}, "name": "01 Riff.mp4", "fileid": 42, "size": 1048576 }}"#);
        let crate_entry: pcloud::entry::Entry = serde_json::from_str(&json).unwrap();
        let entry = map_entry(crate_entry);

        assert_eq!(entry.name, "01 Riff.mp4");
        assert!(!entry.isfolder);
        assert_eq!(entry.fileid, Some(42));
        assert!(entry.folderid.is_none());
        assert_eq!(entry.size, Some(1_048_576));
        assert!(entry.is_video());
    }

    #[test]
    fn map_entry_folder() {
        let json =
            format!(r#"{{ {ENTRY_BASE}, "name": "02 Solo", "isfolder": true, "folderid": 99 }}"#);
        let crate_entry: pcloud::entry::Entry = serde_json::from_str(&json).unwrap();
        let entry = map_entry(crate_entry);

        assert_eq!(entry.name, "02 Solo");
        assert!(entry.isfolder);
        assert_eq!(entry.folderid, Some(99));
        assert!(entry.fileid.is_none());
    }

    #[test]
    fn map_folder_propagates_name_and_contents() {
        let json = format!(
            r#"{{
                {ENTRY_BASE},
                "name": "Méthode A", "isfolder": true, "folderid": 7,
                "contents": [
                    {{ {ENTRY_BASE}, "name": "lesson.mp4", "fileid": 1 }},
                    {{ {ENTRY_BASE}, "name": "sub", "isfolder": true, "folderid": 8 }}
                ]
            }}"#
        );
        let crate_folder: pcloud::folder::Folder = serde_json::from_str(&json).unwrap();
        let fc = map_folder(crate_folder);

        assert_eq!(fc.name, "Méthode A");
        assert_eq!(fc.folderid, 7);
        assert_eq!(fc.contents.len(), 2);
        assert!(fc.contents[0].is_video());
        assert!(fc.contents[1].isfolder);
    }
}
