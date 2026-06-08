use anyhow::Result;
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "chelou-track";
const KEYRING_USER: &str = "pcloud-token";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    pub uid: u64,
}

/// Wraps the pCloud auth token; persists via the OS keychain (Windows Credential Manager).
/// Alternative: tauri-plugin-stronghold for an encrypted local vault.
pub struct AuthStore {
    token: Option<AuthToken>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self { token: None }
    }

    /// Try to load a previously stored token from the OS keychain.
    pub fn load_from_keychain(&mut self) -> Result<bool> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.get_password() {
            Ok(json) => {
                self.token = serde_json::from_str(&json)?;
                Ok(true)
            }
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_token(&mut self, token: AuthToken) -> Result<()> {
        let json = serde_json::to_string(&token)?;
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?.set_password(&json)?;
        self.token = Some(token);
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.into()),
        }
        self.token = None;
        Ok(())
    }

    pub fn token(&self) -> Option<&AuthToken> {
        self.token.as_ref()
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }
}
