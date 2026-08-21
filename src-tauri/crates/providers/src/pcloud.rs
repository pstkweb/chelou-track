use std::time::Duration;

use crate::{Entry, FolderContents, ProviderAuth, ProviderId, StorageProvider};

use anyhow::{anyhow, Result};
use serde::Deserialize;

const AUTH_URL: &str = "https://my.pcloud.com/oauth2/authorize";
const TOKEN_URL: &str = "https://eapi.pcloud.com/oauth2_token";

pub struct PCloudClient {
    client: pcloud::Client,
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

        Ok(Self { client })
    }

    /// Direct download link. Same IP generates and consumes — cf. ARCHITECTURE.md §3.
    async fn get_file_link(&self, file_id: u64) -> Result<String> {
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
    async fn get_video_link(&self, file_id: u64) -> Result<String> {
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
}

impl StorageProvider for PCloudClient {
    fn provider_id(&self) -> ProviderId {
        ProviderId::PCloud
    }

    async fn list_folder(&self, folder_id: &str) -> Result<FolderContents> {
        let id: u64 = folder_id.parse()?;
        let folder = self
            .client
            .list_folder(id)
            .await
            .map_err(|e| anyhow!("listfolder: {e}"))?;

        Ok(map_folder(folder))
    }

    async fn resolve_download_url(&self, file_id: &str, transcoded: bool) -> Result<String> {
        let id: u64 = file_id.parse()?;

        if transcoded {
            self.get_video_link(id).await
        } else {
            self.get_file_link(id).await
        }
    }
}

fn map_folder(folder: pcloud::folder::Folder) -> FolderContents {
    FolderContents {
        id: folder.folder_id.to_string(),
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
            is_folder: false,
            id: f.file_id.to_string(),
            size: f.size.map(|s| s as u64),
        },
        pcloud::entry::Entry::Folder(f) => Entry {
            name: f.base.name,
            is_folder: true,
            id: f.folder_id.to_string(),
            size: None,
        },
    }
}

pub struct PCloudAuth;

impl ProviderAuth for PCloudAuth {
    fn authorize_url(&self, client_id: &str, redirect_uri: &str) -> String {
        format!(
            "{AUTH_URL}?client_id={client_id}&response_type=code&redirect_uri={}",
            crate::oauth::percent_encode(redirect_uri),
        )
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        client_id: &str,
        client_secret: &str,
    ) -> Result<crate::StoredCredentials> {
        let resp: TokenResponse = reqwest::Client::new()
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("code", code),
                ("redirect_uri", redirect_uri),
            ])
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if resp.result.is_some_and(|r| r != 0) {
            return Err(anyhow!(
                "pCloud token exchange error: {}",
                resp.error.unwrap_or_default()
            ));
        }

        resp.access_token
            .ok_or_else(|| anyhow!("no access_token in pCloud response"))
            .map(|token| crate::StoredCredentials {
                access_token: token,
                refresh_token: None,
                expires_at: None,
            })
    }

    async fn refresh(
        &self,
        creds: &crate::StoredCredentials,
        _client_id: &str,
        _client_secret: &str,
    ) -> Result<crate::StoredCredentials> {
        Ok(creds.clone())
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    /// pCloud convention: 0 = success, non-zero = error.
    result: Option<i32>,
    access_token: Option<String>,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            is_folder: false,
            id: "1".to_owned(),
            size: None,
        }
    }

    fn folder(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            is_folder: true,
            id: "1".to_owned(),
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
        assert!(!entry.is_folder);
        assert_eq!(entry.id, "42");
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
        assert!(entry.is_folder);
        assert_eq!(entry.id, "99");
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
        assert_eq!(fc.id, "7");
        assert_eq!(fc.contents.len(), 2);
        assert!(fc.contents[0].is_video());
        assert!(fc.contents[1].is_folder);
    }
}
