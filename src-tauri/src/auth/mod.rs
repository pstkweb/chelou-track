use anyhow::Result;

const KEYRING_SERVICE: &str = "chelou-track";
const KEYRING_USER: &str = "pcloud-token";

pub struct AuthStore {
    token: Option<String>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self { token: None }
    }

    /// Restore token from OS keychain on startup.
    pub fn load_from_keychain(&mut self) -> Result<bool> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.get_password() {
            Ok(token) => {
                self.token = Some(token);
                Ok(true)
            }
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Persist token to OS keychain and keep it in memory.
    pub fn save_token(&mut self, token: String) -> Result<()> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?.set_password(&token)?;
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

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_store_is_unauthenticated() {
        let store = AuthStore::new();
        assert!(!store.is_authenticated());
        assert!(store.token().is_none());
    }

    #[test]
    fn token_set_directly_authenticates() {
        let mut store = AuthStore::new();
        store.token = Some("tok_abc123".into());
        assert!(store.is_authenticated());
        assert_eq!(store.token(), Some("tok_abc123"));
    }

    #[test]
    fn cleared_token_is_unauthenticated() {
        let mut store = AuthStore::new();
        store.token = Some("tok_abc123".into());
        store.token = None;
        assert!(!store.is_authenticated());
    }
}
