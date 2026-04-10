use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;
use tracing::{debug, warn};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce as AesNonce};
use aes_gcm::aead::Aead;
use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const CREDENTIALS_FILENAME: &str = "credentials.enc";
const MASTER_KEY_FILENAME: &str = ".master_key";

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialize(String),
    #[error("Credential not found for id: {0}")]
    NotFound(String),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Decryption error: {0}")]
    Decryption(String),
}

/// On-disk format: nonce (12 bytes) + ciphertext + tag (16 bytes), all base64-encoded
#[derive(Debug, Serialize, Deserialize)]
struct EncryptedFile {
    /// base64-encoded nonce + ciphertext + auth tag
    data: String,
    /// Format version for future migrations
    version: u32,
}

/// Encrypted file-based credential store using AES-256-GCM.
///
/// Passwords are stored encrypted at rest using a machine-local master key.
/// The master key is generated once and stored with restrictive file permissions.
///
/// On-disk layout:
///   ~/.config/stealthterm/.master_key    — 32-byte random key (mode 0600)
///   ~/.config/stealthterm/credentials.enc — AES-256-GCM encrypted JSON blob
pub struct CredentialStore {
    credentials: HashMap<String, String>,
    master_key: Vec<u8>,
    config_dir: PathBuf,
}

impl std::fmt::Debug for CredentialStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialStore")
            .field("credentials_count", &self.credentials.len())
            .field("config_dir", &self.config_dir)
            .finish()
    }
}

