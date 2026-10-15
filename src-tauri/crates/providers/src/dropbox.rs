use std::collections::HashMap;

use serde::Deserialize;

use crate::{DownloadTarget, Entry, ProviderAuth, StorageProvider};

const AUTH_URL: &str = "https://www.dropbox.com/oauth2/authorize";
const TOKEN_URL: &str = "https://api.dropboxapi.com/oauth2/token";

pub struct DropboxClient {
    client: reqwest::Client,
    access_token: String,
}

impl DropboxClient {
    /// `token` is the OAuth access token obtained via Dropbox OAuth flow.
    pub fn new(token: String) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            access_token: token,
        })
    }
}

impl StorageProvider for DropboxClient {
    fn provider_id(&self) -> crate::ProviderId {
        crate::ProviderId::Dropbox
    }

    async fn list_folder(&self, folder_id: &str) -> anyhow::Result<Vec<crate::Entry>> {
        // TODO : handle cursor pagination (has_more + cursor)
        let resp: FolderContentsResponse = self
            .client
            .post("https://api.dropboxapi.com/2/files/list_folder")
            .json(&HashMap::from([("path", folder_id)]))
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp.entries.into_iter().map(map_entry).collect())
    }

    async fn resolve_download_url(
        &self,
        file_id: &str,
        _transcoded: bool,
    ) -> anyhow::Result<DownloadTarget> {
        let resp: TemporaryLinkResponse = self
            .client
            .post("https://api.dropboxapi.com/2/files/get_temporary_link")
            .json(&HashMap::from([("path", file_id)]))
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(DownloadTarget::url_only(resp.link))
    }
}

#[derive(Deserialize)]
struct FolderContentsResponse {
    #[expect(dead_code, reason = "TODO : handle cursor pagination")]
    cursor: String,
    #[expect(dead_code, reason = "TODO : handle cursor pagination")]
    has_more: bool,
    entries: Vec<FolderContentEntry>,
}

#[derive(Deserialize)]
struct FolderContentEntry {
    #[serde(rename = ".tag")]
    entry_type: String,
    id: String,
    name: String,
    size: Option<u64>,
}

#[derive(Deserialize)]
struct TemporaryLinkResponse {
    link: String,
}

fn map_entry(entry: FolderContentEntry) -> Entry {
    match entry {
        FolderContentEntry {
            entry_type,
            id,
            name,
            size,
        } if entry_type == "file" => Entry {
            name,
            is_folder: false,
            id,
            size,
        },
        FolderContentEntry {
            entry_type,
            id,
            name,
            size: _,
        } if entry_type == "folder" => Entry {
            name,
            is_folder: true,
            id,
            size: None,
        },
        FolderContentEntry { id, name, size, .. } => Entry {
            name,
            is_folder: false,
            id,
            size,
        },
    }
}

pub struct DropboxAuth;

impl ProviderAuth for DropboxAuth {
    fn authorize_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        code_challenge: &str,
    ) -> String {
        format!(
            "{AUTH_URL}?client_id={client_id}&redirect_uri={}&token_access_type=offline&response_type=code\
            &state={state}&code_challenge={code_challenge}&code_challenge_method=S256",
            crate::oauth::percent_encode(redirect_uri)
        )
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
        client_id: &str,
        client_secret: &str,
    ) -> anyhow::Result<crate::StoredCredentials> {
        let creds = crate::oauth::request_token(
            TOKEN_URL,
            &[
                ("code", code),
                ("grant_type", "authorization_code"),
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("redirect_uri", redirect_uri),
                ("code_verifier", code_verifier),
            ],
            None,
        )
        .await?;

        anyhow::ensure!(
            creds.refresh_token.is_some(),
            "no refresh_token in Dropbox response"
        );
        Ok(creds)
    }

    async fn refresh(
        &self,
        creds: &crate::StoredCredentials,
        client_id: &str,
        client_secret: &str,
    ) -> anyhow::Result<crate::StoredCredentials> {
        let refresh_token = creds
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no refresh token stored for provider"))?;

        crate::oauth::request_token(
            TOKEN_URL,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", client_id),
                ("client_secret", client_secret),
            ],
            Some(refresh_token),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tag: &str, size: Option<u64>) -> FolderContentEntry {
        FolderContentEntry {
            entry_type: tag.to_owned(),
            id: "id:abc".to_owned(),
            name: "x".to_owned(),
            size,
        }
    }

    #[test]
    fn map_entry_file_keeps_size() {
        let e = map_entry(entry("file", Some(42)));
        assert!(!e.is_folder);
        assert_eq!(e.size, Some(42));
        assert_eq!(e.id, "id:abc");
    }

    #[test]
    fn map_entry_folder_drops_size() {
        let e = map_entry(entry("folder", Some(42)));
        assert!(e.is_folder);
        assert_eq!(e.size, None);
    }

    #[test]
    fn map_entry_unknown_tag_is_not_a_folder() {
        // e.g. "deleted" entries
        assert!(!map_entry(entry("deleted", None)).is_folder);
    }

    #[test]
    fn authorize_url_carries_state_pkce_and_offline_access() {
        let url = DropboxAuth.authorize_url("cid", "http://localhost:1/cb", "st", "ch");
        assert!(url.starts_with(AUTH_URL));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcb"));
        assert!(url.contains("&state=st&"));
        assert!(url.contains("code_challenge=ch&code_challenge_method=S256"));
        assert!(url.contains("token_access_type=offline"));
    }
}
