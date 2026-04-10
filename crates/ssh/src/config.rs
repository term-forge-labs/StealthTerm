use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    pub terminal_type: String,
    pub keepalive_secs: u64,
    pub proxy_jump: Option<ProxyJump>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshAuth {
    Password(String),
    PublicKey { key_path: PathBuf, passphrase: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyJump {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SshAuth::Password(String::new()),
            terminal_type: "xterm-256color".to_string(),
            keepalive_secs: 60,
            proxy_jump: None,
        }
    }
}
