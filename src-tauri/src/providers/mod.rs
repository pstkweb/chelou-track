use anyhow::Result;
use chelou_providers::{
    pcloud::{PCloudAuth, PCloudClient},
    FolderContents, ProviderAuth, ProviderId, StorageProvider, StoredCredentials,
};

pub enum ProviderClient {
    PCloud(chelou_providers::pcloud::PCloudClient),
}

impl StorageProvider for ProviderClient {
    fn provider_id(&self) -> ProviderId {
        match self {
            Self::PCloud(c) => c.provider_id(),
        }
    }

    async fn list_folder(&self, folder_id: &str) -> Result<FolderContents> {
        match self {
            Self::PCloud(c) => c.list_folder(folder_id).await,
        }
    }

    async fn resolve_download_url(&self, file_id: &str, transcoded: bool) -> Result<String> {
        match self {
            Self::PCloud(c) => c.resolve_download_url(file_id, transcoded).await,
        }
    }
}

pub fn make_client(provider: ProviderId, creds: &StoredCredentials) -> Result<ProviderClient> {
    match provider {
        ProviderId::PCloud => Ok(ProviderClient::PCloud(PCloudClient::new(
            creds.access_token.clone(),
        )?)),
    }
}

pub enum ProviderAuthClient {
    PCloud(chelou_providers::pcloud::PCloudAuth),
}

impl ProviderAuth for ProviderAuthClient {
    fn authorize_url(&self, client_id: &str, redirect_uri: &str) -> String {
        match self {
            Self::PCloud(a) => a.authorize_url(client_id, redirect_uri),
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
        }
    }
}

pub fn make_auth(provider: ProviderId) -> ProviderAuthClient {
    match provider {
        ProviderId::PCloud => ProviderAuthClient::PCloud(PCloudAuth),
    }
}
