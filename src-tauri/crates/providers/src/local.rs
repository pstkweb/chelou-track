// Local provider: a folder on this machine — local disk or network mount (mapped drive, UNC
// path, /mnt/nas…), anything the OS exposes as a regular directory. No OAuth, no token:
// "connecting" is picking the root folder (`connect_local` command), stored in place of the
// access token (`StoredCredentials::access_token`) so the rest of the auth/client-cache
// machinery stays uniform — cf. ARCHITECTURE.md §14.
//
// Ids are opaque: base64url (no padding) of the path *relative to the root*, '/'-separated.
// That keeps them URL-safe in `stream://…/{fileId}` (spaces, accents, '#', '?' in file names)
// and confines every request to the root — an id is decoded segment by segment and anything
// other than a plain name (`..`, absolute path, drive prefix) is rejected.
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::{DownloadTarget, Entry, ProviderId, RangeResponse, StorageProvider};

pub struct LocalClient {
    root: PathBuf,
}

impl LocalClient {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Absolute path for `id`, guaranteed to be `root` or below it.
    fn resolve(&self, id: &str) -> Result<PathBuf> {
        let rel = decode_id(id)?;
        let mut path = self.root.clone();
        if rel.is_empty() {
            return Ok(path);
        }
        for segment in rel.split('/') {
            let mut components = Path::new(segment).components();
            let plain = matches!(components.next(), Some(Component::Normal(_)))
                && components.next().is_none()
                && !segment.contains(['/', '\\', ':']);
            if !plain {
                bail!("invalid local file id");
            }
            path.push(segment);
        }
        Ok(path)
    }
}

pub fn encode_id(rel_path: &str) -> String {
    URL_SAFE_NO_PAD.encode(rel_path)
}

fn decode_id(id: &str) -> Result<String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(id)
        .map_err(|_| anyhow!("invalid local file id"))?;
    String::from_utf8(bytes).map_err(|_| anyhow!("invalid local file id"))
}

impl StorageProvider for LocalClient {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Local
    }

    // Resolving is a pure path join — nothing worth caching.
    fn is_cacheable(&self) -> bool {
        false
    }

    async fn list_folder(&self, folder_id: &str) -> Result<Vec<Entry>> {
        let rel = decode_id(folder_id)?;
        let dir = self.resolve(folder_id)?;
        let mut read_dir = tokio::fs::read_dir(&dir)
            .await
            .with_context(|| format!("lecture impossible de {}", dir.display()))?;

        let mut entries = Vec::new();
        while let Some(child) = read_dir.next_entry().await? {
            // Non-UTF-8 names can't round-trip through a string id — skip them, like dotfiles
            // (.DS_Store, .git, …) which are never method content.
            let Ok(name) = child.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            // metadata() follows symlinks; an unreadable entry (broken link, permissions) is
            // skipped rather than failing the whole listing.
            let Ok(meta) = tokio::fs::metadata(child.path()).await else {
                continue;
            };
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            entries.push(Entry {
                id: encode_id(&child_rel),
                name,
                is_folder: meta.is_dir(),
                size: meta.is_file().then_some(meta.len()),
            });
        }
        Ok(entries)
    }

    async fn resolve_download_url(
        &self,
        file_id: &str,
        transcoded: bool,
    ) -> Result<DownloadTarget> {
        if transcoded {
            bail!("pas de version transcodée pour un fichier local");
        }
        Ok(DownloadTarget::File(self.resolve(file_id)?))
    }
}

// --- Byte ranges ---

/// Inclusive byte span to serve for `range` over a `total`-byte file.
/// `Ok(None)` = whole file: no Range header, or one we don't handle (multi-range, malformed) —
/// a 200 with the full body is always a valid answer to those. `Err(())` = unsatisfiable (416).
fn parse_range(range: Option<&str>, total: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = range.and_then(|r| r.trim().strip_prefix("bytes=")) else {
        return Ok(None);
    };
    if spec.contains(',') {
        return Ok(None);
    }
    let Some((start, end)) = spec.split_once('-') else {
        return Ok(None);
    };
    let (start, end) = (start.trim(), end.trim());

    if start.is_empty() {
        // Suffix range: the last N bytes.
        let Ok(n) = end.parse::<u64>() else {
            return Ok(None);
        };
        if n == 0 || total == 0 {
            return Err(());
        }
        return Ok(Some((total.saturating_sub(n), total - 1)));
    }

    let Ok(start) = start.parse::<u64>() else {
        return Ok(None);
    };
    if start >= total {
        return Err(());
    }
    let end = if end.is_empty() {
        total - 1
    } else {
        match end.parse::<u64>() {
            Ok(e) if e >= start => e.min(total - 1),
            _ => return Ok(None),
        }
    };
    Ok(Some((start, end)))
}

fn content_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("mp4") => "video/mp4",
        Some("mkv") => "video/x-matroska",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/x-msvideo",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// A file opened and positioned for an HTTP-style (Range) response: read `len` bytes from
/// `file` to get the body.
pub struct OpenedRange {
    pub file: tokio::fs::File,
    pub status: u16,
    pub len: u64,
    pub content_type: &'static str,
    pub content_range: Option<String>,
}

