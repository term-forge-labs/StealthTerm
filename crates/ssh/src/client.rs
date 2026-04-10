use crate::config::{SshAuth, SshConfig};
use crate::session::SshSessionError;
use stealthterm_config::i18n::t;
use tokio::sync::mpsc;
use tracing::{info, error, debug};
use std::sync::Arc;
use russh::client;
use russh_keys::key;
use russh_sftp::client::SftpSession;
use async_trait::async_trait;

pub struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // TODO: Implement proper host key verification
        Ok(true)
    }
}

pub struct SshClient;

impl SshClient {
    pub async fn connect(
        config: SshConfig,
        output_tx: mpsc::UnboundedSender<Vec<u8>>,
        sftp_tx: mpsc::UnboundedSender<SftpSession>,
        cols: u16,
        rows: u16,
    ) -> Result<(mpsc::UnboundedSender<Vec<u8>>, mpsc::UnboundedSender<(u16, u16)>), SshSessionError> {
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u16, u16)>();

        let config_clone = config.clone();
        tokio::spawn(async move {
            info!("Connecting to {}:{}", config_clone.host, config_clone.port);

            let ssh_config = Arc::new(russh::client::Config::default());
            let handler = ClientHandler;

            let addr = format!("{}:{}", config_clone.host, config_clone.port);
            let mut session = match client::connect(ssh_config, addr, handler).await {
                Ok(s) => s,
                Err(e) => {
                    error!("SSH connect failed: {}", e);
                    return;
                }
            };

            // Authenticate
            let auth_ok = match &config_clone.auth {
                SshAuth::Password(pwd) => {
                    session.authenticate_password(&config_clone.username, pwd).await
                        .unwrap_or(false)
                }
                SshAuth::PublicKey { key_path, passphrase } => {
                    info!("Attempting public key auth with key: {:?}", key_path);
                    match russh_keys::load_secret_key(key_path, passphrase.as_deref()) {
                        Ok(kp) => {
                            info!("Private key loaded successfully, key type: {:?}", kp.name());
                            let kp = Arc::new(kp);
                            match session.authenticate_publickey(&config_clone.username, kp).await {
                                Ok(true) => true,
                                Ok(false) => {
                                    let msg = t("ssh.error_pubkey_rejected");
                                    error!("{}", msg.trim());
                                    let _ = output_tx.send(msg.as_bytes().to_vec());
                                    false
                                }
                                Err(e) => {
                                    let msg = format!("{}: {}\r\n", t("ssh.error_pubkey_auth"), e);
                                    error!("{}", msg.trim());
                                    let _ = output_tx.send(msg.into_bytes());
                                    false
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to load private key {:?}: {}", key_path, e);
                            let msg = format!("{} {:?}: {}\r\n", t("ssh.error_load_key"), key_path, e);
                            let _ = output_tx.send(msg.into_bytes());
                            false
                        }
                    }
                }
            };

            if !auth_ok {
                error!("SSH authentication failed");
                let _ = output_tx.send(t("ssh.error_auth_failed").as_bytes().to_vec());
                return;
            }

            info!("SSH authenticated for {}", config_clone.username);

            // Create SFTP session
            if let Ok(sftp_channel) = session.channel_open_session().await {
                if sftp_channel.request_subsystem(true, "sftp").await.is_ok() {
                    if let Ok(sftp) = SftpSession::new(sftp_channel.into_stream()).await {
                        let _ = sftp_tx.send(sftp);
                        info!("SFTP session created");
                    }
                }
            }

            // Open a channel
            let mut channel = match session.channel_open_session().await {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to open SSH channel: {}", e);
                    return;
                }
            };

            // Request PTY with proper terminal modes
            if let Err(e) = channel.request_pty(
                true,
                &config_clone.terminal_type,
                cols as u32,
                rows as u32,
                0, 0,
                &[],
            ).await {
                error!("Failed to request PTY: {}", e);
                return;
            }

            info!("PTY requested: {}x{} ({})", cols, rows, config_clone.terminal_type);

            // Request the remote server to start the user's default login shell
            if let Err(e) = channel.request_shell(true).await {
                error!("Failed to request shell: {}", e);
                return;
            }
            info!("Shell started with TERM={} (256-color mode)", config_clone.terminal_type);

            // Bidirectional IO loop
            loop {
                tokio::select! {
                    msg = channel.wait() => {
                        match msg {
                            Some(russh::ChannelMsg::Data { data }) => {
                                if output_tx.send(data.to_vec()).is_err() {
                                    break;
                                }
                            }
                            Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                                if output_tx.send(data.to_vec()).is_err() {
                                    break;
                                }
                            }
                            Some(russh::ChannelMsg::Eof) | None => {
                                debug!("SSH channel closed");
                                break;
                            }
                            _ => {}
                        }
                    }
                    data = input_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                if let Err(e) = channel.data(bytes.as_slice()).await {
                                    error!("SSH write error: {}", e);
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    size = resize_rx.recv() => {
                        if let Some((new_cols, new_rows)) = size {
                            info!("SSH window-change: {}x{}", new_cols, new_rows);
                            if let Err(e) = channel.window_change(new_cols as u32, new_rows as u32, 0, 0).await {
                                error!("Failed to send window-change: {}", e);
                            }
                        }
                    }
                }
            }

            info!("SSH session ended");
        });

        Ok((input_tx, resize_tx))
    }

