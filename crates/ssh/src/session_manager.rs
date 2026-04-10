use russh::client::Handle;
use russh::Channel;

pub struct SshSessionManager {
    handle: Handle<super::client::ClientHandler>,
}

impl SshSessionManager {
    pub fn new(handle: Handle<super::client::ClientHandler>) -> Self {
        Self { handle }
    }

    pub async fn open_terminal_channel(&self) -> Result<Channel<russh::client::Msg>, russh::Error> {
        self.handle.channel_open_session().await
    }

    pub async fn open_sftp_channel(&self) -> Result<Channel<russh::client::Msg>, russh::Error> {
        let channel = self.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        Ok(channel)
    }

    pub fn get_handle(&self) -> &Handle<super::client::ClientHandler> {
        &self.handle
    }
}
