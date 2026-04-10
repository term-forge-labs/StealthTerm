use tokio::sync::mpsc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SshSessionError {
    #[error("Connection failed: {0}")]
    Connect(String),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Channel error: {0}")]
    Channel(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SshSessionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticated,
    Error(String),
}

pub struct SshSession {
    pub state: SshSessionState,
    pub input_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    pub output_rx: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
    pub cols: u16,
    pub rows: u16,
}

impl SshSession {
    pub fn new() -> Self {
        Self {
            state: SshSessionState::Disconnected,
            input_tx: None,
            output_rx: None,
            cols: 80,
            rows: 24,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, SshSessionState::Authenticated)
    }

    pub fn send(&self, data: &[u8]) -> Result<(), SshSessionError> {
        if let Some(tx) = &self.input_tx {
            tx.send(data.to_vec())
                .map_err(|e| SshSessionError::Channel(e.to_string()))?;
        }
        Ok(())
    }
}

impl Default for SshSession {
    fn default() -> Self {
        Self::new()
    }
}
