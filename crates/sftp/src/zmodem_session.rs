use std::path::PathBuf;
use std::io::Write;
use std::fs::File;
use zmodem2::{Receiver, Sender, ReceiverEvent, SenderEvent};

#[derive(Debug, PartialEq)]
pub enum TerminalState {
    Normal,
    Receiving,
    Sending,
}

enum TransferState {
    Receiving {
        receiver: Receiver,
        file: Option<File>,
    },
    Sending {
        sender: Sender,
    },
}

pub struct ZmodemSession {
    pub state: TerminalState,
    pub download_dir: PathBuf,
    transfer_state: Option<TransferState>,
    rx_buf: Vec<u8>,  // persistent receive buffer
}

impl ZmodemSession {
    pub fn new() -> Self {
        Self {
            state: TerminalState::Normal,
            download_dir: PathBuf::from("."),
            transfer_state: None,
            rx_buf: Vec::new(),
        }
    }

    pub fn set_download_dir(&mut self, dir: PathBuf) {
        self.download_dir = dir;
    }

    pub fn detect_signature(&self, data: &[u8]) -> bool {
        let signature = b"**\x18B";
        data.windows(signature.len()).any(|w| w == signature)
    }

    pub fn is_active(&self) -> bool {
        self.transfer_state.is_some()
    }

    pub fn poll_receiver_event(&mut self) -> Option<ReceiverEvent> {
        if let Some(TransferState::Receiving { receiver, .. }) = &mut self.transfer_state {
            let event = receiver.poll_event();
            if event.is_some() {
                tracing::info!(">>> [ZMODEM2 SESSION] poll_receiver_event returned: {:?}", event);
            }
            event
        } else {
            None
        }
    }

    pub fn drain_receiver_file(&mut self) -> &[u8] {
        if let Some(TransferState::Receiving { receiver, .. }) = &mut self.transfer_state {
            receiver.drain_file()
        } else {
            &[]
        }
    }

    pub fn advance_receiver_file(&mut self, n: usize) -> Result<(), String> {
        if let Some(TransferState::Receiving { receiver, .. }) = &mut self.transfer_state {
            receiver.advance_file(n).map_err(|e| format!("Advance error: {:?}", e))
        } else {
            Ok(())
        }
    }

    pub fn get_file_name(&self) -> Option<String> {
        if let Some(TransferState::Receiving { receiver, .. }) = &self.transfer_state {
            Some(String::from_utf8_lossy(receiver.file_name()).to_string())
        } else {
            None
        }
    }

    pub fn start_receive(&mut self) {
        match Receiver::new() {
            Ok(receiver) => {
                self.state = TerminalState::Receiving;
                self.transfer_state = Some(TransferState::Receiving {
                    receiver,
                    file: None,
                });
                tracing::info!("ZMODEM receive started");
            }
            Err(e) => {
                tracing::error!("Failed to create receiver: {:?}", e);
            }
        }
    }

    pub fn start_send(&mut self, _file_path: PathBuf) -> Result<(), String> {
        match Sender::new() {
            Ok(sender) => {
                self.state = TerminalState::Sending;
                self.transfer_state = Some(TransferState::Sending { sender });
                tracing::info!("ZMODEM send started");
                Ok(())
            }
            Err(e) => Err(format!("Failed to create sender: {:?}", e))
        }
    }

    pub fn handle_data(&mut self, data: &[u8]) -> Result<(), String> {
        match self.state {
            TerminalState::Normal => {
                if self.detect_signature(data) {
                    self.start_receive();
                    self.append_raw_data(data);
                }
            }
            TerminalState::Receiving => {
                self.append_raw_data(data);
            }
            TerminalState::Sending => {
                self.process_sending(data)?;
            }
        }
        Ok(())
    }

    // Only responsible for pushing data into the buffer, filtering out XON/XOFF
    fn append_raw_data(&mut self, data: &[u8]) {
        for &b in data {
            if b != 0x11 && b != 0x13 {
                self.rx_buf.push(b);
            }
        }
    }

    // Single-step state machine pump
    pub fn pump_rx_buf(&mut self) -> Result<usize, String> {
        if let Some(TransferState::Receiving { receiver, .. }) = &mut self.transfer_state {
            if self.rx_buf.is_empty() {
                return Ok(0);
            }
            let consumed = receiver.feed_incoming(&self.rx_buf)
                .map_err(|e| format!("Feed error: {:?}", e))?;

            if consumed > 0 {
                self.rx_buf.drain(..consumed);
            }
            Ok(consumed)
        } else {
            Ok(0)
        }
    }

    fn process_sending(&mut self, data: &[u8]) -> Result<(), String> {
        if let Some(TransferState::Sending { sender }) = &mut self.transfer_state {
            sender.feed_incoming(data).map_err(|e| format!("Feed error: {:?}", e))?;

            while let Some(event) = sender.poll_event() {
                match event {
                    SenderEvent::FileComplete => {
                        tracing::info!("File send complete");
                    }
                    SenderEvent::SessionComplete => {
                        tracing::info!("Session complete");
                        self.reset();
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_outgoing(&mut self) -> Option<&[u8]> {
        match &mut self.transfer_state {
            Some(TransferState::Receiving { receiver, .. }) => {
                let data = receiver.drain_outgoing();
                tracing::info!(">>> [ZMODEM2 SESSION] drain_outgoing returned {} bytes", data.len());
                if data.is_empty() { None } else { Some(data) }
            }
            Some(TransferState::Sending { sender }) => {
                let data = sender.drain_outgoing();
                if data.is_empty() { None } else { Some(data) }
            }
            None => None,
        }
    }

    pub fn advance_outgoing(&mut self, n: usize) {
        match &mut self.transfer_state {
            Some(TransferState::Receiving { receiver, .. }) => {
                receiver.advance_outgoing(n);
            }
            Some(TransferState::Sending { sender }) => {
                sender.advance_outgoing(n);
            }
            None => {}
        }
    }

    pub fn reset(&mut self) {
        self.state = TerminalState::Normal;
        self.transfer_state = None;
        self.rx_buf.clear();
    }
}

impl Default for ZmodemSession {
    fn default() -> Self {
        Self::new()
    }
}
