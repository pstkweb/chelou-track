use anyhow::Result;

use crate::dropbox::{DropboxAuth, DropboxClient};
use crate::gdrive::{GDriveAuth, GDriveClient};
use crate::local::LocalClient;
use crate::pcloud::{PCloudAuth, PCloudClient};
use crate::{DownloadTarget, Entry, ProviderAuth, ProviderId, StorageProvider, StoredCredentials};

pub enum ProviderClient {
    PCloud(PCloudClient),
    Dropbox(DropboxClient),
    GoogleDrive(GDriveClient),
    Local(LocalClient),
}

impl StorageProvider for ProviderClient {
    fn provider_id(&self) -> ProviderId {
        match self {
            Self::PCloud(c) => c.provider_id(),
            Self::Dropbox(c) => c.provider_id(),
            Self::GoogleDrive(c) => c.provider_id(),
            Self::Local(c) => c.provider_id(),
        }
    }

    fn is_cacheable(&self) -> bool {
        match self {
            Self::PCloud(c) => c.is_cacheable(),
            Self::Dropbox(c) => c.is_cacheable(),
            Self::GoogleDrive(c) => c.is_cacheable(),
            Self::Local(c) => c.is_cacheable(),
        }
    }

    async fn list_folder(&self, folder_id: &str) -> Result<Vec<Entry>> {
        match self {
            Self::PCloud(c) => c.list_folder(folder_id).await,
            Self::Dropbox(c) => c.list_folder(folder_id).await,
            Self::GoogleDrive(c) => c.list_folder(folder_id).await,
            Self::Local(c) => c.list_folder(folder_id).await,
        }
    }

    async fn resolve_download_url(
        &self,
        file_id: &str,
        transcoded: bool,
    ) -> Result<DownloadTarget> {
        match self {
            Self::PCloud(c) => c.resolve_download_url(file_id, transcoded).await,
            Self::Dropbox(c) => c.resolve_download_url(file_id, transcoded).await,
            Self::GoogleDrive(c) => c.resolve_download_url(file_id, transcoded).await,
            Self::Local(c) => c.resolve_download_url(file_id, transcoded).await,
        }
    }
}

pub fn make_client(provider: ProviderId, creds: &StoredCredentials) -> Result<ProviderClient> {
    match provider {
        ProviderId::PCloud => Ok(ProviderClient::PCloud(PCloudClient::new(
            creds.access_token.clone(),
        )?)),
        ProviderId::Dropbox => Ok(ProviderClient::Dropbox(DropboxClient::new(
            creds.access_token.clone(),
        )?)),
        ProviderId::GoogleDrive => Ok(ProviderClient::GoogleDrive(GDriveClient::new(
            creds.access_token.clone(),
        )?)),
        // No token for a local folder: the "access token" slot holds the root path (cf. local.rs).
        ProviderId::Local => Ok(ProviderClient::Local(LocalClient::new(
            creds.access_token.clone().into(),
        ))),
    }
}

pub enum ProviderAuthClient {
    PCloud(PCloudAuth),
    Dropbox(DropboxAuth),
    GoogleDrive(GDriveAuth),
}

impl ProviderAuth for ProviderAuthClient {
    fn authorize_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        code_challenge: &str,
    ) -> String {
        match self {
            Self::PCloud(a) => a.authorize_url(client_id, redirect_uri, state, code_challenge),
            Self::Dropbox(a) => a.authorize_url(client_id, redirect_uri, state, code_challenge),
            Self::GoogleDrive(a) => a.authorize_url(client_id, redirect_uri, state, code_challenge),
        }
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
        client_id: &str,
        client_secret: &str,
    ) -> Result<StoredCredentials> {
        match self {
            Self::PCloud(a) => {
                a.exchange_code(code, redirect_uri, code_verifier, client_id, client_secret)
                    .await
            }
            Self::Dropbox(a) => {
                a.exchange_code(code, redirect_uri, code_verifier, client_id, client_secret)
                    .await
            }
            Self::GoogleDrive(a) => {
                a.exchange_code(code, redirect_uri, code_verifier, client_id, client_secret)
                    .await
            }
        }
    }

    async fn refresh(
        &self,
        creds: &StoredCredentials,
        client_id: &str,
        client_secret: &str,
    ) -> Result<StoredCredentials> {
        match self {
            Self::PCloud(a) => a.refresh(creds, client_id, client_secret).await,
            Self::Dropbox(a) => a.refresh(creds, client_id, client_secret).await,
            Self::GoogleDrive(a) => a.refresh(creds, client_id, client_secret).await,
        }
    }
}

/// Errors for a provider without OAuth (local folder — connected via `connect_local` instead).
pub fn make_auth(provider: ProviderId) -> Result<ProviderAuthClient> {
    match provider {
        ProviderId::PCloud => Ok(ProviderAuthClient::PCloud(PCloudAuth)),
        ProviderId::Dropbox => Ok(ProviderAuthClient::Dropbox(DropboxAuth)),
        ProviderId::GoogleDrive => Ok(ProviderAuthClient::GoogleDrive(GDriveAuth)),
        ProviderId::Local => anyhow::bail!("{provider} has no OAuth flow"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [ProviderId; 4] = [
        ProviderId::PCloud,
        ProviderId::Dropbox,
        ProviderId::GoogleDrive,
        ProviderId::Local,
    ];
    const OAUTH: [ProviderId; 3] = [
        ProviderId::PCloud,
        ProviderId::Dropbox,
        ProviderId::GoogleDrive,
    ];

    fn creds() -> StoredCredentials {
        StoredCredentials {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: None,
        }
    }

    #[test]
    fn make_client_builds_the_requested_provider() {
        for p in ALL {
            assert_eq!(make_client(p, &creds()).unwrap().provider_id(), p);
        }
    }

    #[test]
    fn only_gdrive_and_local_download_targets_are_uncacheable() {
        // GDrive bakes the live access token into the target headers (cf. stream::resolve_url);
        // local resolves with a plain path join, nothing to save.
        for p in ALL {
            let cacheable = make_client(p, &creds()).unwrap().is_cacheable();
            let expected = !matches!(p, ProviderId::GoogleDrive | ProviderId::Local);
            assert_eq!(cacheable, expected, "{p}");
        }
    }

    #[test]
    fn make_auth_rejects_local() {
        assert!(make_auth(ProviderId::Local).is_err());
    }

    #[test]
    fn every_authorize_url_carries_state_and_redirect_uri() {
        for p in OAUTH {
            let url =
                make_auth(p)
                    .unwrap()
                    .authorize_url("cid", "http://localhost:1/cb", "st4te", "ch");
            assert!(url.contains("state=st4te"), "{p}: {url}");
            assert!(
                url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcb"),
                "{p}: {url}"
            );
            assert!(url.contains("client_id=cid"), "{p}: {url}");
        }
    }
}
