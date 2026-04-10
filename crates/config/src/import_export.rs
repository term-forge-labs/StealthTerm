//! Import/export connection configurations from other SSH clients.
//!
//! Supported formats:
//! - **OpenSSH** (`~/.ssh/config`)
//! - **PuTTY** (`.reg` registry export)
//! - **SecureCRT** (`.ini` session files)
//! - **Termius** (JSON export)
//! - **Xshell** (`.xsh` session files, UTF-16 LE or UTF-8)
//!
//! All importers produce `Vec<ConnectionConfig>` that can be merged into a `ConnectionStore`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::connections::{AuthMethod, ConnectionConfig, ConnectionType};

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported format: {0}")]
    Unsupported(String),
}

/// Result of an import operation
#[derive(Debug)]
pub struct ImportResult {
    /// Successfully imported connections
    pub connections: Vec<ConnectionConfig>,
    /// Warnings (e.g., skipped entries, unsupported fields)
    pub warnings: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// OpenSSH config
// ─────────────────────────────────────────────────────────────────────────────

/// Import connections from an OpenSSH config file (`~/.ssh/config`).
pub fn import_openssh(path: &Path) -> Result<ImportResult, ImportError> {
    let content = std::fs::read_to_string(path)?;
    import_openssh_str(&content)
}

/// Import from an OpenSSH config string.
pub fn import_openssh_str(content: &str) -> Result<ImportResult, ImportError> {
    let mut connections = Vec::new();
    let mut warnings = Vec::new();

    // Collect Host blocks. Each block starts with a `Host` line.
    let mut current: Option<SshBlock> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = parse_ssh_kv(line);
        let key_lower = key.to_ascii_lowercase();

        if key_lower == "host" {
            // Flush previous block
            if let Some(block) = current.take() {
                match block.to_connection() {
                    Ok(conn) => connections.push(conn),
                    Err(w) => warnings.push(w),
                }
            }
            current = Some(SshBlock::new(value.to_string()));
        } else if key_lower == "match" {
            // Flush previous, skip Match blocks
            if let Some(block) = current.take() {
                match block.to_connection() {
                    Ok(conn) => connections.push(conn),
                    Err(w) => warnings.push(w),
                }
            }
            warnings.push(format!("Skipped Match block: {}", value));
            current = None;
        } else if let Some(ref mut block) = current {
            block.set(&key_lower, value);
        }
    }
    // Flush last block
    if let Some(block) = current.take() {
        match block.to_connection() {
            Ok(conn) => connections.push(conn),
            Err(w) => warnings.push(w),
        }
    }

    debug!("Imported {} connections from OpenSSH config", connections.len());
    Ok(ImportResult { connections, warnings })
}

/// Parse a single `Key Value` or `Key=Value` line.
fn parse_ssh_kv(line: &str) -> (&str, &str) {
    // Try `Key=Value` first
    if let Some((k, v)) = line.split_once('=') {
        (k.trim(), v.trim())
    } else if let Some((k, v)) = line.split_once(char::is_whitespace) {
        (k.trim(), v.trim())
    } else {
        (line, "")
    }
}

struct SshBlock {
    host_pattern: String,
    hostname: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
    server_alive_interval: Option<u64>,
    forward_agent: bool,
}

impl SshBlock {
    fn new(host_pattern: String) -> Self {
        Self {
            host_pattern,
            hostname: None,
            port: None,
            user: None,
            identity_file: None,
            proxy_jump: None,
            server_alive_interval: None,
            forward_agent: false,
        }
    }

    fn set(&mut self, key: &str, value: &str) {
        match key {
            "hostname" => self.hostname = Some(value.to_string()),
            "port" => self.port = value.parse().ok(),
            "user" => self.user = Some(value.to_string()),
            "identityfile" => self.identity_file = Some(value.to_string()),
            "proxyjump" => self.proxy_jump = Some(value.to_string()),
            "serveraliveinterval" => self.server_alive_interval = value.parse().ok(),
            "forwardagent" => self.forward_agent = value.eq_ignore_ascii_case("yes"),
            _ => {} // Ignore unknown keys
        }
    }

