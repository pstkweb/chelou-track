pub mod scanner;
use std::{fmt::Display, str::FromStr};

pub use scanner::{scan_methods_in_folder, ScanEvent};
pub mod dropbox;
pub mod oauth;
pub mod pcloud;

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
}

impl Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ProviderId::PCloud => "pcloud",
            ProviderId::Dropbox => "dropbox",
        })
    }
}

impl FromStr for ProviderId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pcloud" => Ok(ProviderId::PCloud),
            "dropbox" => Ok(ProviderId::Dropbox),

            _ => anyhow::bail!("unknown provider id: {s}"),
        }
    }
}

pub trait StorageProvider: Send + Sync {
    fn provider_id(&self) -> ProviderId;

    fn list_folder(
        &self,
        folder_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<Entry>>> + Send;

    fn resolve_download_url(
        &self,
        file_id: &str,
        transcoded: bool,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
}
pub trait ProviderAuth {
    fn authorize_url(&self, client_id: &str, redirect_uri: &str) -> String;

    fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
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

pub async fn fetch_range(
    http: &reqwest::Client,
    url: &str,
    range_header: Option<&str>,
) -> Result<RangeResponse> {
    let mut req = http.get(url);

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
