use std::collections::VecDeque;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

const MAX_HISTORY: usize = 10_000;
const HISTORY_FILENAME: &str = "history.json";

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// On-disk representation — only the entries are persisted
#[derive(Debug, Serialize, Deserialize)]
struct HistoryFile {
    entries: VecDeque<String>,
}

/// Persistent command history with dedup and search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHistory {
    entries: VecDeque<String>,
    #[serde(skip)]
    search_index: Option<usize>,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    file_path: Option<PathBuf>,
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(1000),
            search_index: None,
            dirty: false,
            file_path: None,
        }
    }
}

impl CommandHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build directly from sorted entries (newest-first order)
    pub fn from_entries(entries: VecDeque<String>) -> Self {
        Self {
            entries,
            search_index: None,
            dirty: false,
            file_path: None,
        }
    }

    /// Default history file path: `~/.config/stealthterm/history.json`
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stealthterm")
            .join(HISTORY_FILENAME)
    }

    /// Load history from disk (or return empty history if file doesn't exist)
    pub fn load() -> Result<Self, HistoryError> {
        Self::load_from(Self::default_path())
    }

    /// Load history from a specific file path
    pub fn load_from(path: PathBuf) -> Result<Self, HistoryError> {
        let mut history = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let file: HistoryFile = serde_json::from_str(&content)?;
            debug!("Loaded {} history entries from {}", file.entries.len(), path.display());
            Self {
                entries: file.entries,
                search_index: None,
                dirty: false,
                file_path: None,
            }
        } else {
            debug!("No history file at {}, starting fresh", path.display());
            Self::default()
        };
        // Enforce max limit on loaded data
        while history.entries.len() > MAX_HISTORY {
            history.entries.pop_back();
        }
        history.file_path = Some(path);
        history.dirty = false;
        Ok(history)
    }

    /// Save history to disk (uses the path it was loaded from, or the default)
    pub fn save(&mut self) -> Result<(), HistoryError> {
        let path = self.file_path.clone().unwrap_or_else(Self::default_path);
        self.save_to(&path)
    }

    /// Save history to a specific file path
    pub fn save_to(&mut self, path: &PathBuf) -> Result<(), HistoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = HistoryFile {
            entries: self.entries.clone(),
        };
        let content = serde_json::to_string_pretty(&file)?;
        std::fs::write(path, content)?;
        self.dirty = false;
        debug!("Saved {} history entries to {}", self.entries.len(), path.display());
        Ok(())
    }

    /// Save only if there are unsaved changes
    pub fn save_if_dirty(&mut self) -> Result<(), HistoryError> {
        if self.dirty {
            self.save()
        } else {
            Ok(())
        }
    }

    /// Whether there are unsaved changes
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Add a command to history (dedup: removes previous identical entry)
    pub fn push(&mut self, cmd: impl Into<String>) {
        let cmd = cmd.into();
        if cmd.trim().is_empty() {
            return;
        }

        // Detect sensitive info; skip saving if found
        if Self::contains_sensitive_info(&cmd) {
            debug!("Skipping sensitive command from history");
            return;
        }

        // Remove duplicate if exists
        self.entries.retain(|e| e != &cmd);
        self.entries.push_front(cmd);
        if self.entries.len() > MAX_HISTORY {
            self.entries.pop_back();
        }
        self.search_index = None;
        self.dirty = true;
    }

    /// Detect whether a command contains sensitive information
    fn contains_sensitive_info(cmd: &str) -> bool {
        let lower = cmd.to_lowercase();
        lower.contains("password=")
            || lower.contains("token=")
            || lower.contains("api_key=")
            || lower.contains("secret=")
            || lower.contains("apikey=")
    }

    /// Add a command and immediately persist to disk
    pub fn push_and_save(&mut self, cmd: impl Into<String>) {
        self.push(cmd);
        if let Err(e) = self.save() {
            warn!("Failed to save history: {}", e);
        }
    }

    /// Navigate backward in history (older)
    pub fn prev(&mut self) -> Option<&str> {
        let len = self.entries.len();
        if len == 0 {
            return None;
        }
        let idx = match self.search_index {
            None => 0,
            Some(i) if i + 1 < len => i + 1,
            Some(i) => i,
        };
        self.search_index = Some(idx);
        Some(&self.entries[idx])
    }

    /// Navigate forward in history (newer)
    pub fn next(&mut self) -> Option<&str> {
        match self.search_index {
            None | Some(0) => {
                self.search_index = None;
                None
            }
            Some(i) => {
                self.search_index = Some(i - 1);
                Some(&self.entries[i - 1])
            }
        }
    }

    /// Reset navigation position
    pub fn reset_nav(&mut self) {
        self.search_index = None;
    }

    /// Find best inline suggestion for prefix
    pub fn suggest(&self, prefix: &str) -> Option<&str> {
        if prefix.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|e| e.starts_with(prefix) && e.as_str() != prefix)
            .map(|s| s.as_str())
    }

    /// Reverse search
    pub fn search(&self, query: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.contains(query))
            .map(|s| s.as_str())
            .collect()
    }

    pub fn entries(&self) -> &VecDeque<String> {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Drop for CommandHistory {
    fn drop(&mut self) {
        if self.dirty {
            if let Err(e) = self.save() {
                warn!("Failed to save history on drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_push_dedup() {
        let mut h = CommandHistory::new();
        h.push("ls");
        h.push("cd /tmp");
        h.push("ls");
        assert_eq!(h.len(), 2);
        assert_eq!(h.entries()[0], "ls");
        assert_eq!(h.entries()[1], "cd /tmp");
    }

    #[test]
    fn test_push_marks_dirty() {
        let mut h = CommandHistory::new();
        assert!(!h.is_dirty());
        h.push("ls");
        assert!(h.is_dirty());
    }

    #[test]
    fn test_empty_and_whitespace_ignored() {
        let mut h = CommandHistory::new();
        h.push("");
        h.push("   ");
        assert!(h.is_empty());
        assert!(!h.is_dirty());
    }

    #[test]
    fn test_max_history_enforced() {
        let mut h = CommandHistory::new();
        for i in 0..MAX_HISTORY + 100 {
            h.push(format!("cmd-{}", i));
        }
        assert_eq!(h.len(), MAX_HISTORY);
    }

    #[test]
    fn test_navigation() {
        let mut h = CommandHistory::new();
        h.push("first");
        h.push("second");
        h.push("third");
        // prev walks newest→oldest
        assert_eq!(h.prev(), Some("third"));
        assert_eq!(h.prev(), Some("second"));
        assert_eq!(h.prev(), Some("first"));
        // stays at oldest
        assert_eq!(h.prev(), Some("first"));
        // next walks back
        assert_eq!(h.next(), Some("second"));
        assert_eq!(h.next(), Some("third"));
        // past newest returns None
        assert_eq!(h.next(), None);
    }

    #[test]
    fn test_suggest() {
        let mut h = CommandHistory::new();
        h.push("git commit -m 'fix'");
        h.push("git push origin main");
        assert_eq!(h.suggest("git p"), Some("git push origin main"));
        assert_eq!(h.suggest("git c"), Some("git commit -m 'fix'"));
        assert_eq!(h.suggest("docker"), None);
        assert_eq!(h.suggest(""), None);
    }

    #[test]
    fn test_search() {
        let mut h = CommandHistory::new();
        h.push("cargo build");
        h.push("cargo test");
        h.push("git status");
        let results = h.search("cargo");
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"cargo build"));
        assert!(results.contains(&"cargo test"));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("stealthterm_test_history");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");

        // Save
        let mut h = CommandHistory::new();
        h.push("echo hello");
        h.push("ls -la");
        h.push("cargo build");
        h.save_to(&path).unwrap();
        assert!(!h.is_dirty());

        // Load
        let loaded = CommandHistory::load_from(path.clone()).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded.entries()[0], "cargo build");
        assert_eq!(loaded.entries()[1], "ls -la");
        assert_eq!(loaded.entries()[2], "echo hello");
        assert!(!loaded.is_dirty());

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_missing_file() {
        let path = std::env::temp_dir().join("stealthterm_nonexistent_history.json");
        let _ = std::fs::remove_file(&path);
        let h = CommandHistory::load_from(path).unwrap();
        assert!(h.is_empty());
    }

    #[test]
    fn test_load_corrupt_file() {
        let dir = std::env::temp_dir().join("stealthterm_test_corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"not json {{{{").unwrap();
        let result = CommandHistory::load_from(path);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_if_dirty() {
        let dir = std::env::temp_dir().join("stealthterm_test_dirty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");

        let mut h = CommandHistory::load_from(path.clone()).unwrap();
        // Not dirty, save_if_dirty should be no-op
        assert!(!path.exists());
        h.save_if_dirty().unwrap();
        assert!(!path.exists());

        // Now push and save_if_dirty should write
        h.push("test");
        h.save_if_dirty().unwrap();
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