    fn to_connection(self) -> Result<ConnectionConfig, String> {
        // Skip wildcard patterns and empty hosts
        if self.host_pattern.contains('*') || self.host_pattern.contains('?') {
            return Err(format!("Skipped wildcard Host pattern: {}", self.host_pattern));
        }

        let host = self.hostname.unwrap_or_else(|| self.host_pattern.clone());
        if host.is_empty() {
            return Err(format!("Skipped Host with no hostname: {}", self.host_pattern));
        }

        let auth = if let Some(ref key_path) = self.identity_file {
            // Expand ~ to home dir
            let expanded = if key_path.starts_with("~/") {
                dirs::home_dir()
                    .map(|h| h.join(&key_path[2..]))
                    .unwrap_or_else(|| PathBuf::from(key_path))
            } else {
                PathBuf::from(key_path)
            };
            AuthMethod::PublicKey { key_path: expanded }
        } else {
            AuthMethod::Password
        };

        Ok(ConnectionConfig {
            id: Uuid::new_v4().to_string(),
            name: self.host_pattern,
            group: Some("OpenSSH Import".to_string()),
            connection_type: ConnectionType::Ssh,
            host,
            port: self.port.unwrap_or(22),
            username: self.user.unwrap_or_else(|| String::new()),
            auth,
            encoding: "UTF-8".to_string(),
            terminal_type: "xterm-256color".to_string(),
            proxy_jump: self.proxy_jump,
            keepalive_interval: self.server_alive_interval.unwrap_or(60),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PuTTY .reg export
// ─────────────────────────────────────────────────────────────────────────────

/// Import connections from a PuTTY `.reg` export file.
pub fn import_putty(path: &Path) -> Result<ImportResult, ImportError> {
    let raw = std::fs::read(path)?;
    // PuTTY .reg files may be UTF-16 LE with BOM
    let content = decode_with_bom(&raw);
    import_putty_str(&content)
}

/// Import from PuTTY `.reg` content string.
pub fn import_putty_str(content: &str) -> Result<ImportResult, ImportError> {
    let mut connections = Vec::new();
    let mut warnings = Vec::new();

    let session_re = regex::Regex::new(
        r"(?i)\[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\([^\]]+)\]"
    ).map_err(|e| ImportError::Parse(e.to_string()))?;

    // Split into session blocks
    let mut blocks: Vec<(String, HashMap<String, String>)> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if let Some(caps) = session_re.captures(line) {
            let session_name = url_decode(&caps[1]);
            blocks.push((session_name, HashMap::new()));
        } else if let Some((_, kv)) = blocks.last_mut() {
            if let Some((key, value)) = parse_reg_value(line) {
                kv.insert(key, value);
            }
        }
    }

    for (name, kv) in blocks {
        // Skip the Default Settings template
        if name == "Default Settings" {
            continue;
        }

        let hostname = kv.get("HostName").map(|s| s.as_str()).unwrap_or("");
        if hostname.is_empty() {
            warnings.push(format!("Skipped PuTTY session '{}': no hostname", name));
            continue;
        }

        let protocol = kv.get("Protocol").map(|s| s.as_str()).unwrap_or("ssh");
        let conn_type = match protocol {
            "ssh" => ConnectionType::Ssh,
            "telnet" => ConnectionType::Telnet,
            "serial" => {
                let baud = kv.get("SerialSpeed")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(9600);
                let device = kv.get("SerialLine")
                    .cloned()
                    .unwrap_or_else(|| "COM1".to_string());
                ConnectionType::Serial { baud_rate: baud, device }
            }
            other => {
                warnings.push(format!("Skipped PuTTY session '{}': unsupported protocol '{}'", name, other));
                continue;
            }
        };

        let port = kv.get("PortNumber")
            .and_then(|s| parse_putty_dword(s))
            .unwrap_or(22) as u16;

        let username = kv.get("UserName").cloned().unwrap_or_default();
        let key_file = kv.get("PublicKeyFile").cloned().unwrap_or_default();
        let _agent_fwd = kv.get("AgentFwd")
            .and_then(|s| parse_putty_dword(s))
            .unwrap_or(0);

        let auth = if !key_file.is_empty() {
            AuthMethod::PublicKey { key_path: PathBuf::from(&key_file) }
        } else {
            AuthMethod::Password
        };

        let keepalive = kv.get("PingIntervalSecs")
            .and_then(|s| parse_putty_dword(s))
            .unwrap_or(60) as u64;

        let terminal_type = kv.get("TerminalType")
            .cloned()
            .unwrap_or_else(|| "xterm-256color".to_string());

        connections.push(ConnectionConfig {
            id: Uuid::new_v4().to_string(),
            name,
            group: Some("PuTTY Import".to_string()),
            connection_type: conn_type,
            host: hostname.to_string(),
            port,
            username,
            auth,
            encoding: "UTF-8".to_string(),
            terminal_type,
            proxy_jump: None,
            keepalive_interval: keepalive,
        });
    }

    debug!("Imported {} connections from PuTTY", connections.len());
    Ok(ImportResult { connections, warnings })
}

/// Parse a .reg value line like `"Key"="value"` or `"Key"=dword:00000016`
fn parse_reg_value(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if !line.starts_with('"') {
        return None;
    }
    let (key_part, value_part) = line.split_once('=')?;
    let key = key_part.trim().trim_matches('"').to_string();
    let value = value_part.trim();

    let parsed = if value.starts_with("dword:") {
        // Keep as-is for dword — callers use parse_putty_dword
        value.to_string()
    } else {
        // String value — strip quotes
        value.trim_matches('"').to_string()
    };

    Some((key, parsed))
}

/// Parse a PuTTY dword value (hex or decimal).
fn parse_putty_dword(s: &str) -> Option<u32> {
    if let Some(hex) = s.strip_prefix("dword:") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse().ok()
    }
}

/// URL-decode a PuTTY session name (`%20` → space, etc.)
fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }
    result
}

// ─────────────────────────────────────────────────────────────────────────────
// SecureCRT .ini session files
// ─────────────────────────────────────────────────────────────────────────────

