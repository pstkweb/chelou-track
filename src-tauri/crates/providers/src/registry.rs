use anyhow::Result;

use crate::dropbox::{DropboxAuth, DropboxClient};
use crate::gdrive::{GDriveAuth, GDriveClient};
use crate::pcloud::{PCloudAuth, PCloudClient};
use crate::{DownloadTarget, Entry, ProviderAuth, ProviderId, StorageProvider, StoredCredentials};

pub enum ProviderClient {
    PCloud(PCloudClient),
    Dropbox(DropboxClient),
    GoogleDrive(GDriveClient),
}

impl StorageProvider for ProviderClient {
    fn provider_id(&self) -> ProviderId {
        match self {
            Self::PCloud(c) => c.provider_id(),
            Self::Dropbox(c) => c.provider_id(),
            Self::GoogleDrive(c) => c.provider_id(),
        }
    }

    fn is_cacheable(&self) -> bool {
        match self {
            Self::PCloud(c) => c.is_cacheable(),
            Self::Dropbox(c) => c.is_cacheable(),
            Self::GoogleDrive(c) => c.is_cacheable(),
        }
    }

    async fn list_folder(&self, folder_id: &str) -> Result<Vec<Entry>> {
        match self {
            Self::PCloud(c) => c.list_folder(folder_id).await,
            Self::Dropbox(c) => c.list_folder(folder_id).await,
            Self::GoogleDrive(c) => c.list_folder(folder_id).await,
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
    }
}

pub enum ProviderAuthClient {
    PCloud(PCloudAuth),
    Dropbox(DropboxAuth),
    GoogleDrive(GDriveAuth),
}

impl ProviderAuth for ProviderAuthClient {
    fn authorize_url(&self, client_id: &str, redirect_uri: &str) -> String {
        match self {
            Self::PCloud(a) => a.authorize_url(client_id, redirect_uri),
            Self::Dropbox(a) => a.authorize_url(client_id, redirect_uri),
            Self::GoogleDrive(a) => a.authorize_url(client_id, redirect_uri),
        }
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        client_id: &str,
        client_secret: &str,
    ) -> Result<StoredCredentials> {
        match self {
            Self::PCloud(a) => {
                a.exchange_code(code, redirect_uri, client_id, client_secret)
                    .await
            }
            Self::Dropbox(a) => {
                a.exchange_code(code, redirect_uri, client_id, client_secret)
                    .await
            }
            Self::GoogleDrive(a) => {
                a.exchange_code(code, redirect_uri, client_id, client_secret)
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

pub fn make_auth(provider: ProviderId) -> ProviderAuthClient {
    match provider {
        ProviderId::PCloud => ProviderAuthClient::PCloud(PCloudAuth),
        ProviderId::Dropbox => ProviderAuthClient::Dropbox(DropboxAuth),
        ProviderId::GoogleDrive => ProviderAuthClient::GoogleDrive(GDriveAuth),
    }
}