impl CredentialStore {
    fn default_config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stealthterm")
    }

    /// Load credential store from the default config directory
    pub fn load() -> Result<Self, CredentialError> {
        Self::load_from(Self::default_config_dir())
    }

    /// Load credential store from a specific config directory
    pub fn load_from(config_dir: PathBuf) -> Result<Self, CredentialError> {
        std::fs::create_dir_all(&config_dir)?;

        let master_key = Self::load_or_create_master_key(&config_dir)?;
        let cred_path = config_dir.join(CREDENTIALS_FILENAME);

        let credentials = if cred_path.exists() {
            let content = std::fs::read_to_string(&cred_path)?;
            let enc_file: EncryptedFile = serde_json::from_str(&content)
                .map_err(|e| CredentialError::Serialize(e.to_string()))?;

            Self::decrypt_credentials(&master_key, &enc_file)?
        } else {
            // Try migrating from old plaintext credentials.json
            let legacy_path = config_dir.join("credentials.json");
            if legacy_path.exists() {
                debug!("Migrating legacy plaintext credentials");
                let content = std::fs::read_to_string(&legacy_path)?;
                let legacy: LegacyCredentials = serde_json::from_str(&content)
                    .map_err(|e| CredentialError::Serialize(e.to_string()))?;
                // We'll save encrypted on first store/save call
                legacy.credentials
            } else {
                HashMap::new()
            }
        };

        debug!("Loaded {} credentials from {}", credentials.len(), config_dir.display());

        Ok(Self {
            credentials,
            master_key,
            config_dir,
        })
    }

    /// Load existing master key or generate a new one
    pub fn load_or_create_master_key(config_dir: &PathBuf) -> Result<Vec<u8>, CredentialError> {
        let key_path = config_dir.join(MASTER_KEY_FILENAME);

        if key_path.exists() {
            let encoded = std::fs::read_to_string(&key_path)?;
            let key = BASE64.decode(encoded.trim())
                .map_err(|e| CredentialError::Decryption(format!("Invalid master key: {}", e)))?;
            if key.len() != KEY_LEN {
                return Err(CredentialError::Decryption(
                    format!("Master key has wrong length: {} (expected {})", key.len(), KEY_LEN),
                ));
            }
            debug!("Loaded master key from {}", key_path.display());
            Ok(key)
        } else {
            let mut key = vec![0u8; KEY_LEN];
            rand::rngs::OsRng.fill_bytes(&mut key);

            let encoded = BASE64.encode(&key);
            std::fs::write(&key_path, &encoded)?;

            // Set restrictive permissions (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
            }

            debug!("Generated new master key at {}", key_path.display());
            Ok(key)
        }
    }

    /// Encrypt the credentials map to an EncryptedFile
    fn encrypt_credentials(
        master_key: &[u8],
        credentials: &HashMap<String, String>,
    ) -> Result<EncryptedFile, CredentialError> {
        let plaintext = serde_json::to_string(credentials)
            .map_err(|e| CredentialError::Serialize(e.to_string()))?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(master_key)
            .map_err(|_| CredentialError::Encryption("Invalid key".into()))?;
        let nonce = AesNonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| CredentialError::Encryption("Seal failed".into()))?;

        // Prepend nonce to ciphertext+tag
        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);

        Ok(EncryptedFile {
            data: BASE64.encode(&combined),
            version: 1,
        })
    }

    /// Decrypt an EncryptedFile back to a credentials map
    fn decrypt_credentials(
        master_key: &[u8],
        enc_file: &EncryptedFile,
    ) -> Result<HashMap<String, String>, CredentialError> {
        let combined = BASE64.decode(&enc_file.data)
            .map_err(|e| CredentialError::Decryption(format!("Base64 decode failed: {}", e)))?;

        // AES-256-GCM tag is 16 bytes
        if combined.len() < NONCE_LEN + 16 {
            return Err(CredentialError::Decryption("Ciphertext too short".into()));
        }

        let (nonce_bytes, ciphertext_and_tag) = combined.split_at(NONCE_LEN);

        let cipher = Aes256Gcm::new_from_slice(master_key)
            .map_err(|_| CredentialError::Decryption("Invalid key".into()))?;
        let nonce = AesNonce::from_slice(nonce_bytes);

        let plaintext = cipher.decrypt(nonce, ciphertext_and_tag)
            .map_err(|_| CredentialError::Decryption(
                "Decryption failed — master key may have changed or data is corrupt".into(),
            ))?;

        serde_json::from_slice(&plaintext)
            .map_err(|e| CredentialError::Decryption(format!("JSON parse failed: {}", e)))
    }

    /// Save encrypted credentials to disk
    pub fn save(&self) -> Result<(), CredentialError> {
        let enc_file = Self::encrypt_credentials(&self.master_key, &self.credentials)?;
        let content = serde_json::to_string_pretty(&enc_file)
            .map_err(|e| CredentialError::Serialize(e.to_string()))?;

        let cred_path = self.config_dir.join(CREDENTIALS_FILENAME);
        std::fs::write(&cred_path, &content)?;

        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cred_path, std::fs::Permissions::from_mode(0o600))?;
        }

        // Remove legacy plaintext file if it exists
        let legacy_path = self.config_dir.join("credentials.json");
        if legacy_path.exists() {
            if let Err(e) = std::fs::remove_file(&legacy_path) {
                warn!("Failed to remove legacy credentials file: {}", e);
            } else {
                debug!("Removed legacy plaintext credentials file");
            }
        }

        debug!("Saved {} encrypted credentials", self.credentials.len());
        Ok(())
    }

    /// Store a password for a connection and persist to disk
    pub fn store(&mut self, connection_id: &str, password: &str) -> Result<(), CredentialError> {
        self.credentials.insert(connection_id.to_string(), password.to_string());
        self.save()
    }

    /// Get a password for a connection
    pub fn get(&self, connection_id: &str) -> Option<&str> {
        self.credentials.get(connection_id).map(|s| s.as_str())
    }

    /// Remove a password and persist the change
    pub fn remove(&mut self, connection_id: &str) -> Result<(), CredentialError> {
        self.credentials.remove(connection_id);
        self.save()
    }

    /// Check if a credential exists
    pub fn contains(&self, connection_id: &str) -> bool {
        self.credentials.contains_key(connection_id)
    }

    /// Number of stored credentials
    pub fn len(&self) -> usize {
        self.credentials.len()
    }

    /// Whether the store is empty
    pub fn is_empty(&self) -> bool {
        self.credentials.is_empty()
    }

    /// List all connection IDs that have stored credentials
    pub fn connection_ids(&self) -> Vec<&str> {
        self.credentials.keys().map(|s| s.as_str()).collect()
    }
}

/// Legacy plaintext format for migration
#[derive(Deserialize)]
struct LegacyCredentials {
    credentials: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stealthterm_cred_test_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_store_and_retrieve() {
        let dir = test_dir("store_retrieve");
        let mut store = CredentialStore::load_from(dir.clone()).unwrap();
        store.store("srv1", "hunter2").unwrap();
        store.store("srv2", "p@ssw0rd!").unwrap();

        assert_eq!(store.get("srv1"), Some("hunter2"));
        assert_eq!(store.get("srv2"), Some("p@ssw0rd!"));
        assert_eq!(store.get("srv3"), None);
        assert_eq!(store.len(), 2);

        cleanup(&dir);
    }

    #[test]
    fn test_save_and_reload() {
        let dir = test_dir("save_reload");

        {
            let mut store = CredentialStore::load_from(dir.clone()).unwrap();
            store.store("conn-a", "secret123").unwrap();
            store.store("conn-b", "p@$$word").unwrap();
        }

        // Reload from disk
        let store = CredentialStore::load_from(dir.clone()).unwrap();
        assert_eq!(store.get("conn-a"), Some("secret123"));
        assert_eq!(store.get("conn-b"), Some("p@$$word"));
        assert_eq!(store.len(), 2);

        cleanup(&dir);
    }