/// Import connections from a SecureCRT session `.ini` file.
pub fn import_securecrt(path: &Path) -> Result<ImportResult, ImportError> {
    let content = std::fs::read_to_string(path)?;
    import_securecrt_str(&content, path.file_stem().and_then(|s| s.to_str()).unwrap_or("Unnamed"))
}

/// Import from SecureCRT `.ini` content string.
pub fn import_securecrt_str(content: &str, name_hint: &str) -> Result<ImportResult, ImportError> {
    let mut connections = Vec::new();
    let mut warnings = Vec::new();

    let kv = parse_securecrt_kv(content);

    // Check if this is a valid session file
    let is_session = kv.get("Is Session")
        .and_then(|v| parse_securecrt_dword(v))
        .unwrap_or(0);
    if is_session != 1 {
        warnings.push(format!("'{}' is not a session file (Is Session != 1)", name_hint));
        return Ok(ImportResult { connections, warnings });
    }

    let protocol = kv.get("Protocol Name").map(|s| s.as_str()).unwrap_or("SSH2");
    let conn_type = match protocol {
        "SSH2" | "SSH1" => ConnectionType::Ssh,
        "Telnet" => ConnectionType::Telnet,
        "Serial" => {
            let baud = kv.get("Baud Rate")
                .and_then(|s| parse_securecrt_dword(s))
                .unwrap_or(9600);
            let device = kv.get("Port").cloned().unwrap_or_else(|| "COM1".to_string());
            ConnectionType::Serial { baud_rate: baud, device }
        }
        other => {
            warnings.push(format!("Unsupported SecureCRT protocol: {}", other));
            return Ok(ImportResult { connections, warnings });
        }
    };

    let hostname = kv.get("Hostname").cloned().unwrap_or_default();
    if hostname.is_empty() && !matches!(conn_type, ConnectionType::Serial { .. }) {
        warnings.push(format!("Skipped '{}': no hostname", name_hint));
        return Ok(ImportResult { connections, warnings });
    }

    let port_key = if protocol == "SSH1" { "[SSH1] Port" } else { "[SSH2] Port" };
    let port = kv.get(port_key)
        .and_then(|v| parse_securecrt_dword(v))
        .unwrap_or(22) as u16;

    let username = kv.get("Username").cloned().unwrap_or_default();
    let identity_file = kv.get("Identity Filename V2").cloned().unwrap_or_default();

    let auth = if !identity_file.is_empty() {
        AuthMethod::PublicKey { key_path: PathBuf::from(&identity_file) }
    } else {
        AuthMethod::Password
    };

    let emulation = kv.get("Emulation").cloned().unwrap_or_else(|| "xterm-256color".to_string());

    connections.push(ConnectionConfig {
        id: Uuid::new_v4().to_string(),
        name: name_hint.to_string(),
        group: Some("SecureCRT Import".to_string()),
        connection_type: conn_type,
        host: hostname,
        port,
        username,
        auth,
        encoding: "UTF-8".to_string(),
        terminal_type: emulation,
        proxy_jump: None,
        keepalive_interval: 60,
    });

    debug!("Imported SecureCRT session '{}'", name_hint);
    Ok(ImportResult { connections, warnings })
}

/// Parse SecureCRT typed key-value pairs: `S:"Key"=Value` / `D:"Key"=HexValue`
fn parse_securecrt_kv(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        // Match S:"Key"=Value or D:"Key"=Value
        let (type_prefix, rest) = if line.starts_with("S:") {
            ('S', &line[2..])
        } else if line.starts_with("D:") {
            ('D', &line[2..])
        } else if line.starts_with("Z:") {
            ('Z', &line[2..])
        } else {
            continue;
        };

        // Parse "Key"=Value
        if !rest.starts_with('"') {
            continue;
        }
        let after_open = &rest[1..];
        if let Some(close_pos) = after_open.find('"') {
            let key = &after_open[..close_pos];
            let after_key = &after_open[close_pos + 1..];
            if let Some(value) = after_key.strip_prefix('=') {
                let value = value.trim();
                match type_prefix {
                    'D' => {
                        // Store as "dword:HEXVALUE" for consistency
                        map.insert(key.to_string(), format!("dword:{}", value));
                    }
                    _ => {
                        map.insert(key.to_string(), value.to_string());
                    }
                }
            }
        }
    }
    map
}

