use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::debug;
use russh_sftp::client::SftpSession;

#[derive(Debug, Error)]
pub enum SftpError {
    #[error("SFTP not connected")]
    NotConnected,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Transfer error: {0}")]
    Transfer(String),
    #[error("SFTP error: {0}")]
    Sftp(String),
}

#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub permissions: u32,
    pub modified: u64,
}

pub struct SftpClient {
    session: Option<SftpSession>,
}

impl SftpClient {
    pub fn new() -> Self {
        Self { session: None }
    }

    pub fn from_session(session: SftpSession) -> Self {
        Self { session: Some(session) }
    }

    pub async fn list_dir(&mut self, path: &Path) -> Result<Vec<RemoteEntry>, SftpError> {
        let session = self.session.as_ref().ok_or(SftpError::NotConnected)?;

        let path_str = path.to_string_lossy().to_string();
        tracing::info!("SFTP list_dir: {}", path_str);

        let entries = session.read_dir(path_str).await
            .map_err(|e| SftpError::Sftp(e.to_string()))?;

        let mut result = Vec::new();
        for entry in entries {
            let metadata = entry.metadata();
            let is_dir = metadata.is_dir();
            let size = metadata.size.unwrap_or(0);
            let perms = metadata.permissions.unwrap_or(0);

            tracing::info!(">>> SFTP Entry: name={}, is_dir={}, size={}, perms={:o}",
                entry.file_name(), is_dir, size, perms);

            // Use forward slashes to join remote path
            let path_str = path.to_string_lossy();
            let entry_path = if path_str.ends_with('/') {
                format!("{}{}", path_str, entry.file_name())
            } else {
                format!("{}/{}", path_str, entry.file_name())
            };

            result.push(RemoteEntry {
                name: entry.file_name().to_string(),
                path: PathBuf::from(entry_path),
                size,
                is_dir,
                permissions: perms,
                modified: metadata.mtime.unwrap_or(0) as u64,
            });
        }
        tracing::info!("SFTP list_dir returned {} entries", result.len());
        Ok(result)
    }

    pub async fn upload(&mut self, local: &Path, remote: &Path) -> Result<(), SftpError> {
        let session = self.session.as_ref().ok_or(SftpError::NotConnected)?;

        let remote_str = remote.to_string_lossy().to_string();
        tracing::info!("SFTP uploading to {}", remote_str);

        // Open local file
        let mut local_file = tokio::fs::File::open(local).await?;

        // Create remote file
        let mut remote_file = session.create(remote_str).await
            .map_err(|e| SftpError::Sftp(format!("create failed: {}", e)))?;

        // Read in chunks and write
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = [0u8; 8192];
        let mut total = 0;

        loop {
            let n = local_file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            remote_file.write_all(&buf[..n]).await?;
            total += n;
        }

        remote_file.flush().await?;
        tracing::info!("SFTP upload completed: {} bytes", total);
        Ok(())
    }

    pub async fn download(&mut self, remote: &Path, local: &Path) -> Result<(), SftpError> {
        let session = self.session.as_ref().ok_or(SftpError::NotConnected)?;

        let remote_str = remote.to_string_lossy().to_string();
        let data = session.read(remote_str).await
            .map_err(|e| SftpError::Sftp(e.to_string()))?;
        tokio::fs::write(local, data).await?;

        debug!("SFTP download: {:?} -> {:?}", remote, local);
        Ok(())
    }

    pub async fn mkdir(&mut self, path: &Path) -> Result<(), SftpError> {
        let session = self.session.as_ref().ok_or(SftpError::NotConnected)?;
        let path_str = path.to_string_lossy().to_string();
        session.create_dir(path_str).await
            .map_err(|e| SftpError::Sftp(e.to_string()))?;
        Ok(())
    }

    pub async fn remove(&mut self, path: &Path) -> Result<(), SftpError> {
        let session = self.session.as_ref().ok_or(SftpError::NotConnected)?;
        let path_str = path.to_string_lossy().to_string();
        session.remove_file(path_str).await
            .map_err(|e| SftpError::Sftp(e.to_string()))?;
        Ok(())
    }

    pub async fn realpath(&mut self, path: &str) -> Result<PathBuf, SftpError> {
        let session = self.session.as_ref().ok_or(SftpError::NotConnected)?;
        tracing::info!("SFTP realpath called with: {}", path);
        let result = session.canonicalize(path).await
            .map_err(|e| SftpError::Sftp(e.to_string()))?;
        tracing::info!("SFTP realpath result: {}", result);
        Ok(PathBuf::from(result))
    }

    pub async fn rename(&mut self, from: &Path, to: &Path) -> Result<(), SftpError> {
        let session = self.session.as_ref().ok_or(SftpError::NotConnected)?;
        let from_str = from.to_string_lossy().to_string();
        let to_str = to.to_string_lossy().to_string();
        session.rename(from_str, to_str).await
            .map_err(|e| SftpError::Sftp(e.to_string()))?;
        Ok(())
    }
}

impl Default for SftpClient {
    fn default() -> Self {
        Self::new()
    }
}

