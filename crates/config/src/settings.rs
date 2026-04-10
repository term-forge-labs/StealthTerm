use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use crate::master_password::MasterPassword;

fn default_language() -> String {
    "en".to_string()
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme: String,
    pub font_family: String,
    pub font_size: f32,
    pub scrollback_lines: usize,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub window_opacity: f32,
    pub sidebar_visible: bool,
    pub show_status_bar: bool,
    pub show_line_numbers: bool,
    pub tab_bar_position: TabBarPosition,
    pub bell_enabled: bool,
    /// PBKDF2 hash of the access password (base64-encoded); empty means not set
    #[serde(default)]
    pub access_password_hash: String,
    /// PBKDF2 salt (base64-encoded)
    #[serde(default)]
    pub access_password_salt: String,
    /// Idle auto-lock timeout in minutes; 0 means disabled
    #[serde(default)]
    pub auto_lock_minutes: u32,
    /// UI language: "en" or "zh"
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CursorStyle {
    Block,
    Underline,
    Bar,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TabBarPosition {
    Top,
    Bottom,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dracula".to_string(),
            font_family: "JetBrains Mono".to_string(),
            font_size: 16.0,
            scrollback_lines: 100_000,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            window_opacity: 1.0,
            sidebar_visible: true,
            show_status_bar: true,
            show_line_numbers: false,
            tab_bar_position: TabBarPosition::Top,
            bell_enabled: false,
            access_password_hash: String::new(),
            access_password_salt: String::new(),
            auto_lock_minutes: 0,
            language: "en".to_string(),
        }
    }
}

impl Settings {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stealthterm")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("settings.toml")
    }

    pub fn load() -> Result<Self, SettingsError> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }

    /// Whether an access password has been set
    pub fn has_access_password(&self) -> bool {
        !self.access_password_hash.is_empty() && !self.access_password_salt.is_empty()
    }

    /// Set the access password
    pub fn set_access_password(&mut self, password: &str) {
        if let Ok(mp) = MasterPassword::derive_from_password(password, None) {
            self.access_password_salt = BASE64.encode(mp.salt());
            self.access_password_hash = BASE64.encode(mp.key());
        }
    }

    /// Clear the access password
    pub fn clear_access_password(&mut self) {
        self.access_password_hash.clear();
        self.access_password_salt.clear();
        self.auto_lock_minutes = 0;
    }

    /// Verify the access password
    pub fn verify_access_password(&self, password: &str) -> bool {
        if !self.has_access_password() {
            return true;
        }
        let salt = match BASE64.decode(&self.access_password_salt) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let stored_hash = match BASE64.decode(&self.access_password_hash) {
            Ok(h) => h,
            Err(_) => return false,
        };
        if let Ok(mp) = MasterPassword::derive_from_password(password, Some(salt)) {
            mp.key() == stored_hash.as_slice()
        } else {
            false
        }
    }
}