/// Parse a SecureCRT dword value (8-digit hex, optionally prefixed with `dword:`)
fn parse_securecrt_dword(s: &str) -> Option<u32> {
    if let Some(hex) = s.strip_prefix("dword:") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        u32::from_str_radix(s, 16).ok()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Termius JSON export
// ─────────────────────────────────────────────────────────────────────────────

/// Import connections from a Termius JSON export file.
pub fn import_termius(path: &Path) -> Result<ImportResult, ImportError> {
    let content = std::fs::read_to_string(path)?;
    import_termius_str(&content)
}

/// Import from Termius JSON string.
pub fn import_termius_str(content: &str) -> Result<ImportResult, ImportError> {
    let data: serde_json::Value = serde_json::from_str(content)?;
    let mut connections = Vec::new();
    let mut warnings = Vec::new();

    // Build lookup tables
    let groups = build_termius_lookup(&data, "group_set", "label");
    let ssh_configs = build_termius_map(&data, "sshconfig_set");
    let identities = build_termius_map(&data, "identity_set");
    let ssh_keys = build_termius_map(&data, "sshkeycrypt_set");

    let hosts = data.get("host_set").and_then(|v| v.as_array());
    let hosts = match hosts {
        Some(h) => h,
        None => {
            warnings.push("No host_set found in Termius export".to_string());
            return Ok(ImportResult { connections, warnings });
        }
    };

    for host in hosts {
        let label = host.get("label").and_then(|v| v.as_str()).unwrap_or("Unnamed");
        let address = host.get("address").and_then(|v| v.as_str()).unwrap_or("");
        if address.is_empty() {
            warnings.push(format!("Skipped Termius host '{}': no address", label));
            continue;
        }

        // Resolve group name
        let group_id = host.get("group").and_then(|v| v.as_i64());
        let group_name = group_id.and_then(|id| groups.get(&id).cloned());

        // Resolve SSH config → identity → ssh_key
        let ssh_config_id = host.get("ssh_config").and_then(|v| v.as_i64());
        let ssh_config = ssh_config_id.and_then(|id| ssh_configs.get(&id));

        let port = ssh_config
            .and_then(|c| c.get("port"))
            .and_then(|v| v.as_u64())
            .unwrap_or(22) as u16;

        let identity_id = ssh_config
            .and_then(|c| c.get("identity"))
            .and_then(|v| v.as_i64());
        let identity = identity_id.and_then(|id| identities.get(&id));

        let username = identity
            .and_then(|i| i.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let use_ssh_key = ssh_config
            .and_then(|c| c.get("use_ssh_key"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let auth = if use_ssh_key {
            let ssh_key_id = identity
                .and_then(|i| i.get("ssh_key"))
                .and_then(|v| v.as_i64());
            let has_key = ssh_key_id
                .and_then(|id| ssh_keys.get(&id))
                .and_then(|k| k.get("private_key"))
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if has_key {
                // Termius stores inline keys — we can't import the key data as a file path,
                // so fall back to Password (user will need to configure key separately)
                warnings.push(format!(
                    "Host '{}': SSH key is inline in Termius export; set to Password auth (configure key path manually)",
                    label
                ));
                AuthMethod::Password
            } else {
                AuthMethod::Password
            }
        } else {
            AuthMethod::Password
        };

        let keepalive = ssh_config
            .and_then(|c| c.get("keep_alive_packages"))
            .and_then(|v| v.as_u64())
            .map(|k| if k > 0 { 60 } else { 0 })
            .unwrap_or(60);

        connections.push(ConnectionConfig {
            id: Uuid::new_v4().to_string(),
            name: label.to_string(),
            group: Some(group_name.unwrap_or_else(|| "Termius Import".to_string())),
            connection_type: ConnectionType::Ssh,
            host: address.to_string(),
            port,
            username,
            auth,
            encoding: "UTF-8".to_string(),
            terminal_type: "xterm-256color".to_string(),
            proxy_jump: None,
            keepalive_interval: keepalive,
        });
    }

    debug!("Imported {} connections from Termius", connections.len());
    Ok(ImportResult { connections, warnings })
}

/// Build a simple id → label lookup from a Termius set array
fn build_termius_lookup(data: &serde_json::Value, set_key: &str, field: &str) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    if let Some(arr) = data.get(set_key).and_then(|v| v.as_array()) {
        for item in arr {
            if let (Some(id), Some(val)) = (
                item.get("id").and_then(|v| v.as_i64()),
                item.get(field).and_then(|v| v.as_str()),
            ) {
                map.insert(id, val.to_string());
            }
        }
    }
    map
}

/// Build a full id → Value lookup from a Termius set array
fn build_termius_map(data: &serde_json::Value, set_key: &str) -> HashMap<i64, serde_json::Value> {
    let mut map = HashMap::new();
    if let Some(arr) = data.get(set_key).and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(id) = item.get("id").and_then(|v| v.as_i64()) {
                map.insert(id, item.clone());
            }
        }
    }
    map
}

// ─────────────────────────────────────────────────────────────────────────────
// Xshell .xsh session files
// ─────────────────────────────────────────────────────────────────────────────

/// Import connections from an Xshell `.xsh` session file.
pub fn import_xshell(path: &Path) -> Result<ImportResult, ImportError> {
    let raw = std::fs::read(path)?;
    let content = decode_with_bom(&raw);
    let name_hint = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Unnamed");
    import_xshell_str(&content, name_hint)
}

/// Import from Xshell `.xsh` content string.
pub fn import_xshell_str(content: &str, name_hint: &str) -> Result<ImportResult, ImportError> {
    let mut connections = Vec::new();
    let mut warnings = Vec::new();

    // Parse INI sections into section_name → key→value map
    let sections = parse_ini_sections(content);

    let conn = sections.get("CONNECTION");
    let auth = sections.get("CONNECTION:AUTHENTICATION");
    let terminal = sections.get("TERMINAL");

    let host = conn.and_then(|m| m.get("Host")).map(|s| s.as_str()).unwrap_or("");
    if host.is_empty() {
        warnings.push(format!("Skipped '{}': no host", name_hint));
        return Ok(ImportResult { connections, warnings });
    }

    let protocol = conn.and_then(|m| m.get("Protocol")).map(|s| s.as_str()).unwrap_or("SSH");
    let conn_type = match protocol.to_ascii_uppercase().as_str() {
        "SSH" => ConnectionType::Ssh,
        "TELNET" => ConnectionType::Telnet,
        "SERIAL" => {
            let baud = conn.and_then(|m| m.get("BaudRate"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(9600);
            ConnectionType::Serial {
                baud_rate: baud,
                device: host.to_string(),
            }
        }
        other => {
            warnings.push(format!("Unsupported Xshell protocol: {}", other));
            return Ok(ImportResult { connections, warnings });
        }
    };

    let port = conn.and_then(|m| m.get("Port"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(22u16);

    let username = auth.and_then(|m| m.get("UserName")).cloned().unwrap_or_default();
    let method = auth.and_then(|m| m.get("Method")).map(|s| s.as_str()).unwrap_or("PASSWORD");

    let auth_method = match method.to_ascii_uppercase().as_str() {
        "PUBLICKEY" => {
            let key_path = auth.and_then(|m| m.get("UserKeyFile"))
                .or_else(|| auth.and_then(|m| m.get("IdentityFile")))
                .cloned()
                .unwrap_or_default();
            if key_path.is_empty() {
                warnings.push(format!("Host '{}': PUBLICKEY auth but no key file found, falling back to Password", name_hint));
                AuthMethod::Password
            } else {
                AuthMethod::PublicKey { key_path: PathBuf::from(key_path) }
            }
        }
        _ => AuthMethod::Password,
    };

    let terminal_type = terminal.and_then(|m| m.get("Type"))
        .cloned()
        .unwrap_or_else(|| "xterm-256color".to_string());

    let encoding = terminal.and_then(|m| m.get("Encoding"))
        .cloned()
        .unwrap_or_else(|| "UTF-8".to_string());

    connections.push(ConnectionConfig {
        id: Uuid::new_v4().to_string(),
        name: name_hint.to_string(),
        group: Some("Xshell Import".to_string()),
        connection_type: conn_type,
        host: host.to_string(),
        port,
        username,
        auth: auth_method,
        encoding,
        terminal_type,
        proxy_jump: None,
        keepalive_interval: 60,
    });

    debug!("Imported Xshell session '{}'", name_hint);
    Ok(ImportResult { connections, warnings })
}

/// Parse INI sections: `[SECTION]` → key=value pairs
fn parse_ini_sections(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            sections.entry(current_section.clone()).or_default();
        } else if let Some((key, value)) = line.split_once('=') {
            if let Some(section) = sections.get_mut(&current_section) {
                section.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    sections
}

// ─────────────────────────────────────────────────────────────────────────────
// Export (StealthTerm → OpenSSH config)
// ─────────────────────────────────────────────────────────────────────────────

/// Export connections as an OpenSSH config string.
pub fn export_openssh(connections: &[ConnectionConfig]) -> String {
    let mut out = String::from("# Exported from StealthTerm\n\n");
    for conn in connections {
        if !matches!(conn.connection_type, ConnectionType::Ssh) {
            continue;
        }
        out.push_str(&format!("Host {}\n", conn.name.replace(' ', "-")));
        out.push_str(&format!("    HostName {}\n", conn.host));
        if conn.port != 22 {
            out.push_str(&format!("    Port {}\n", conn.port));
        }
        if !conn.username.is_empty() {
            out.push_str(&format!("    User {}\n", conn.username));
        }
        match &conn.auth {
            AuthMethod::PublicKey { key_path } => {
                out.push_str(&format!("    IdentityFile {}\n", key_path.display()));
                out.push_str("    IdentitiesOnly yes\n");
            }
            AuthMethod::Password => {}
        }
        if let Some(ref proxy) = conn.proxy_jump {
            out.push_str(&format!("    ProxyJump {}\n", proxy));
        }
        if conn.keepalive_interval > 0 {
            out.push_str(&format!("    ServerAliveInterval {}\n", conn.keepalive_interval));
        }
        out.push('\n');
    }
    out
}

/// Export connections as StealthTerm's own TOML format string.
pub fn export_toml(connections: &[ConnectionConfig]) -> Result<String, ImportError> {
    let store = crate::connections::ConnectionStore {
        connections: connections.to_vec(),
    };
    toml::to_string_pretty(&store)
        .map_err(|e| ImportError::Parse(e.to_string()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Detect BOM and decode bytes to UTF-8 string.
/// Handles UTF-16 LE (PuTTY .reg, Xshell .xsh) and UTF-8.
fn decode_with_bom(raw: &[u8]) -> String {
    // UTF-16 LE BOM: 0xFF 0xFE
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        let (cow, _, had_errors) = encoding_rs::UTF_16LE.decode(&raw[2..]);
        if had_errors {
            warn!("UTF-16 LE decoding had errors, some characters may be lost");
        }
        return cow.into_owned();
    }
    // UTF-16 BE BOM: 0xFE 0xFF
    if raw.len() >= 2 && raw[0] == 0xFE && raw[1] == 0xFF {
        let (cow, _, had_errors) = encoding_rs::UTF_16BE.decode(&raw[2..]);
        if had_errors {
            warn!("UTF-16 BE decoding had errors");
        }
        return cow.into_owned();
    }
    // UTF-8 BOM: 0xEF 0xBB 0xBF
    if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        return String::from_utf8_lossy(&raw[3..]).into_owned();
    }
    // Default: UTF-8
    String::from_utf8_lossy(raw).into_owned()
}

// ─────────────────────────────────────────────────────────────────────────────
// Auto-detect importer
// ─────────────────────────────────────────────────────────────────────────────

/// Supported import formats
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportFormat {
    OpenSsh,
    PuTTY,
    SecureCRT,
    Termius,
    Xshell,
}

/// Auto-detect format and import from a file.
pub fn import_auto(path: &Path) -> Result<ImportResult, ImportError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "reg" => import_putty(path),
        "xsh" => import_xshell(path),
        "json" => import_termius(path),
        "ini" => import_securecrt(path),
        _ => {
            // Try to detect OpenSSH config by content
            let content = std::fs::read_to_string(path)
                .map_err(|_| ImportError::Unsupported(format!(
                    "Cannot read file: {}", path.display()
                )))?;
            if content.lines().any(|l| {
                let t = l.trim().to_ascii_lowercase();
                t.starts_with("host ") || t.starts_with("host\t")
            }) {
                import_openssh_str(&content)
            } else {
                Err(ImportError::Unsupported(format!(
                    "Cannot detect format for: {}", path.display()
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── OpenSSH ──

    #[test]
    fn test_openssh_basic() {
        let config = r#"
Host prod-web
    HostName 10.0.1.50
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_ed25519
    ServerAliveInterval 30

Host staging
    HostName staging.example.com
    User admin
"#;
        let result = import_openssh_str(config).unwrap();
        assert_eq!(result.connections.len(), 2);

        let c1 = &result.connections[0];
        assert_eq!(c1.name, "prod-web");
        assert_eq!(c1.host, "10.0.1.50");
        assert_eq!(c1.port, 2222);
        assert_eq!(c1.username, "deploy");
        assert_eq!(c1.keepalive_interval, 30);
        assert!(matches!(c1.auth, AuthMethod::PublicKey { .. }));

        let c2 = &result.connections[1];
        assert_eq!(c2.name, "staging");
        assert_eq!(c2.host, "staging.example.com");
        assert_eq!(c2.port, 22); // default
        assert_eq!(c2.username, "admin");
    }

    #[test]
    fn test_openssh_wildcard_skipped() {
        let config = r#"
Host *
    ServerAliveInterval 60

Host prod
    HostName prod.example.com
"#;
        let result = import_openssh_str(config).unwrap();
        assert_eq!(result.connections.len(), 1);
        assert_eq!(result.connections[0].name, "prod");
        assert!(result.warnings.iter().any(|w| w.contains("wildcard")));
    }

    #[test]
    fn test_openssh_agent_forwarding() {
        let config = "Host jump\n    HostName bastion.example.com\n    ForwardAgent yes\n";
        let result = import_openssh_str(config).unwrap();
        // Agent removed — ForwardAgent now falls through to Password
        assert_eq!(result.connections[0].auth, AuthMethod::Password);
    }

    #[test]
    fn test_openssh_equals_delimiter() {
        let config = "Host myhost\n    HostName=10.0.0.1\n    Port=8022\n    User=admin\n";
        let result = import_openssh_str(config).unwrap();
        let c = &result.connections[0];
        assert_eq!(c.host, "10.0.0.1");
        assert_eq!(c.port, 8022);
        assert_eq!(c.username, "admin");
    }

    #[test]
    fn test_openssh_proxy_jump() {
        let config = "Host internal\n    HostName 192.168.1.1\n    ProxyJump bastion\n";
        let result = import_openssh_str(config).unwrap();
        assert_eq!(result.connections[0].proxy_jump.as_deref(), Some("bastion"));
    }

    // ── PuTTY ──

    #[test]
    fn test_putty_basic() {
        let reg = r#"Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\my%20server]
"HostName"="10.0.1.50"
"PortNumber"=dword:00000016
"Protocol"="ssh"
"UserName"="deploy"
"PublicKeyFile"=""
"AgentFwd"=dword:00000000
"PingIntervalSecs"=dword:0000003c
"TerminalType"="xterm-256color"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Default%20Settings]
"HostName"=""
"PortNumber"=dword:00000016
"#;
        let result = import_putty_str(reg).unwrap();
        assert_eq!(result.connections.len(), 1); // Default Settings skipped
        let c = &result.connections[0];
        assert_eq!(c.name, "my server");
        assert_eq!(c.host, "10.0.1.50");
        assert_eq!(c.port, 22);
        assert_eq!(c.username, "deploy");
        assert_eq!(c.keepalive_interval, 60);
    }

    #[test]
    fn test_putty_with_key() {
        let reg = r#"
[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\keyhost]
"HostName"="example.com"
"PortNumber"=dword:00000016
"Protocol"="ssh"
"UserName"="root"
"PublicKeyFile"="C:\\Users\\admin\\.ssh\\id_rsa.ppk"
"AgentFwd"=dword:00000000
"#;
        let result = import_putty_str(reg).unwrap();
        let c = &result.connections[0];
        assert!(matches!(&c.auth, AuthMethod::PublicKey { key_path } if key_path.to_str().unwrap().contains("id_rsa.ppk")));
    }

    #[test]
    fn test_putty_telnet() {
        let reg = r#"
[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\telnet%20box]
"HostName"="192.168.1.1"
"PortNumber"=dword:00000017
"Protocol"="telnet"
"UserName"=""
"#;
        let result = import_putty_str(reg).unwrap();
        let c = &result.connections[0];
        assert_eq!(c.connection_type, ConnectionType::Telnet);
        assert_eq!(c.port, 23);
    }

    #[test]
    fn test_putty_url_decode() {
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("foo%23bar"), "foo#bar");
        assert_eq!(url_decode("no_encoding"), "no_encoding");
    }

    // ── SecureCRT ──

    #[test]
    fn test_securecrt_basic() {
        let ini = r#"S:"Protocol Name"=SSH2
S:"Hostname"=10.0.1.50
D:"[SSH2] Port"=00000016
S:"Username"=deploy
S:"Identity Filename V2"=
D:"Is Session"=00000001
S:"Emulation"=Xterm
"#;
        let result = import_securecrt_str(ini, "prod-server").unwrap();
        assert_eq!(result.connections.len(), 1);
        let c = &result.connections[0];
        assert_eq!(c.name, "prod-server");
        assert_eq!(c.host, "10.0.1.50");
        assert_eq!(c.port, 22);
        assert_eq!(c.username, "deploy");
        assert_eq!(c.auth, AuthMethod::Password);
    }

    #[test]
    fn test_securecrt_with_key() {
        let ini = r#"S:"Protocol Name"=SSH2
S:"Hostname"=example.com
D:"[SSH2] Port"=00000016
S:"Username"=admin
S:"Identity Filename V2"=C:\Users\admin\.ssh\id_rsa
D:"Is Session"=00000001
"#;
        let result = import_securecrt_str(ini, "keyhost").unwrap();
        let c = &result.connections[0];
        assert!(matches!(&c.auth, AuthMethod::PublicKey { key_path } if key_path.to_str().unwrap().contains("id_rsa")));
    }

    #[test]
    fn test_securecrt_not_session() {
        let ini = r#"S:"Protocol Name"=SSH2
S:"Hostname"=example.com
D:"Is Session"=00000000
"#;
        let result = import_securecrt_str(ini, "folder").unwrap();
        assert_eq!(result.connections.len(), 0);
        assert!(!result.warnings.is_empty());
    }

    // ── Termius ──

    #[test]
    fn test_termius_basic() {
        let json = r#"{
  "host_set": [
    {"id": 1, "label": "Prod Web", "address": "10.0.1.50", "group": 1, "ssh_config": 1}
  ],
  "group_set": [
    {"id": 1, "label": "Production"}
  ],
  "sshconfig_set": [
    {"id": 1, "port": 2222, "identity": 1, "use_ssh_key": false}
  ],
  "identity_set": [
    {"id": 1, "label": "deploy", "username": "deploy", "ssh_key": null}
  ],
  "sshkeycrypt_set": []
}"#;
        let result = import_termius_str(json).unwrap();
        assert_eq!(result.connections.len(), 1);
        let c = &result.connections[0];
        assert_eq!(c.name, "Prod Web");
        assert_eq!(c.host, "10.0.1.50");
        assert_eq!(c.port, 2222);
        assert_eq!(c.username, "deploy");
        assert_eq!(c.group.as_deref(), Some("Production"));
    }

    #[test]
    fn test_termius_with_ssh_key() {
        let json = r#"{
  "host_set": [
    {"id": 1, "label": "Key Host", "address": "example.com", "group": null, "ssh_config": 1}
  ],
  "group_set": [],
  "sshconfig_set": [
    {"id": 1, "port": 22, "identity": 1, "use_ssh_key": true}
  ],
  "identity_set": [
    {"id": 1, "label": "keyuser", "username": "keyuser", "ssh_key": 1}
  ],
  "sshkeycrypt_set": [
    {"id": 1, "label": "mykey", "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\ndata\n-----END OPENSSH PRIVATE KEY-----", "public_key": "ssh-ed25519 AAAA"}
  ]
}"#;
        let result = import_termius_str(json).unwrap();
        assert_eq!(result.connections.len(), 1);
        let c = &result.connections[0];
        assert_eq!(c.auth, AuthMethod::Password); // Inline key → Password fallback
        assert!(result.warnings.iter().any(|w| w.contains("inline")));
    }

    #[test]
    fn test_termius_no_hosts() {
        let json = r#"{"group_set":[],"sshconfig_set":[],"identity_set":[],"sshkeycrypt_set":[]}"#;
        let result = import_termius_str(json).unwrap();
        assert_eq!(result.connections.len(), 0);
        assert!(result.warnings.iter().any(|w| w.contains("No host_set")));
    }

    // ── Xshell ──

    #[test]
    fn test_xshell_basic() {
        let xsh = r#"[CONNECTION]
Host=10.0.1.50
Port=2222
Protocol=SSH

[CONNECTION:AUTHENTICATION]
UserName=deploy
Method=PASSWORD

[TERMINAL]
Type=xterm-256color
Encoding=UTF-8
"#;
        let result = import_xshell_str(xsh, "prod-server").unwrap();
        assert_eq!(result.connections.len(), 1);
        let c = &result.connections[0];
        assert_eq!(c.name, "prod-server");
        assert_eq!(c.host, "10.0.1.50");
        assert_eq!(c.port, 2222);
        assert_eq!(c.username, "deploy");
        assert_eq!(c.terminal_type, "xterm-256color");
    }

    #[test]
    fn test_xshell_publickey() {
        let xsh = r#"[CONNECTION]
Host=example.com
Port=22
Protocol=SSH

[CONNECTION:AUTHENTICATION]
UserName=admin
Method=PUBLICKEY
UserKeyFile=C:\Users\admin\.ssh\id_rsa
"#;
        let result = import_xshell_str(xsh, "keyhost").unwrap();
        let c = &result.connections[0];
        assert!(matches!(&c.auth, AuthMethod::PublicKey { key_path } if key_path.to_str().unwrap().contains("id_rsa")));
    }

    #[test]
    fn test_xshell_no_host() {
        let xsh = "[CONNECTION]\nPort=22\nProtocol=SSH\n";
        let result = import_xshell_str(xsh, "empty").unwrap();
        assert_eq!(result.connections.len(), 0);
        assert!(!result.warnings.is_empty());
    }

    // ── Export ──

    #[test]
    fn test_export_openssh() {
        let conns = vec![
            ConnectionConfig {
                name: "prod web".to_string(),
                host: "10.0.1.50".to_string(),
                port: 2222,
                username: "deploy".to_string(),
                auth: AuthMethod::PublicKey { key_path: PathBuf::from("/home/user/.ssh/id_ed25519") },
                proxy_jump: Some("bastion".to_string()),
                keepalive_interval: 30,
                ..Default::default()
            },
            ConnectionConfig {
                name: "staging".to_string(),
                host: "staging.example.com".to_string(),
                port: 22,
                username: "admin".to_string(),
                auth: AuthMethod::Password,
                ..Default::default()
            },
        ];
        let output = export_openssh(&conns);
        assert!(output.contains("Host prod-web"));
        assert!(output.contains("HostName 10.0.1.50"));
        assert!(output.contains("Port 2222"));
        assert!(output.contains("IdentityFile /home/user/.ssh/id_ed25519"));
        assert!(output.contains("ProxyJump bastion"));
        assert!(output.contains("ServerAliveInterval 30"));
        assert!(output.contains("Host staging"));
        // The second connection has default port 22 from Default::default(),
        // but also keepalive_interval=60 from default, so "Port 22" should not appear
        // for the staging entry. However the default ConnectionConfig has port=22
        // which we explicitly set, so it won't be emitted. Check staging section specifically.
        let staging_section = output.split("Host staging").nth(1).unwrap_or("");
        assert!(!staging_section.contains("Port "), "Default port 22 should be omitted for staging");
    }

    #[test]
    fn test_export_skips_non_ssh() {
        let conns = vec![ConnectionConfig {
            connection_type: ConnectionType::Local,
            ..Default::default()
        }];
        let output = export_openssh(&conns);
        assert!(!output.contains("Host "));
    }

    // ── BOM decode ──

    #[test]
    fn test_decode_utf8_no_bom() {
        let s = "hello world";
        assert_eq!(decode_with_bom(s.as_bytes()), "hello world");
    }

    #[test]
    fn test_decode_utf8_with_bom() {
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice(b"hello");
        assert_eq!(decode_with_bom(&raw), "hello");
    }

    #[test]
    fn test_decode_utf16le_with_bom() {
        // UTF-16 LE BOM + "Hi"
        let raw: Vec<u8> = vec![0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
        assert_eq!(decode_with_bom(&raw), "Hi");
    }

    // ── Auto-detect ──

    #[test]
    fn test_auto_detect_openssh() {
        let dir = std::env::temp_dir().join("stealthterm_import_auto");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config");
        std::fs::write(&path, "Host myhost\n    HostName example.com\n").unwrap();

        let result = import_auto(&path).unwrap();
        assert_eq!(result.connections.len(), 1);
        assert_eq!(result.connections[0].host, "example.com");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