    #[test]
    fn test_encrypted_on_disk() {
        let dir = test_dir("encrypted_disk");
        let mut store = CredentialStore::load_from(dir.clone()).unwrap();
        store.store("myconn", "supersecret").unwrap();

        // Read the raw file — should NOT contain plaintext password
        let raw = std::fs::read_to_string(dir.join(CREDENTIALS_FILENAME)).unwrap();
        assert!(!raw.contains("supersecret"));
        assert!(!raw.contains("myconn"));
        // Should be valid JSON with base64 data
        let enc: EncryptedFile = serde_json::from_str(&raw).unwrap();
        assert_eq!(enc.version, 1);
        assert!(!enc.data.is_empty());

        cleanup(&dir);
    }

    #[test]
    fn test_remove_credential() {
        let dir = test_dir("remove");
        let mut store = CredentialStore::load_from(dir.clone()).unwrap();
        store.store("srv1", "pass1").unwrap();
        store.store("srv2", "pass2").unwrap();
        store.remove("srv1").unwrap();

        assert_eq!(store.get("srv1"), None);
        assert_eq!(store.get("srv2"), Some("pass2"));
        assert_eq!(store.len(), 1);

        // Verify persistence after remove
        let reloaded = CredentialStore::load_from(dir.clone()).unwrap();
        assert_eq!(reloaded.get("srv1"), None);
        assert_eq!(reloaded.get("srv2"), Some("pass2"));

        cleanup(&dir);
    }

    #[test]
    fn test_empty_store() {
        let dir = test_dir("empty");
        let store = CredentialStore::load_from(dir.clone()).unwrap();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        cleanup(&dir);
    }

    #[test]
    fn test_unicode_passwords() {
        let dir = test_dir("unicode");
        let mut store = CredentialStore::load_from(dir.clone()).unwrap();
        store.store("jp-server", "パスワード🔑").unwrap();

        let reloaded = CredentialStore::load_from(dir.clone()).unwrap();
        assert_eq!(reloaded.get("jp-server"), Some("パスワード🔑"));

        cleanup(&dir);
    }

    #[test]
    fn test_master_key_permissions() {
        let dir = test_dir("perms");
        let _store = CredentialStore::load_from(dir.clone()).unwrap();

        let key_path = dir.join(MASTER_KEY_FILENAME);
        assert!(key_path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&key_path).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }

        cleanup(&dir);
    }

    #[test]
    fn test_legacy_migration() {
        let dir = test_dir("migration");
        std::fs::create_dir_all(&dir).unwrap();

        // Write old-format plaintext file
        let legacy = serde_json::json!({
            "credentials": {
                "old-srv": "old-pass"
            }
        });
        std::fs::write(
            dir.join("credentials.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        ).unwrap();

        // Load should migrate
        let store = CredentialStore::load_from(dir.clone()).unwrap();
        assert_eq!(store.get("old-srv"), Some("old-pass"));

        // Save should create encrypted file and remove legacy
        store.save().unwrap();
        assert!(dir.join(CREDENTIALS_FILENAME).exists());
        assert!(!dir.join("credentials.json").exists());

        cleanup(&dir);
    }

    #[test]
    fn test_connection_ids() {
        let dir = test_dir("conn_ids");
        let mut store = CredentialStore::load_from(dir.clone()).unwrap();
        store.store("a", "1").unwrap();
        store.store("b", "2").unwrap();

        let mut ids = store.connection_ids();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"]);

        cleanup(&dir);
    }

    #[test]
    fn test_overwrite_password() {
        let dir = test_dir("overwrite");
        let mut store = CredentialStore::load_from(dir.clone()).unwrap();
        store.store("srv", "old").unwrap();
        store.store("srv", "new").unwrap();

        assert_eq!(store.get("srv"), Some("new"));
        assert_eq!(store.len(), 1);

        let reloaded = CredentialStore::load_from(dir.clone()).unwrap();
        assert_eq!(reloaded.get("srv"), Some("new"));

        cleanup(&dir);
    }

    #[test]
    fn test_wrong_key_fails_decryption() {
        let dir = test_dir("wrong_key");
        let mut store = CredentialStore::load_from(dir.clone()).unwrap();
        store.store("srv", "secret").unwrap();

        // Overwrite the master key with a different one
        let mut bad_key = vec![0u8; KEY_LEN];
        rand::rngs::OsRng.fill_bytes(&mut bad_key);
        std::fs::write(dir.join(MASTER_KEY_FILENAME), BASE64.encode(&bad_key)).unwrap();

        // Load should fail decryption
        let result = CredentialStore::load_from(dir.clone());
        assert!(result.is_err());

        cleanup(&dir);
    }
}
