use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;

use crate::{Entry, ProviderAuth, StorageProvider};

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
    ) -> anyhow::Result<String> {
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

        Ok(resp.link)
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
    fn authorize_url(&self, client_id: &str, redirect_uri: &str) -> String {
        format!(
            "{AUTH_URL}?client_id={client_id}&redirect_uri={}&token_access_type=offline&response_type=code",
            crate::oauth::percent_encode(redirect_uri)
        )
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        client_id: &str,
        client_secret: &str,
    ) -> anyhow::Result<crate::StoredCredentials> {
        let resp: TokenResponse = reqwest::Client::new()
            .post(TOKEN_URL)
            .form(&[
                ("code", code),
                ("grant_type", "authorization_code"),
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("redirect_uri", redirect_uri),
            ])
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(crate::StoredCredentials {
            access_token: resp.access_token,
            refresh_token: Some(resp.refresh_token),
            expires_at: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + resp.expires_in,
            ),
        })
    }

    async fn refresh(
        &self,
        creds: &crate::StoredCredentials,
        client_id: &str,
        client_secret: &str,
    ) -> anyhow::Result<crate::StoredCredentials> {
        let resp: RefreshResponse = reqwest::Client::new()
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", creds.refresh_token.as_ref().unwrap()),
                ("client_id", client_id),
                ("client_secret", client_secret),
            ])
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(crate::StoredCredentials {
            access_token: resp.access_token,
            refresh_token: creds.refresh_token.clone(),
            expires_at: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + resp.expires_in,
            ),
        })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: u64,
}
