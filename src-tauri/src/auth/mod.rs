use anyhow::Result;
use chelou_providers::{ProviderId, StoredCredentials};
use keyring_core::Entry;
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "chelou-track";
const KEYRING_USER: &str = "active-provider-token";

#[derive(Serialize, Deserialize)]
struct StoredAuth {
    provider: ProviderId,
    credentials: StoredCredentials,
}

pub struct AuthStore {
    active: Option<StoredAuth>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self { active: None }
    }

    /// Restore token from OS keychain on startup.
    pub fn load_from_keychain(&mut self) -> Result<bool> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)?;

        match entry.get_password() {
            Ok(token) => match serde_json::from_str(token.as_str()) {
                Ok(active) => {
                    self.active = active;

                    Ok(true)
                }
                Err(_) => Ok(false),
            },
            Err(keyring_core::Error::NoEntry) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Persist token to OS keychain and keep it in memory.
    pub fn save(&mut self, provider: ProviderId, credentials: StoredCredentials) -> Result<()> {
        Entry::new(KEYRING_SERVICE, KEYRING_USER)?.set_password(
            &serde_json::to_string(&StoredAuth {
                provider,
                credentials: credentials.clone(),
            })
            .unwrap(),
        )?;

        self.active = Some(StoredAuth {
            provider,
            credentials,
        });

        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => {}
            Err(e) => return Err(e.into()),
        }
        self.active = None;
        Ok(())
    }

    pub fn credentials(&self) -> Option<&StoredCredentials> {
        self.active.as_ref().map(|a| &a.credentials)
    }

    pub fn active_provider(&self) -> Option<ProviderId> {
        self.active.as_ref().map(|a| a.provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_credentials() -> StoredCredentials {
        StoredCredentials {
            access_token: "tok_abc123".into(),
            refresh_token: None,
            expires_at: None,
        }
    }

    #[test]
    fn new_store_is_unauthenticated() {
        let store = AuthStore::new();

        assert_eq!(store.active_provider(), None);
        assert!(store.credentials().is_none());
    }

    #[test]
    fn token_set_directly_authenticates() {
        let mut store = AuthStore::new();
        store.active = Some(StoredAuth {
            provider: ProviderId::PCloud,
            credentials: sample_credentials(),
        });

        assert_eq!(store.active_provider(), Some(ProviderId::PCloud));
        assert_eq!(store.credentials(), Some(&sample_credentials()));
    }

    #[test]
    fn cleared_token_is_unauthenticated() {
        let mut store = AuthStore::new();
        store.active = Some(StoredAuth {
            provider: ProviderId::PCloud,
            credentials: sample_credentials(),
        });
        store.active = None;

        assert_eq!(store.active_provider(), None);
    }
}
