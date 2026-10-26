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
        let query = format!(
            "trashed = false and '{}' in parents",
            escape_query_literal(folder_id)
        );

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
        Ok(DownloadTarget::Http {
            url: format!(
                "https://www.googleapis.com/drive/v3/files/{}?alt=media",
                crate::oauth::percent_encode(file_id)
            ),
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
    fn authorize_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        code_challenge: &str,
    ) -> String {
        format!(
            "{AUTH_URL}?client_id={client_id}&redirect_uri={}&response_type=code&scope={}\
            &state={state}&code_challenge={code_challenge}&code_challenge_method=S256",
            crate::oauth::percent_encode(redirect_uri),
            // Google's own suggestion of drive.file + drive.metadata.readonly does not
            // work for this app: drive.file grants content access only to files the app
            // individually opened/created (via Picker or "Open with"), never to files
            // merely discovered by listing a picked folder's contents — confirmed against
            // a real scan, which got a 403 appNotAuthorizedToFile trying to stream a file
            // found that way. Scanning an arbitrary user-chosen folder tree and streaming
            // whatever's found in it — same model as pCloud/Dropbox — needs drive.readonly.
            crate::oauth::percent_encode("https://www.googleapis.com/auth/drive.readonly"),
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
            "no refresh_token in Google Drive response"
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

/// Escape a value for a single-quoted string literal in a Drive `q` query
/// (https://developers.google.com/drive/api/guides/ref-search-terms): `\` and `'` are
/// backslash-escaped, so a crafted folder id can't close the literal and add clauses.
fn escape_query_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_query_literal_neutralises_quotes_and_backslashes() {
        assert_eq!(escape_query_literal("root"), "root");
        assert_eq!(
            escape_query_literal("x' in parents or 'a"),
            "x\\' in parents or \\'a"
        );
        assert_eq!(escape_query_literal("a\\'b"), "a\\\\\\'b");
    }

    #[test]
    fn map_entry_detects_folders_and_parses_string_size() {
        let folder = map_entry(FolderContentEntry {
            id: "1".into(),
            name: "f".into(),
            mime_type: "application/vnd.google-apps.folder".into(),
            size: None,
        });
        assert!(folder.is_folder);

        let file = map_entry(FolderContentEntry {
            id: "2".into(),
            name: "a.mp3".into(),
            mime_type: "audio/mpeg".into(),
            size: Some("1234".into()),
        });
        assert!(!file.is_folder);
        assert_eq!(file.size, Some(1234));
    }

    #[test]
    fn map_entry_ignores_unparseable_size() {
        let e = map_entry(FolderContentEntry {
            id: "3".into(),
            name: "x".into(),
            mime_type: "text/plain".into(),
            size: Some("n/a".into()),
        });
        assert_eq!(e.size, None);
    }

    #[test]
    fn authorize_url_carries_state_pkce_and_readonly_scope() {
        let url = GDriveAuth.authorize_url("cid", "http://localhost:1/cb", "st", "ch");
        assert!(url.starts_with(AUTH_URL));
        assert!(url.contains("&state=st&"));
        assert!(url.contains("code_challenge=ch&code_challenge_method=S256"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.readonly"));
    }

    #[tokio::test]
    async fn resolve_download_url_sends_token_as_header_not_in_url() {
        let client = GDriveClient::new("tok".into()).unwrap();
        let target = client.resolve_download_url("fid", false).await.unwrap();
        let DownloadTarget::Http { url, headers } = target else {
            panic!("gdrive target must be HTTP");
        };
        assert!(!url.contains("tok"));
        assert_eq!(
            headers,
            vec![("Authorization".to_owned(), "Bearer tok".to_owned())]
        );
    }
}