/// Open `path` and seek to the span `range_header` asks for. Shared by the buffered
/// `read_range` (stream://) and the Linux media server, which streams `file` itself.
pub async fn open_range(path: &Path, range_header: Option<&str>) -> Result<OpenedRange> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("ouverture impossible de {}", path.display()))?;
    let total = file.metadata().await?.len();
    let content_type = content_type(path);

    let (status, start, len, content_range) = match parse_range(range_header, total) {
        Ok(None) => (200, 0, total, None),
        Ok(Some((start, end))) => (
            206,
            start,
            end - start + 1,
            Some(format!("bytes {start}-{end}/{total}")),
        ),
        Err(()) => (416, 0, 0, Some(format!("bytes */{total}"))),
    };
    file.seek(std::io::SeekFrom::Start(start)).await?;

    Ok(OpenedRange {
        file,
        status,
        len,
        content_type,
        content_range,
    })
}

/// `fetch_range` for a local file: the requested span, buffered in memory.
pub async fn read_range(path: &Path, range_header: Option<&str>) -> Result<RangeResponse> {
    let opened = open_range(path, range_header).await?;
    let mut body = Vec::with_capacity(usize::try_from(opened.len).unwrap_or(0));
    opened.file.take(opened.len).read_to_end(&mut body).await?;

    Ok(RangeResponse {
        status: opened.status,
        content_type: opened.content_type.to_owned(),
        content_range: opened.content_range,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh directory under the OS temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("chelou-local-{}", crate::random_token()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sorted(mut entries: Vec<Entry>) -> Vec<Entry> {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    #[test]
    fn ids_round_trip_and_root_is_empty() {
        assert_eq!(encode_id(""), "");
        let id = encode_id("Méthode #1/01 Intro?.mp4");
        assert!(id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(decode_id(&id).unwrap(), "Méthode #1/01 Intro?.mp4");
    }

    #[test]
    fn resolve_stays_under_root() {
        let client = LocalClient::new(PathBuf::from("root"));
        assert_eq!(client.resolve("").unwrap(), PathBuf::from("root"));
        assert_eq!(
            client.resolve(&encode_id("a/b.mp4")).unwrap(),
            Path::new("root").join("a").join("b.mp4")
        );
        for evil in [
            "..",
            "a/../../x",
            "/etc/passwd",
            "C:/Windows",
            "a\\..\\..",
            ".",
            "f:ads",
        ] {
            assert!(client.resolve(&encode_id(evil)).is_err(), "{evil}");
        }
        assert!(client.resolve("not base64!").is_err());
    }

    #[test]
    fn parse_range_cases() {
        assert_eq!(parse_range(None, 100), Ok(None));
        assert_eq!(parse_range(Some("bytes=0-9"), 100), Ok(Some((0, 9))));
        assert_eq!(parse_range(Some("bytes=90-"), 100), Ok(Some((90, 99))));
        assert_eq!(parse_range(Some("bytes=90-500"), 100), Ok(Some((90, 99))));
        assert_eq!(parse_range(Some("bytes=-10"), 100), Ok(Some((90, 99))));
        assert_eq!(parse_range(Some("bytes=-500"), 100), Ok(Some((0, 99))));
        assert_eq!(parse_range(Some("bytes=100-"), 100), Err(()));
        assert_eq!(parse_range(Some("bytes=-0"), 100), Err(()));
        assert_eq!(parse_range(Some("bytes=0-"), 0), Err(()));
        assert_eq!(parse_range(Some("bytes=9-3"), 100), Ok(None));
        assert_eq!(parse_range(Some("bytes=0-1,5-6"), 100), Ok(None));
        assert_eq!(parse_range(Some("items=0-1"), 100), Ok(None));
    }

    #[tokio::test]
    async fn lists_folders_and_files_skipping_dotfiles() {
        let tmp = TempDir::new();
        std::fs::create_dir(tmp.0.join("Méthode A")).unwrap();
        std::fs::write(tmp.0.join("Méthode A").join("01 Riff.mp4"), b"0123456789").unwrap();
        std::fs::write(tmp.0.join(".DS_Store"), b"").unwrap();
        let client = LocalClient::new(tmp.0.clone());

        let root = sorted(client.list_folder("").await.unwrap());
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].name, "Méthode A");
        assert!(root[0].is_folder);
        assert_eq!(root[0].size, None);

        let sub = client.list_folder(&root[0].id).await.unwrap();
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].name, "01 Riff.mp4");
        assert!(sub[0].is_video());
        assert_eq!(sub[0].size, Some(10));
        assert_eq!(decode_id(&sub[0].id).unwrap(), "Méthode A/01 Riff.mp4");
    }

    #[tokio::test]
    async fn download_target_reads_ranges_from_disk() {
        let tmp = TempDir::new();
        std::fs::write(tmp.0.join("track.wav"), b"0123456789").unwrap();
        let client = LocalClient::new(tmp.0.clone());
        let id = encode_id("track.wav");

        assert!(client.resolve_download_url(&id, true).await.is_err());
        let target = client.resolve_download_url(&id, false).await.unwrap();
        let http = reqwest::Client::new();

        let full = crate::fetch_range(&http, &target, None).await.unwrap();
        assert_eq!(full.status, 200);
        assert_eq!(full.content_type, "audio/wav");
        assert_eq!(full.content_range, None);
        assert_eq!(full.body, b"0123456789");

        let part = crate::fetch_range(&http, &target, Some("bytes=2-4"))
            .await
            .unwrap();
        assert_eq!(part.status, 206);
        assert_eq!(part.content_range.as_deref(), Some("bytes 2-4/10"));
        assert_eq!(part.body, b"234");

        let past_end = crate::fetch_range(&http, &target, Some("bytes=10-"))
            .await
            .unwrap();
        assert_eq!(past_end.status, 416);
        assert_eq!(past_end.content_range.as_deref(), Some("bytes */10"));
        assert!(past_end.body.is_empty());
    }
}
