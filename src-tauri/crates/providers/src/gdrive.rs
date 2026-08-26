use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::{DownloadTarget, Entry, ProviderAuth, StorageProvider};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub struct GDriveClient {
    client: reqwest::Client,
    access_token: String,
}

impl GDriveClient {
    /// `token` is the OAuth access token obtained via Google Drive OAuth flow.
    pub fn new(token: String) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            access_token: token,
        })
    }
}

impl StorageProvider for GDriveClient {
    fn provider_id(&self) -> crate::ProviderId {
        crate::ProviderId::GoogleDrive
    }

    async fn list_folder(&self, folder_id: &str) -> anyhow::Result<Vec<crate::Entry>> {
        // "root" is Drive's documented alias for the user's root folder — usable directly
        // here, same as any other folder id (cf. lib/providers.ts PROVIDERS.gdrive.rootId).
        let query = format!("trashed = false and '{folder_id}' in parents");

        // TODO : handle cursor pagination (nextPageToken)
        let resp: FolderContentsResponse = self
            .client
            .get("https://www.googleapis.com/drive/v3/files")
            .query(&[
                ("fields", "files(id,name,mimeType,size),nextPageToken"),
                ("pageSize", "1000"),
                ("q", &query),
            ])
            .bearer_auth(&self.access_token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp.files.into_iter().map(map_entry).collect())
    }

    async fn resolve_download_url(
        &self,
        file_id: &str,
        _transcoded: bool,
    ) -> anyhow::Result<DownloadTarget> {
        Ok(DownloadTarget {
            url: format!("https://www.googleapis.com/drive/v3/files/{file_id}?alt=media"),
            headers: vec![(
                "Authorization".into(),
                format!("Bearer {}", self.access_token),
            )],
        })
    }

    // Google Drive directly send the file in the API endpoint so no URL cache is required
    fn is_cacheable(&self) -> bool {
        false
    }
}

#[derive(Deserialize)]
struct FolderContentsResponse {
    #[expect(dead_code, reason = "TODO : handle cursor pagination")]
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    files: Vec<FolderContentEntry>,
}

#[derive(Deserialize)]
struct FolderContentEntry {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    // Drive serializes int64 fields as JSON strings (avoids precision loss in JS clients).
    size: Option<String>,
}

fn map_entry(entry: FolderContentEntry) -> Entry {
    Entry {
        id: entry.id,
        name: entry.name,
        is_folder: entry.mime_type == "application/vnd.google-apps.folder",
        size: entry.size.and_then(|s| s.parse().ok()),
    }
}

pub struct GDriveAuth;

impl ProviderAuth for GDriveAuth {
    fn authorize_url(&self, client_id: &str, redirect_uri: &str) -> String {
        format!(
            "{AUTH_URL}?client_id={client_id}&redirect_uri={}&response_type=code&scope={}",
            crate::oauth::percent_encode(redirect_uri),
            crate::oauth::percent_encode("https://www.googleapis.com/auth/drive.readonly"),
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
        let refresh_token = creds
            .refresh_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no refresh token stored for provider"))?;

        let resp: RefreshResponse = reqwest::Client::new()
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
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
