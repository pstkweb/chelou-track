#![cfg(windows)]

use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use keyring_core::api::{CredentialApi, CredentialPersistence, CredentialStoreApi};
use keyring_core::{Credential, Entry, Error, Result};
use windows_dpapi::{decrypt_data, encrypt_data, Scope};

/// Binds the encrypted file to this app specifically (defense in depth, not a secret in
/// itself — baked into the binary). Decryption itself is tied to the current Windows
/// user account via DPAPI (`Scope::User`), which is what actually keeps the token safe.
const DPAPI_ENTROPY: &[u8] = b"chelou-track-auth-v1";

/// A `keyring-core` store backed by DPAPI-encrypted files, one per `<service, user>`
/// pair, in `dir`. Unlike Windows Credential Manager, there's no ~2560 UTF-16 char
/// blob-size ceiling — the whole reason this store exists.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { dir })
    }

    fn credential_path(&self, service: &str, user: &str) -> PathBuf {
        self.dir.join(format!("{service}.{user}.cred"))
    }
}

impl CredentialStoreApi for Store {
    fn vendor(&self) -> String {
        String::from("chelou-track DPAPI file store")
    }

    fn id(&self) -> String {
        String::from("keyring-dpapi-store-v1")
    }

    fn build(
        &self,
        service: &str,
        user: &str,
        _modifiers: Option<&HashMap<&str, &str>>,
    ) -> Result<Entry> {
        let cred = DpapiCredential {
            path: self.credential_path(service, user),
            service: service.to_string(),
            user: user.to_string(),
        };
        Ok(Entry::new_with_credential(Arc::new(cred)))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn persistence(&self) -> CredentialPersistence {
        CredentialPersistence::UntilDelete
    }
}

struct DpapiCredential {
    path: PathBuf,
    service: String,
    user: String,
}

impl CredentialApi for DpapiCredential {
    fn set_secret(&self, secret: &[u8]) -> Result<()> {
        let encrypted = encrypt_data(secret, Scope::User, Some(DPAPI_ENTROPY))
            .map_err(|e| Error::PlatformFailure(Box::new(std::io::Error::other(e.to_string()))))?;
        std::fs::write(&self.path, encrypted).map_err(|e| Error::PlatformFailure(Box::new(e)))
    }

    fn get_secret(&self) -> Result<Vec<u8>> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Error::NoEntry),
            Err(e) => return Err(Error::PlatformFailure(Box::new(e))),
        };
        decrypt_data(&bytes, Scope::User, Some(DPAPI_ENTROPY))
            .map_err(|e| Error::PlatformFailure(Box::new(std::io::Error::other(e.to_string()))))
    }

    fn delete_credential(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::NoEntry),
            Err(e) => Err(Error::PlatformFailure(Box::new(e))),
        }
    }

    fn get_credential(&self) -> Result<Option<Arc<Credential>>> {
        // No ambiguity possible in this store (one file per <service, user>), so `self`
        // is already the concrete credential — same convention as keyring-core's own
        // `mock::Cred::get_credential`.
        self.get_secret()?;
        Ok(None)
    }

    fn get_specifiers(&self) -> Option<(String, String)> {
        Some((self.service.clone(), self.user.clone()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Store;
    use keyring_core::api::CredentialStoreApi;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "keyring-dpapi-store-test-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_a_secret() {
        let store = Store::new(temp_dir());
        let entry = store.build("svc", "user", None).unwrap();

        entry.set_password("hunter2").unwrap();
        assert_eq!(entry.get_password().unwrap(), "hunter2");

        entry.delete_credential().unwrap();
        assert!(matches!(
            entry.get_password(),
            Err(keyring_core::Error::NoEntry)
        ));
    }

    #[test]
    fn missing_credential_is_no_entry() {
        let store = Store::new(temp_dir());
        let entry = store.build("svc", "user", None).unwrap();

        assert!(matches!(
            entry.get_password(),
            Err(keyring_core::Error::NoEntry)
        ));
    }

    #[test]
    fn overwriting_a_secret_updates_it() {
        let store = Store::new(temp_dir());
        let entry = store.build("svc", "user", None).unwrap();

        entry.set_password("first").unwrap();
        entry.set_password("second").unwrap();

        assert_eq!(entry.get_password().unwrap(), "second");
    }

    #[test]
    fn survives_secrets_larger_than_credential_manager_allows() {
        let store = Store::new(temp_dir());
        let entry = store.build("svc", "user", None).unwrap();

        // Comfortably over Windows Credential Manager's ~2560 UTF-16 char limit —
        // the whole reason this store exists instead of the native Windows backend.
        let big = "x".repeat(6000);
        entry.set_password(&big).unwrap();

        assert_eq!(entry.get_password().unwrap(), big);
    }

    #[test]
    fn secret_is_not_stored_in_plaintext() {
        let store = Store::new(temp_dir());
        let entry = store.build("svc", "user", None).unwrap();
        entry.set_password("hunter2").unwrap();

        let path = store.credential_path("svc", "user");
        let raw = std::fs::read(&path).unwrap();

        assert!(!raw.windows(7).any(|w| w == b"hunter2"));
    }
}
