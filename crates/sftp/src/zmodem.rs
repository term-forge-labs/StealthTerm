// Zmodem protocol support using system lrzsz tools

use std::path::PathBuf;
use tokio::process::{Command, Child};
use tracing::info;

pub struct ZmodemHandler {
    pub active: bool,
    pub mode: Option<ZmodemMode>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZmodemMode {
    Send,    // sz - send files from remote to local
    Receive, // rz - receive files from local to remote
}

impl ZmodemHandler {
    pub fn new() -> Self {
        Self {
            active: false,
            mode: None,
        }
    }

    /// Detect rz/sz invocation in terminal output
    pub fn detect(&mut self, data: &[u8]) -> Option<ZmodemMode> {
        // Detect full ZMODEM sequence **\x18B0 — multiple trailing 0s means sz send
        // rz is **\x18B00 (exactly two 0s), sz is **\x18B0000... (multiple 0s)
        if data.windows(8).any(|w| w == b"**\x18B0000") {
            info!("Detected sz command (send mode) via ZMODEM sequence");
            self.active = true;
            self.mode = Some(ZmodemMode::Send);
            return Some(ZmodemMode::Send);
        }

        // rz receive file: detect **\x18B00 (exactly two 0s)
        for i in 0..data.len().saturating_sub(6) {
            if &data[i..i+5] == b"**\x18B00" {
                // Check that the next byte is not another 0 (avoid false match with sz)
                if i + 6 >= data.len() || data[i+5] != b'0' {
                    info!("Detected rz command (receive mode) via ZMODEM sequence");
                    self.active = true;
                    self.mode = Some(ZmodemMode::Receive);
                    return Some(ZmodemMode::Receive);
                }
            }
        }

        None
    }

    pub fn bridge_upload(&mut self, files: Vec<PathBuf>) -> Result<Child, String> {
        if self.mode != Some(ZmodemMode::Receive) {
            return Err("Not in receive mode".to_string());
        }

        let mut cmd = Command::new("sz");
        cmd.arg("-e");
        for file in files {
            cmd.arg(file);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        cmd.spawn().map_err(|e| format!("Failed to start sz: {}", e))
    }

    pub fn bridge_download(&mut self, save_dir: PathBuf) -> Result<Child, String> {
        if self.mode != Some(ZmodemMode::Send) {
            return Err("Not in send mode".to_string());
        }

        let mut cmd = Command::new("rz");
        cmd.arg("-e");
        cmd.arg("-y"); // overwrite existing files
        cmd.current_dir(save_dir);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        cmd.spawn().map_err(|e| format!("Failed to start rz: {}", e))
    }
}


impl Default for ZmodemHandler {
    fn default() -> Self {
        Self::new()
    }
}
