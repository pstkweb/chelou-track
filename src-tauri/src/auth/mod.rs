use anyhow::Result;
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "chelou-track";
const KEYRING_USER: &str = "pcloud-credentials";

/// pCloud credentials persisted via the OS keychain (Windows Credential Manager).
/// Password is never written in cleartext on disk — keyring handles encryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredentials {
    pub username: String,
    pub password: String,
}

pub struct AuthStore {
    credentials: Option<StoredCredentials>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self { credentials: None }
    }

    /// Try to restore credentials from the OS keychain on startup.
    pub fn load_from_keychain(&mut self) -> Result<bool> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.get_password() {
            Ok(json) => {
                self.credentials = serde_json::from_str(&json)?;
                Ok(true)
            }
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_credentials(&mut self, creds: StoredCredentials) -> Result<()> {
        let json = serde_json::to_string(&creds)?;
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?.set_password(&json)?;
        self.credentials = Some(creds);
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.into()),
        }
        self.credentials = None;
        Ok(())
    }

    pub fn credentials(&self) -> Option<&StoredCredentials> {
        self.credentials.as_ref()
    }

    pub fn is_authenticated(&self) -> bool {
        self.credentials.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_store_is_unauthenticated() {
        let store = AuthStore::new();
        assert!(!store.is_authenticated());
        assert!(store.credentials().is_none());
    }

    /// Verifies the JSON shape stored in the keyring doesn't silently change.
    #[test]
    fn stored_credentials_json_roundtrip() {
        let creds = StoredCredentials {
            username: "user@example.com".into(),
            password: "s3cr3t".into(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let restored: StoredCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.username, creds.username);
        assert_eq!(restored.password, creds.password);
    }

    /// Ensures the JSON contains the expected keys (keyring stores this verbatim).
    #[test]
    fn stored_credentials_json_keys() {
        let creds = StoredCredentials {
            username: "u".into(),
            password: "p".into(),
        };
        let v: serde_json::Value = serde_json::to_value(&creds).unwrap();
        assert!(
            v.get("username").is_some(),
            "key 'username' must be present"
        );
        assert!(
            v.get("password").is_some(),
            "key 'password' must be present"
        );
    }
}