    /// Connect and execute a specific command (instead of an interactive shell).
    pub async fn connect_command(
        config: SshConfig,
        command: &str,
        output_tx: mpsc::UnboundedSender<Vec<u8>>,
        cols: u16,
        rows: u16,
    ) -> Result<mpsc::UnboundedSender<Vec<u8>>, SshSessionError> {
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let config_clone = config.clone();
        let command = command.to_string();
        tokio::spawn(async move {
            info!("Connecting to {}:{} (exec: {})", config_clone.host, config_clone.port, command);

            let ssh_config = Arc::new(russh::client::Config::default());
            let handler = ClientHandler;

            let addr = format!("{}:{}", config_clone.host, config_clone.port);
            let mut session = match client::connect(ssh_config, addr, handler).await {
                Ok(s) => s,
                Err(e) => {
                    error!("SSH connect failed: {}", e);
                    return;
                }
            };

            // Authenticate (same as connect())
            let auth_ok = match &config_clone.auth {
                SshAuth::Password(pwd) => {
                    session.authenticate_password(&config_clone.username, pwd).await
                        .unwrap_or(false)
                }
                SshAuth::PublicKey { key_path, passphrase } => {
                    info!("Attempting public key auth with key: {:?}", key_path);
                    match russh_keys::load_secret_key(key_path, passphrase.as_deref()) {
                        Ok(kp) => {
                            info!("Private key loaded successfully, key type: {:?}", kp.name());
                            let kp = Arc::new(kp);
                            match session.authenticate_publickey(&config_clone.username, kp).await {
                                Ok(true) => true,
                                Ok(false) => {
                                    let msg = t("ssh.error_pubkey_rejected");
                                    error!("{}", msg.trim());
                                    let _ = output_tx.send(msg.as_bytes().to_vec());
                                    false
                                }
                                Err(e) => {
                                    let msg = format!("{}: {}\r\n", t("ssh.error_pubkey_auth"), e);
                                    error!("{}", msg.trim());
                                    let _ = output_tx.send(msg.into_bytes());
                                    false
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to load private key {:?}: {}", key_path, e);
                            let msg = format!("{} {:?}: {}\r\n", t("ssh.error_load_key"), key_path, e);
                            let _ = output_tx.send(msg.into_bytes());
                            false
                        }
                    }
                }
            };

            if !auth_ok {
                error!("SSH authentication failed");
                let _ = output_tx.send(t("ssh.error_auth_failed").as_bytes().to_vec());
                return;
            }

            info!("SSH authenticated for {} (command mode)", config_clone.username);

            let mut channel = match session.channel_open_session().await {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to open SSH channel: {}", e);
                    return;
                }
            };

            // Request PTY
            if let Err(e) = channel.request_pty(
                true,
                &config_clone.terminal_type,
                cols as u32,
                rows as u32,
                0, 0,
                &[],
            ).await {
                error!("Failed to request PTY: {}", e);
                return;
            }

            // Execute command instead of shell
            if let Err(e) = channel.exec(true, command.as_bytes()).await {
                error!("Failed to exec command '{}': {}", command, e);
                return;
            }

            // Bidirectional IO loop (identical to connect())
            loop {
                tokio::select! {
                    msg = channel.wait() => {
                        match msg {
                            Some(russh::ChannelMsg::Data { data }) => {
                                if output_tx.send(data.to_vec()).is_err() {
                                    break;
                                }
                            }
                            Some(russh::ChannelMsg::Eof) | None => {
                                debug!("SSH command channel closed");
                                break;
                            }
                            _ => {}
                        }
                    }
                    data = input_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                if let Err(e) = channel.data(bytes.as_slice()).await {
                                    error!("SSH write error: {}", e);
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }

            info!("SSH command session ended");
        });

        Ok(input_tx)
    }
}
