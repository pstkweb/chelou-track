pub mod scanner;

use anyhow::{anyhow, Result};
use serde::Deserialize;

/// EU endpoint — never the US one (cf. ARCHITECTURE.md §3).
pub const API_BASE: &str = "https://eapi.pcloud.com";

pub struct PCloudClient {
    http: reqwest::Client,
    pub token: String,
}

impl PCloudClient {
    pub fn new(token: String) -> Self {
        Self { http: reqwest::Client::new(), token }
    }

    /// Authenticate and return (token, uid).
    pub async fn login(username: &str, password: &str) -> Result<(String, u64)> {
        let resp: serde_json::Value = reqwest::Client::new()
            .get(format!("{API_BASE}/userinfo"))
            .query(&[
                ("getauth", "1"),
                ("logout", "1"),
                ("username", username),
                ("password", password),
                ("authexpire", "0"),
            ])
            .send()
            .await?
            .json()
            .await?;

        if resp["result"].as_i64().unwrap_or(-1) != 0 {
            return Err(anyhow!("pCloud login error: {}", resp["error"]));
        }
        let token = resp["token"].as_str().ok_or_else(|| anyhow!("no token"))?.to_owned();
        let uid   = resp["userid"].as_u64().ok_or_else(|| anyhow!("no userid"))?;
        Ok((token, uid))
    }

    /// List a folder (shallow). Use scanner::scan_tree for recursive DFS.
    pub async fn list_folder(&self, folder_id: u64) -> Result<FolderContents> {
        let resp: serde_json::Value = self.http
            .get(format!("{API_BASE}/listfolder"))
            .query(&[("auth", self.token.as_str()), ("folderid", &folder_id.to_string())])
            .send()
            .await?
            .json()
            .await?;

        if resp["result"].as_i64().unwrap_or(-1) != 0 {
            return Err(anyhow!("listfolder error: {}", resp["error"]));
        }
        Ok(serde_json::from_value(resp["metadata"].clone())?)
    }

    /// Direct download link for a file (same IP generates and consumes — cf. §3).
    pub async fn get_file_link(&self, file_id: u64) -> Result<String> {
        let resp: serde_json::Value = self.http
            .get(format!("{API_BASE}/getfilelink"))
            .query(&[("auth", self.token.as_str()), ("fileid", &file_id.to_string())])
            .send()
            .await?
            .json()
            .await?;

        if resp["result"].as_i64().unwrap_or(-1) != 0 {
            return Err(anyhow!("getfilelink error: {}", resp["error"]));
        }
        let host = resp["hosts"][0].as_str().ok_or_else(|| anyhow!("no host"))?;
        let path = resp["path"].as_str().ok_or_else(|| anyhow!("no path"))?;
        Ok(format!("https://{host}{path}"))
    }

    /// Transcoded video link — fallback when H.265 fails to decode (cf. §4 + §11).
    pub async fn get_video_link(&self, file_id: u64) -> Result<String> {
        let resp: serde_json::Value = self.http
            .get(format!("{API_BASE}/getvideolink"))
            .query(&[("auth", self.token.as_str()), ("fileid", &file_id.to_string())])
            .send()
            .await?
            .json()
            .await?;

        if resp["result"].as_i64().unwrap_or(-1) != 0 {
            return Err(anyhow!("getvideolink error: {}", resp["error"]));
        }
        let host = resp["hosts"][0].as_str().ok_or_else(|| anyhow!("no host"))?;
        let path = resp["path"].as_str().ok_or_else(|| anyhow!("no path"))?;
        Ok(format!("https://{host}{path}"))
    }

    /// Forward a Range-aware fetch from a pCloud direct link (for the stream:// handler).
    pub async fn fetch_range(
        &self,
        url: &str,
        range_header: Option<&str>,
    ) -> Result<reqwest::Response> {
        let mut req = self.http.get(url);
        if let Some(r) = range_header {
            req = req.header("Range", r);
        }
        Ok(req.send().await?)
    }
}

// --- API response types ---

#[derive(Debug, Deserialize)]
pub struct FolderContents {
    pub folderid: u64,
    pub name: String,
    pub contents: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub name: String,
    pub isfolder: bool,
    pub fileid: Option<u64>,
    pub folderid: Option<u64>,
    pub size: Option<u64>,
}

impl Entry {
    pub fn is_video(&self) -> bool {
        !self.isfolder && matches!(
            std::path::Path::new(&self.name)
                .extension()
                .and_then(|e| e.to_str()),
            Some("mp4" | "mkv" | "avi" | "mov")
        )
    }

    pub fn is_audio(&self) -> bool {
        !self.isfolder && matches!(
            std::path::Path::new(&self.name)
                .extension()
                .and_then(|e| e.to_str()),
            Some("wav" | "mp3" | "flac")
        )
    }

    pub fn is_tab(&self) -> bool {
        !self.isfolder && matches!(
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
                .map_or(false, |e| e.eq_ignore_ascii_case("pdf"))
    }
}
