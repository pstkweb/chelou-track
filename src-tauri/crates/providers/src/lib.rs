pub mod scanner;
use std::{fmt::Display, str::FromStr};

pub use scanner::{scan_methods_in_folder, ScanEvent};
pub mod dropbox;
pub mod gdrive;
pub mod local;
pub mod oauth;
pub mod pcloud;
pub mod registry;
pub use registry::{make_auth, make_client, ProviderAuthClient, ProviderClient};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
    pub size: Option<u64>,
}

impl Entry {
    pub fn is_video(&self) -> bool {
        !self.is_folder
            && matches!(
                std::path::Path::new(&self.name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("mp4" | "mkv" | "avi" | "mov")
            )
    }

    pub fn is_audio(&self) -> bool {
        !self.is_folder
            && matches!(
                std::path::Path::new(&self.name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("wav" | "mp3" | "flac")
            )
    }

    pub fn is_tab(&self) -> bool {
        !self.is_folder
            && matches!(
                std::path::Path::new(&self.name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("gp" | "gpx" | "gp5" | "gp4" | "gp3")
            )
    }

    pub fn is_pdf(&self) -> bool {
        !self.is_folder
            && std::path::Path::new(&self.name)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    }

    pub fn is_img(&self) -> bool {
        !self.is_folder
            && matches!(
                std::path::Path::new(&self.name)
                    .extension()
                    .and_then(|e| e.to_str()),
                Some("png" | "jpg" | "gif")
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    #[serde(rename = "pcloud")]
    PCloud,
    #[serde(rename = "dropbox")]
    Dropbox,
    // Implemented and functional (drive.readonly, cf. gdrive.rs), but hidden from the
    // connect UI (src/lib/providers.ts PROVIDERS omits it from the selectable list) since
    // 2026-09: Google requires drive.readonly to pass an ADA-CASA AL1 security assessment,
    // ~540$/year recurring plus multi-week turnaround (drive.file was tried first — doesn't
    // work, cf. authorize_url's comment below) — disproportionate for this project. Left
    // wired up in case that calculus changes; re-enabling is just restoring the UI entry.
    #[serde(rename = "gdrive")]
    GoogleDrive,
    /// A folder on this machine — local disk or a network mount (mapped drive, UNC path,
    /// /mnt/…). No OAuth: "connecting" just picks the root folder, cf. local.rs.
    #[serde(rename = "local")]
    Local,
}

impl Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ProviderId::PCloud => "pcloud",
            ProviderId::Dropbox => "dropbox",
            ProviderId::GoogleDrive => "gdrive",
            ProviderId::Local => "local",
        })
    }
}

impl FromStr for ProviderId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pcloud" => Ok(ProviderId::PCloud),
            "dropbox" => Ok(ProviderId::Dropbox),
            "gdrive" => Ok(ProviderId::GoogleDrive),
            "local" => Ok(ProviderId::Local),
            _ => anyhow::bail!("unknown provider id: {s}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadTarget {
    /// Remote file, fetched with reqwest (`headers` sent on every Range request).
    Http {
        url: String,
        headers: Vec<(String, String)>,
    },
    /// File readable straight from this machine's filesystem (local provider).
    File(std::path::PathBuf),
}

impl DownloadTarget {
    pub fn url_only(url: String) -> Self {
        Self::Http {
            url,
            headers: Vec::new(),
        }
    }
}

pub trait StorageProvider: Send + Sync {
    fn provider_id(&self) -> ProviderId;

    // Almost every provider will provide download URL generation so we can enable this by default
    // to skip this call when possible
    fn is_cacheable(&self) -> bool {
        true
    }

    fn list_folder(
        &self,
        folder_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<Entry>>> + Send;

    fn resolve_download_url(
        &self,
        file_id: &str,
        transcoded: bool,
    ) -> impl std::future::Future<Output = Result<DownloadTarget>> + Send;
}
pub trait ProviderAuth {
    /// `state` and `code_challenge` (PKCE S256) come from the [`oauth::LoopbackSession`].
    /// A provider without PKCE support ignores `code_challenge` (and `code_verifier` below).
    fn authorize_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        code_challenge: &str,
    ) -> String;

    fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
        client_id: &str,
        client_secret: &str,
    ) -> impl std::future::Future<Output = Result<StoredCredentials>> + Send;

    fn refresh(
        &self,
        creds: &StoredCredentials,
        client_id: &str,
        client_secret: &str,
    ) -> impl std::future::Future<Output = Result<StoredCredentials>> + Send;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
}

pub struct RangeResponse {
    pub status: u16,
    pub content_type: String,
    pub content_range: Option<String>,
    pub body: Vec<u8>,
}

/// 128 bits from the OS CSPRNG (uuid v4 → getrandom), as 32 hex chars. For OAuth `state`/PKCE
/// and the Linux media server's per-session token.
pub fn random_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Replace every `http://`/`https://` URL in `message` with `<url>`.
///
/// Error messages crossing into the WebView (or the logs) go through this: reqwest puts the full
/// request URL in its `Display`, and for pCloud/Dropbox that URL is a pre-signed download link
/// — as good as a token (cf. ARCHITECTURE.md §3).
pub fn redact_urls(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = ["http://", "https://"]
        .iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
    {
        out.push_str(&rest[..start]);
        out.push_str("<url>");
        let url = &rest[start..];
        let end = url
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '"' | '\'' | '>'))
            .unwrap_or(url.len());
        rest = &url[end..];
    }
    out.push_str(rest);
    out
}

pub async fn fetch_range(
    http: &reqwest::Client,
    target: &DownloadTarget,
    range_header: Option<&str>,
) -> Result<RangeResponse> {
    let (url, headers) = match target {
        DownloadTarget::Http { url, headers } => (url, headers),
        DownloadTarget::File(path) => return local::read_range(path, range_header).await,
    };
    let mut req = http.get(url);

    for (key, value) in headers {
        req = req.header(key, value);
    }

    if let Some(r) = range_header {
        req = req.header("Range", r);
    }

    // without_url: the target URL is a pre-signed link — keep it out of the error message.
    let resp = req.send().await.map_err(reqwest::Error::without_url)?;
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
    let body = resp
        .bytes()
        .await
        .map_err(reqwest::Error::without_url)?
        .to_vec();

    Ok(RangeResponse {
        status,
        content_type,
        content_range,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_urls_strips_reqwest_style_urls() {
        let msg = "error sending request for url (https://p-def1.pcloud.com/abc/x.mp4?key=s3cr3t)";
        assert_eq!(redact_urls(msg), "error sending request for url (<url>)");
    }

    #[test]
    fn redact_urls_handles_several_urls_and_plain_text() {
        assert_eq!(
            redact_urls("a http://x/y b https://z c"),
            "a <url> b <url> c"
        );
        assert_eq!(redact_urls("no url here"), "no url here");
    }

    #[test]
    fn random_token_is_32_hex_and_unique() {
        let a = random_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, random_token());
    }

    #[test]
    fn provider_id_round_trips_through_display_and_from_str() {
        for p in [
            ProviderId::PCloud,
            ProviderId::Dropbox,
            ProviderId::GoogleDrive,
            ProviderId::Local,
        ] {
            assert_eq!(p.to_string().parse::<ProviderId>().unwrap(), p);
        }
        assert!("onedrive".parse::<ProviderId>().is_err());
    }
}
