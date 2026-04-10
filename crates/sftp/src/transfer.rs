use std::path::PathBuf;
// tokio channels used by transfer operations

#[derive(Debug, Clone, PartialEq)]
pub enum TransferStatus {
    Pending,
    InProgress { bytes_done: u64, total_bytes: u64 },
    Paused,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone)]
pub struct TransferTask {
    pub id: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub direction: TransferDirection,
    pub status: TransferStatus,
    pub speed_bps: f64,
}

impl TransferTask {
    pub fn progress(&self) -> f32 {
        match &self.status {
            TransferStatus::InProgress { bytes_done, total_bytes } => {
                if *total_bytes == 0 { 0.0 }
                else { *bytes_done as f32 / *total_bytes as f32 }
            }
            TransferStatus::Completed => 1.0,
            _ => 0.0,
        }
    }
}

pub struct TransferQueue {
    pub tasks: Vec<TransferTask>,
}

impl TransferQueue {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    pub fn add(&mut self, task: TransferTask) {
        self.tasks.push(task);
    }

    pub fn remove(&mut self, id: &str) {
        self.tasks.retain(|t| t.id != id);
    }

    pub fn active_count(&self) -> usize {
        self.tasks.iter().filter(|t| matches!(t.status, TransferStatus::InProgress { .. })).count()
    }
}

impl Default for TransferQueue {
    fn default() -> Self {
        Self::new()
    }
}
