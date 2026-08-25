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

    /// Restore token from the credential store on startup.
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

    /// Persist token to the credential store and keep it in memory.
    pub fn save(&mut self, provider: ProviderId, credentials: StoredCredentials) -> Result<()> {
        let active = StoredAuth {
            provider,
            credentials,
        };

        Entry::new(KEYRING_SERVICE, KEYRING_USER)?
            .set_password(&serde_json::to_string(&active).unwrap())?;

        self.active = Some(active);

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
    use std::sync::{Mutex, Once};

    static INIT: Once = Once::new();
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// `AuthStore` always targets the same fixed `<service, user>` pair, and the mock
    /// store is process-global, so tests that touch the keyring must run one at a time.
    fn lock_mock_store() -> std::sync::MutexGuard<'static, ()> {
        INIT.call_once(|| {
            keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
        });
        TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

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
    fn missing_credential_load_returns_false() {
        let _guard = lock_mock_store();
        let mut store = AuthStore::new();
        let _ = store.clear(); // clean slate regardless of test execution order

        assert!(!store.load_from_keychain().unwrap());
        assert_eq!(store.active_provider(), None);
    }

    #[test]
    fn save_and_reload_round_trip() {
        let _guard = lock_mock_store();
        let mut store = AuthStore::new();
        store
            .save(ProviderId::PCloud, sample_credentials())
            .unwrap();

        let mut reopened = AuthStore::new();
        assert!(reopened.load_from_keychain().unwrap());
        assert_eq!(reopened.active_provider(), Some(ProviderId::PCloud));
        assert_eq!(reopened.credentials(), Some(&sample_credentials()));

        reopened.clear().unwrap();
    }

    #[test]
    fn cleared_token_is_unauthenticated() {
        let _guard = lock_mock_store();
        let mut store = AuthStore::new();
        store
            .save(ProviderId::PCloud, sample_credentials())
            .unwrap();

        store.clear().unwrap();

        assert_eq!(store.active_provider(), None);
        assert!(!store.load_from_keychain().unwrap());
    }
}
