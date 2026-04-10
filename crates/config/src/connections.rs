use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum AuthMethod {
    Password,
    PublicKey { key_path: PathBuf },
}

// Custom deserialization: downgrade Agent to Password for old configs
impl<'de> serde::Deserialize<'de> for AuthMethod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum AuthMethodHelper {
            Password,
            PublicKey { key_path: PathBuf },
            Agent,
        }

        match AuthMethodHelper::deserialize(deserializer) {
            Ok(AuthMethodHelper::Password) => Ok(AuthMethod::Password),
            Ok(AuthMethodHelper::PublicKey { key_path }) => Ok(AuthMethod::PublicKey { key_path }),
            Ok(AuthMethodHelper::Agent) => Ok(AuthMethod::Password),
            Err(e) => Err(e),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub group: Option<String>,
    pub connection_type: ConnectionType,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub encoding: String,
    pub terminal_type: String,
    pub proxy_jump: Option<String>,
    pub keepalive_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionType {
    Ssh,
    Local,
    Serial { baud_rate: u32, device: String },
    Telnet,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "localhost".to_string(),
            group: None,
            connection_type: ConnectionType::Ssh,
            host: "127.0.0.1".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: AuthMethod::Password,
            encoding: "UTF-8".to_string(),
            terminal_type: "xterm-256color".to_string(),
            proxy_jump: None,
            keepalive_interval: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionStore {
    pub connections: Vec<ConnectionConfig>,
}

impl ConnectionStore {
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stealthterm")
            .join("connections.toml")
    }

    pub fn load() -> Result<Self, ConnectionError> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self) -> Result<(), ConnectionError> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn add(&mut self, conn: ConnectionConfig) {
        self.connections.push(conn);
    }

    pub fn update(&mut self, conn: ConnectionConfig) {
        if let Some(existing) = self.connections.iter_mut().find(|c| c.id == conn.id) {
            *existing = conn;
        } else {
            self.connections.push(conn);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.connections.retain(|c| c.id != id);
    }

    pub fn find_by_id(&self, id: &str) -> Option<&ConnectionConfig> {
        self.connections.iter().find(|c| c.id == id)
    }

    pub fn groups(&self) -> Vec<&str> {
        let mut groups: Vec<&str> = self.connections
            .iter()
            .filter_map(|c| c.group.as_deref())
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }
}
