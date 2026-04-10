use aes_gcm::{Aes256Gcm, KeyInit, Nonce as AesNonce};
use aes_gcm::aead::Aead;
use sha2::{Sha256, Digest};
use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use thiserror::Error;
use tracing::{debug, warn};

use crate::credentials::CredentialStore;

const NONCE_LEN: usize = 12;
const MAX_HISTORY: usize = 10_000;

/// Sensitive command keywords — commands containing these are not recorded
const SENSITIVE_PATTERNS: &[&str] = &[
    "password=", "passwd=", "token=", "api_key=", "apikey=",
    "secret=", "secret_key=", "access_key=",
];

#[derive(Debug, Error)]
pub enum EncryptedHistoryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Decryption error: {0}")]
    Decryption(String),
    #[error("Serialization error: {0}")]
    Serialize(String),
    #[error("Credential error: {0}")]
    Credential(#[from] crate::credentials::CredentialError),
}

#[derive(Serialize, Deserialize)]
struct EncryptedFile {
    data: String,
    version: u32,
}

/// Encrypted command history store, isolated per session.
///
/// Each session (identified by session_key) has its own encrypted file.
/// session_key format:
///   - SSH: `ssh://username@host:port`
///   - Local: `local://os_user@hostname`
pub struct EncryptedHistoryStore {
    entries: VecDeque<String>,
    master_key: Vec<u8>,
    file_path: PathBuf,
    dirty: bool,
}

impl EncryptedHistoryStore {
    /// Load (or create) the encrypted history for the given session_key
    pub fn load(session_key: &str) -> Result<Self, EncryptedHistoryError> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stealthterm");
        let history_dir = config_dir.join("history");
        std::fs::create_dir_all(&history_dir)?;

        let master_key = CredentialStore::load_or_create_master_key(&config_dir)?;
        let file_name = Self::session_key_to_filename(session_key);
        let file_path = history_dir.join(file_name);

        let entries = if file_path.exists() {
            match Self::decrypt_entries(&master_key, &file_path) {
                Ok(e) => {
                    debug!("Loaded {} encrypted history entries for session", e.len());
                    e
                }
                Err(e) => {
                    warn!("Failed to decrypt history ({}), starting fresh", e);
                    VecDeque::new()
                }
            }
        } else {
            debug!("No history file for session, starting fresh");
            VecDeque::new()
        };

        Ok(Self {
            entries,
            master_key,
            file_path,
            dirty: false,
        })
    }

    /// Add a command and save immediately
    pub fn push_and_save(&mut self, cmd: &str) -> Result<(), EncryptedHistoryError> {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return Ok(());
        }
        // Filter sensitive commands
        let lower = cmd.to_lowercase();
        if SENSITIVE_PATTERNS.iter().any(|p| lower.contains(p)) {
            debug!("Skipping sensitive command");
            return Ok(());
        }
        // Deduplicate: remove existing identical command
        self.entries.retain(|e| e != cmd);
        self.entries.push_front(cmd.to_string());
        // Enforce size limit
        while self.entries.len() > MAX_HISTORY {
            self.entries.pop_back();
        }
        self.dirty = true;
        self.save()
    }

    /// Get all history entries
    pub fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    /// Clear all history entries and save
    pub fn clear_all(&mut self) -> Result<(), EncryptedHistoryError> {
        self.entries.clear();
        self.dirty = true;
        self.save()
    }

    /// Clear history files for all sessions
    pub fn clear_all_sessions() -> Result<(), EncryptedHistoryError> {
        let history_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stealthterm")
            .join("history");
        if history_dir.exists() {
            for entry in std::fs::read_dir(&history_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("enc") {
                    std::fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }

    fn save(&mut self) -> Result<(), EncryptedHistoryError> {
        if !self.dirty {
            return Ok(());
        }
        let plaintext = serde_json::to_string(&self.entries)
            .map_err(|e| EncryptedHistoryError::Serialize(e.to_string()))?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|_| EncryptedHistoryError::Encryption("Invalid key".into()))?;
        let nonce = AesNonce::from_slice(&nonce_bytes);

        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| EncryptedHistoryError::Encryption("Seal failed".into()))?;

        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);

        let enc_file = EncryptedFile {
            data: BASE64.encode(&combined),
            version: 1,
        };
        let json = serde_json::to_string(&enc_file)
            .map_err(|e| EncryptedHistoryError::Serialize(e.to_string()))?;
        std::fs::write(&self.file_path, json)?;
        self.dirty = false;
        Ok(())
    }

    fn decrypt_entries(
        master_key: &[u8],
        file_path: &PathBuf,
    ) -> Result<VecDeque<String>, EncryptedHistoryError> {
        let content = std::fs::read_to_string(file_path)?;
        let enc_file: EncryptedFile = serde_json::from_str(&content)
            .map_err(|e| EncryptedHistoryError::Decryption(e.to_string()))?;

        let combined = BASE64.decode(&enc_file.data)
            .map_err(|e| EncryptedHistoryError::Decryption(e.to_string()))?;

        if combined.len() < NONCE_LEN {
            return Err(EncryptedHistoryError::Decryption("Data too short".into()));
        }

        let (nonce_bytes, ciphertext_and_tag) = combined.split_at(NONCE_LEN);

        let cipher = Aes256Gcm::new_from_slice(master_key)
            .map_err(|_| EncryptedHistoryError::Decryption("Invalid key".into()))?;
        let nonce = AesNonce::from_slice(nonce_bytes);

        let plaintext = cipher.decrypt(nonce, ciphertext_and_tag)
            .map_err(|_| EncryptedHistoryError::Decryption("Decryption failed".into()))?;

        let entries: VecDeque<String> = serde_json::from_slice(&plaintext)
            .map_err(|e| EncryptedHistoryError::Decryption(e.to_string()))?;
        Ok(entries)
    }

    /// session_key → filename (first 16 hex chars of SHA-256 + .enc)
    fn session_key_to_filename(session_key: &str) -> String {
        let hash = Sha256::digest(session_key.as_bytes());
        let hex: String = hash.iter().take(8).map(|b| format!("{:02x}", b)).collect();
        format!("{}.enc", hex)
    }
}

impl Drop for EncryptedHistoryStore {
    fn drop(&mut self) {
        if self.dirty {
            let _ = self.save();
        }
    }
}
