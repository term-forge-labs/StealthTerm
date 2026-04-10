use egui::Ui;
use egui::Sense;
use stealthterm_terminal::emulator::TerminalEmulator;
use stealthterm_terminal::pty::{PtySession, PtySize};
use stealthterm_terminal::search::{SearchState, SearchResult};
use stealthterm_terminal::selection::SelectionMode;
use stealthterm_terminal::CompletionEngine;
use stealthterm_terminal::CommandFoldManager;
use stealthterm_ssh::config::SshConfig;
use stealthterm_ssh::client::SshClient;
use stealthterm_sftp::{SftpClient, zmodem::{ZmodemHandler, ZmodemMode}, ZmodemSession, ReceiverEvent};
use stealthterm_config::EncryptedHistoryStore;
use stealthterm_config::i18n::{t, tf};
use stealthterm_utils::CommandHistory;
use tokio::sync::mpsc;
use std::sync::Arc;
use std::path::PathBuf;
use std::io::Write;
use crate::theme::Theme;
use crate::widgets::terminal_view::TerminalView;
use crate::widgets::input_area::InputArea;
use crate::widgets::search_bar::{SearchBar, SearchAction};

/// Single file receive object, responsible for managing file handle lifetime
struct ActiveFile {
    writer: Option<std::io::BufWriter<std::fs::File>>,
    path: PathBuf,
}

impl ActiveFile {
    fn new(path: PathBuf) -> std::io::Result<Self> {
        let file = std::fs::File::create(&path)?;
        Ok(Self {
            writer: Some(std::io::BufWriter::new(file)),
            path,
        })
    }

    fn write_chunk(&mut self, data: &[u8]) -> std::io::Result<()> {
        if let Some(w) = self.writer.as_mut() {
            w.write_all(data)?;
        }
        Ok(())
    }

    fn finish(mut self) -> std::io::Result<PathBuf> {
        if let Some(mut w) = self.writer.take() {
            w.flush()?;
            let file = w.into_inner()?;
            file.sync_all()?;
            drop(file);
            tracing::warn!(">>> [ActiveFile] File handle released: {:?}", self.path);
        }
        Ok(self.path.clone())
    }
}

impl Drop for ActiveFile {
    fn drop(&mut self) {
        if let Some(mut w) = self.writer.take() {
            let _ = w.flush();
            tracing::warn!(">>> [ActiveFile] File handle dropped in Drop");
        }
    }
}

/// Abstraction over the terminal backend (local PTY or SSH)
#[derive(Clone)]
enum Backend {
    /// Local PTY session
    Pty {
        writer: Arc<tokio::sync::Mutex<Box<dyn std::io::Write + Send>>>,
        master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    },
    /// SSH remote session
    Ssh {
        input_tx: mpsc::UnboundedSender<Vec<u8>>,
        resize_tx: mpsc::UnboundedSender<(u16, u16)>,
    },
    /// No backend connected yet
    None,
}

impl Backend {
    fn from_pty(pty: &PtySession) -> Self {
        Backend::Pty {
            writer: pty.writer.clone(),
            master: pty.master.clone(),
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            Backend::Pty { .. } => true,
            Backend::Ssh { .. } => true,
            Backend::None => false,
        }
    }

    fn write(&self, data: &[u8]) {
        match self {
            Backend::Pty { writer, .. } => {
                let writer = writer.clone();
                let data = data.to_vec();
                tokio::spawn(async move {
                    let mut w = writer.lock().await;
                    use std::io::Write;
                    let _ = w.write_all(&data);
                });
            }
            Backend::Ssh { input_tx, .. } => {
                let _ = input_tx.send(data.to_vec());
            }
            Backend::None => {}
        }
    }

    fn resize(&self, cols: u16, rows: u16) {
        match self {
            Backend::Pty { master, .. } => {
                if let Ok(m) = master.lock() {
                    let _ = m.resize(portable_pty::PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                    tracing::info!("PTY resize sent: {}x{}", cols, rows);
                }
            }
            Backend::Ssh { resize_tx, .. } => {
                let _ = resize_tx.send((cols, rows));
                tracing::info!("SSH resize sent: {}x{}", cols, rows);
            }
            Backend::None => {}
        }
    }
}

pub struct TerminalPanel {
    pub emulator: TerminalEmulator,
    backend: Backend,
    /// Keep PtySession alive so the child process isn't dropped
    _pty: Option<PtySession>,
    output_rx: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
    /// Shared slot for SSH to deliver its input_tx back to us
    ssh_input_tx: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>>,
    ssh_resize_tx: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<(u16, u16)>>>>,
    /// Shared slot for SFTP client
    sftp_slot: Option<Arc<tokio::sync::Mutex<Option<SftpClient>>>>,
    pub view: TerminalView,
    pub input_area: InputArea,
    pub search_bar: SearchBar,
    pub search_state: SearchState,
    pub cols: usize,
    pub rows: usize,
    pub zmodem: ZmodemHandler,
    pub zmodem_native: ZmodemSession,
    zmodem_current_file: Option<ActiveFile>,
    pub pending_upload: Option<PathBuf>,
    pub pending_download: Option<PathBuf>,
    pub zmodem_download_active: bool,
    pub zmodem_download_triggered: bool,
    pub zmodem_data_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
    pub sftp_client: Option<SftpClient>,
    pub current_dir: PathBuf,
    pub is_root_user: bool,
    pub zmodem_upload_triggered: bool,
    pub upload_success_message: Option<String>,
    show_context_menu: bool,
    context_menu_pos: egui::Pos2,
    show_paste_confirm: bool,
    pending_paste_text: String,
    paste_convert_tabs: bool,
    paste_remove_crlf: bool,
    initial_resize_done: bool,
    pending_ssh_config: Option<(SshConfig, Arc<tokio::sync::Mutex<Option<SftpClient>>>)>,
    zmodem_idle_frames: usize,
    /// IME pre-edit text (input method candidate characters)
    ime_preedit: String,
    /// Batch mode broadcast buffer: records all user input in this frame for app to broadcast to other terminals
    pub broadcast_buffer: Vec<Vec<u8>>,
    /// Scroll accumulator to prevent small deltas from being truncated to 0
    scroll_accumulator: f32,
    /// Session identifier for isolating command history
    session_key: Option<String>,
    /// Encrypted history store
    history_store: Option<EncryptedHistoryStore>,
    /// Completion engine
    completion_engine: Option<CompletionEngine>,
    /// Current ghost text suggestion (suffix after cursor)
    current_suggestion: Option<String>,
    /// Input prefix from the previous frame, used to detect changes
    last_input_prefix: String,
    /// Whether to show the completion dropdown
    show_completion_popup: bool,
    /// Completion candidate list
    completion_candidates: Vec<String>,
    /// Currently selected candidate index (None = nothing selected)
    completion_selected: Option<usize>,
    /// Terminal area rect (used to position the dropdown)
    terminal_rect_cache: egui::Rect,
    /// User keystroke input buffer (excludes prompt; records only the user's actual command)
    input_buffer: String,
    /// Cursor column at end of prompt, used to read the actual command from the screen
    prompt_end_col: usize,
    /// Row number (grid row) where the prompt is located
    prompt_row: usize,
    /// Waiting for first user input (set to true after Enter; records prompt position on first keypress)
    awaiting_prompt_capture: bool,
    /// Command output fold manager
    fold_manager: CommandFoldManager,
    /// Whether to show the fold context menu
    show_fold_context_menu: bool,
    /// Fold context menu position
    fold_context_menu_pos: egui::Pos2,
    /// prompt_line associated with the fold context menu
    fold_context_prompt_line: usize,
    /// Process has exited (PTY EOF or SSH disconnected)
    process_exited: bool,
    /// Previous frame's is_interactive() result, for transition logging
    prev_interactive: bool,
}

impl TerminalPanel {
    pub fn new(cols: usize, rows: usize, font_size: f32) -> Self {
        Self {
            emulator: TerminalEmulator::new(cols, rows),
            backend: Backend::None,
            _pty: None,
            output_rx: None,
            ssh_input_tx: Arc::new(tokio::sync::Mutex::new(None)),
            ssh_resize_tx: Arc::new(tokio::sync::Mutex::new(None)),
            sftp_slot: None,
            view: TerminalView::new(font_size),
            input_area: InputArea::new(),
            search_bar: SearchBar::new(),
            search_state: SearchState::new(),
            cols,
            rows,
            zmodem: ZmodemHandler::new(),
            zmodem_native: {
                let mut session = ZmodemSession::new();
                session.set_download_dir(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
                session
            },
            zmodem_current_file: None,
            pending_upload: None,
            pending_download: None,
            zmodem_download_active: false,
            zmodem_download_triggered: false,
            zmodem_data_tx: None,
            sftp_client: None,
            current_dir: PathBuf::from("/"),
            is_root_user: false,
            zmodem_upload_triggered: false,
            upload_success_message: None,
            show_context_menu: false,
            context_menu_pos: egui::Pos2::ZERO,
            show_paste_confirm: false,
            pending_paste_text: String::new(),
            paste_convert_tabs: true,
            paste_remove_crlf: true,
            initial_resize_done: false,
            pending_ssh_config: None,
            zmodem_idle_frames: 0,
            ime_preedit: String::new(),
            broadcast_buffer: Vec::new(),
            scroll_accumulator: 0.0,
            session_key: None,
            history_store: None,
            completion_engine: None,
            current_suggestion: None,
            last_input_prefix: String::new(),
            show_completion_popup: false,
            completion_candidates: Vec::new(),
            completion_selected: None,
            terminal_rect_cache: egui::Rect::NOTHING,
            input_buffer: String::new(),
            prompt_end_col: 0,
            prompt_row: 0,
            awaiting_prompt_capture: true,
            fold_manager: CommandFoldManager::new(),
            show_fold_context_menu: false,
            fold_context_menu_pos: egui::Pos2::ZERO,
            fold_context_prompt_line: 0,
            process_exited: false,
            prev_interactive: false,
        }
    }

    /// Whether this panel has an active backend connection
    pub fn is_connected(&self) -> bool {
        if self.process_exited {
            return false;
        }
        self.backend.is_connected()
    }

    /// Public write interface for batch mode broadcast
    pub fn write(&self, data: &[u8]) {
        self.backend.write(data);
    }

    /// Write to backend and record in broadcast buffer (for user keyboard input)
    fn write_and_broadcast(&mut self, data: &[u8]) {
        self.backend.write(data);
        self.broadcast_buffer.push(data.to_vec());
    }

    /// Spawn a local shell via PTY
    pub fn spawn_local_shell(&mut self) {
        let (output_tx, output_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let size = PtySize { rows: self.rows as u16, cols: self.cols as u16 };
        match PtySession::spawn(None, size, output_tx) {
            Ok(pty) => {
                self.backend = Backend::from_pty(&pty);
                self._pty = Some(pty);
                self.output_rx = Some(output_rx);
            }
            Err(e) => {
                tracing::error!("Failed to spawn PTY: {}", e);
            }
        }
        // Initialize history for local session
        let username = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".into());
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .into_owned();
        let key = format!("local://{}@{}", username, hostname);
        self.init_history(&key);
    }

    /// Spawn an SSH session
    pub fn spawn_ssh_session(&mut self, config: SshConfig, sftp_slot: Arc<tokio::sync::Mutex<Option<SftpClient>>>) {
        // Initialize history for SSH session (isolated by username@host:port)
        let key = format!("ssh://{}@{}:{}", config.username, config.host, config.port);
        tracing::info!("SSH session key for history: {}", key);
        self.init_history(&key);
        // Defer connection until after first frame render to use correct size
        self.pending_ssh_config = Some((config, sftp_slot));
    }

    /// Drain PTY/SSH output into emulator + check for SSH input_tx arrival
    pub fn drain_output(&mut self) {
        // Check if SSH input_tx has arrived
        if matches!(self.backend, Backend::None) {
            if let Ok(mut input_slot) = self.ssh_input_tx.try_lock() {
                if let Some(input_tx) = input_slot.take() {
                    if let Ok(mut resize_slot) = self.ssh_resize_tx.try_lock() {
                        if let Some(resize_tx) = resize_slot.take() {
                            tracing::info!(">>> SSH CONNECTION ESTABLISHED <<<");
                            self.backend = Backend::Ssh { input_tx, resize_tx };
                        }
                    }
                }
            }
        }

        // Sync SFTP client from slot (attempt sync every frame)
        if let Some(slot) = &self.sftp_slot {
            if let Ok(client_opt) = slot.try_lock() {
                if client_opt.is_some() && self.sftp_client.is_none() {
                    // Record only once
                    tracing::info!("SFTP client available");
                }
            }
        }

        // Drain output bytes into emulator
        if let Some(rx) = &mut self.output_rx {
            let mut total_bytes = 0;
            let mut zmodem_data_queue = Vec::new();

            loop {
                match rx.try_recv() {
                    Ok(data) => {
                total_bytes += data.len();
                tracing::debug!(">>> [MAIN LOOP] Received {} bytes from SSH", data.len());

                // Check ZMODEM active state first
                if self.zmodem_native.is_active() {
                    tracing::debug!(">>> [ZMODEM ACTIVE] Queuing for raw mode");
                    zmodem_data_queue.push(data);
                    continue;
                } else if self.zmodem_current_file.is_some() {
                    // ZMODEM ended but file handle still open; detect end marker and release
                    if data.windows(3).any(|w| w == b"**B") {
                        tracing::warn!(">>> [MAIN LOOP] Detected **B end marker, finishing file");
                        if let Some(file) = self.zmodem_current_file.take() {
                            match file.finish() {
                                Ok(path) => {
                                    tracing::warn!(">>> [MAIN LOOP] File finished: {:?}", path);
                                }
                                Err(e) => {
                                    tracing::error!(">>> [MAIN LOOP] Failed to finish file: {}", e);
                                }
                            }
                        }
                    }
                }

                // Detect ZMODEM command
                if let Some(mode) = self.zmodem.detect(&data) {
                    tracing::info!(">>> ZMODEM detected: {:?}", mode);

                    match mode {
                        ZmodemMode::Send => {
                            tracing::info!(">>> [SZ DETECTED] Starting zmodem2 receive immediately");
                            self.zmodem_native.start_receive();

                            // Send initialization frame immediately
                            if let Some(outgoing) = self.zmodem_native.get_outgoing() {
                                let init_frame = outgoing.to_vec();
                                tracing::info!(">>> [SZ] Sending {} bytes init frame", init_frame.len());
                                self.backend.write(&init_frame);
                                self.zmodem_native.advance_outgoing(init_frame.len());
                            }
                        }
                        ZmodemMode::Receive => {
                            tracing::info!(">>> [RZ DETECTED] upload_triggered={}", self.zmodem_upload_triggered);
                            if !self.zmodem_upload_triggered {
                                if self.pending_upload.is_none() {
                                    tracing::info!(">>> [RZ] Opening file picker");
                                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                                        tracing::info!(">>> [RZ] User selected: {:?}", path);
                                        self.pending_upload = Some(path);
                                    }
                                }
                            }
                        }
                    }
                }

                // Detect ZMODEM end marker (before displaying to terminal)
                if self.zmodem_current_file.is_some() {
                    tracing::warn!(">>> [MAIN LOOP] Checking for **B in {} bytes: {:?}",
                        data.len(), &data[..data.len().min(30)]);
                    if data.windows(3).any(|w| w == b"**B") {
                        tracing::warn!(">>> [MAIN LOOP] **B detected, finishing file NOW");
                        if let Some(file) = self.zmodem_current_file.take() {
                            match file.finish() {
                                Ok(path) => {
                                    tracing::warn!(">>> [MAIN LOOP] File FINISHED: {:?}", path);
                                }
                                Err(e) => {
                                    tracing::error!(">>> [MAIN LOOP] Failed to finish: {}", e);
                                }
                            }
                        }
                        self.zmodem_native.reset();
                    }
                }

                // Normal terminal data, display to terminal
                self.emulator.process(&data);
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        if !self.process_exited {
                            tracing::info!("Terminal process exited (channel disconnected)");
                            // Force exit alt-screen so fold lines and history resume
                            self.emulator.exit_alt_screen();
                            self.emulator.clear_interactive_child();
                            self.process_exited = true;
                        }
                        break;
                    }
                }
            }

            // Process ZMODEM data queue
            for data in zmodem_data_queue {
                self.handle_zmodem_raw_data(&data);
            }

            if total_bytes > 1000 {
                tracing::debug!("Processed {} bytes, total_rows={}, scrollback={}",
                    total_bytes, self.emulator.total_rows(), self.emulator.scrollback.len());
            }
        }

        // Check idle state regardless of whether ZMODEM is active
        if self.zmodem_current_file.is_some() {
            self.drain_zmodem_file();
        }

        // Process pending upload files (rz mode)
        if let Some(file_path) = self.pending_upload.take() {
            tracing::info!(">>> [UPLOAD] Processing file: {:?}", file_path);
            let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
            tracing::info!(">>> [UPLOAD] File name: {}", file_name);

            if let Ok(mut child) = self.zmodem.bridge_upload(vec![file_path.clone()]) {
                tracing::info!(">>> [UPLOAD] sz process started");
                let backend = self.backend.clone();
                let stdout = child.stdout.take();
                let file_name_for_msg = file_name.clone();

                // sz stdout -> SSH channel
                if let Some(mut out) = stdout {
                    tracing::info!(">>> [UPLOAD] Starting stdout->SSH task");
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 4096];
                        let mut total_bytes = 0;
                        loop {
                            match tokio::io::AsyncReadExt::read(&mut out, &mut buf).await {
                                Ok(0) => {
                                    tracing::info!(">>> [UPLOAD] sz EOF, total: {} bytes", total_bytes);
                                    break;
                                }
                                Ok(n) => {
                                    total_bytes += n;
                                    tracing::info!(">>> [UPLOAD] sz->SSH {} bytes (total: {})", n, total_bytes);
                                    backend.write(&buf[..n]);
                                }
                                Err(e) => {
                                    tracing::error!(">>> [UPLOAD] Read error: {}", e);
                                    break;
                                }
                            }
                        }
                        tracing::info!(">>> [UPLOAD] Completed: {}", file_name_for_msg);
                    });
                } else {
                    tracing::error!(">>> [UPLOAD] stdout is None!");
                }

                // Wait for process to exit
                tokio::spawn(async move {
                    let status = child.wait().await;
                    tracing::info!(">>> [UPLOAD] sz process exited: {:?}", status);
                });

                self.upload_success_message = Some(format!("{}: {}", t("terminal.upload_success"), file_name));
                self.zmodem_upload_triggered = false;
                tracing::info!(">>> [UPLOAD] Success message set, triggered reset to false");
            } else {
                tracing::error!(">>> [UPLOAD] Failed to start sz process");
            }
        }

        // Process pending download files (sz mode) - moved to start_zmodem_download method
    }

    fn handle_zmodem_raw_data(&mut self, data: &[u8]) {
        tracing::info!(">>> [ZMODEM RAW] Received {} bytes from SSH", data.len());

        // 1. Simply append data into rx_buf
        if let Err(e) = self.zmodem_native.handle_data(data) {
            tracing::error!(">>> [ZMODEM RAW] Error: {:?}", e);
            return;
        }

        // 2. Core loop: keep consuming rx_buf until no more progress
        loop {
            let mut made_progress = false;

            // A. Try to let zmodem2 consume some of its internal rx_buf
            match self.zmodem_native.pump_rx_buf() {
                Ok(consumed) if consumed > 0 => {
                    made_progress = true;
                }
                Err(e) => {
                    tracing::error!(">>> [ZMODEM FEED] Error: {:?}", e);
                    break;
                }
                _ => {}
            }

            // B. Process events
            if self.process_zmodem_events() {
                made_progress = true;
            }

            // C. Write file to disk
            if self.drain_zmodem_file() {
                made_progress = true;
            }

            // D. Send ACK response to SSH
            if self.drain_zmodem_outgoing() {
                made_progress = true;
            }

            // If none of the four steps above made progress, all current data has been processed
            if !made_progress {
                break;
            }
        }
    }

    fn process_zmodem_events(&mut self) -> bool {
        let mut progress = false;
        while let Some(event) = self.zmodem_native.poll_receiver_event() {
            progress = true;
            tracing::info!(">>> [ZMODEM EVENT] {:?}", event);
            match event {
                ReceiverEvent::FileStart => {
                    if let Some(name) = self.zmodem_native.get_file_name() {
                        tracing::info!(">>> [ZMODEM EVENT] FileStart, name: {}", name);

                        if let Some(path) = rfd::FileDialog::new()
                            .set_title(t("terminal.save_file"))
                            .set_file_name(&name)
                            .save_file()
                        {
                            tracing::info!(">>> [ZMODEM EVENT] User selected: {:?}", path);
                            match ActiveFile::new(path) {
                                Ok(f) => {
                                    self.zmodem_current_file = Some(f);
                                    tracing::info!(">>> [ZMODEM EVENT] ActiveFile created");
                                }
                                Err(e) => {
                                    tracing::error!(">>> [ZMODEM EVENT] Failed to create file: {}", e);
                                }
                            }
                        } else {
                            tracing::info!(">>> [ZMODEM EVENT] User cancelled");
                        }
                    }
                }
                ReceiverEvent::FileComplete => {
                    tracing::warn!(">>> [ZMODEM EVENT] FileComplete - finishing file");
                    if let Some(file) = self.zmodem_current_file.take() {
                        match file.finish() {
                            Ok(path) => {
                                tracing::warn!(">>> [ZMODEM EVENT] File finished: {:?}", path);
                            }
                            Err(e) => {
                                tracing::error!(">>> [ZMODEM EVENT] Failed to finish file: {}", e);
                            }
                        }
                    }
                }
                ReceiverEvent::SessionComplete => {
                    tracing::warn!(">>> [ZMODEM EVENT] SessionComplete - finishing file and resetting");
                    if let Some(file) = self.zmodem_current_file.take() {
                        match file.finish() {
                            Ok(path) => {
                                tracing::warn!(">>> [ZMODEM EVENT] File finished: {:?}", path);
                            }
                            Err(e) => {
                                tracing::error!(">>> [ZMODEM EVENT] Failed to finish file: {}", e);
                            }
                        }
                    }

                    // Send cancel sequence to terminate sz process
                    tracing::warn!(">>> [ZMODEM EVENT] Sending cancel sequence to terminate sz");
                    self.backend.write(b"\x18\x18\x18\x18\x18\x18\x18\x18");

                    self.zmodem_native.reset();
                    tracing::warn!(">>> [ZMODEM EVENT] Session reset");
                }
            }
        }
        progress
    }

    fn drain_zmodem_file(&mut self) -> bool {
        let mut progress = false;
        loop {
            let file_data = self.zmodem_native.drain_receiver_file();
            if file_data.is_empty() {
                break;
            }

            progress = true;
            let data_len = file_data.len();
            if let Some(file) = &mut self.zmodem_current_file {
                tracing::info!(">>> [ZMODEM DRAIN] Writing {} bytes to file", data_len);
                if let Err(e) = file.write_chunk(file_data) {
                    tracing::error!(">>> [ZMODEM DRAIN] Write error: {}", e);
                    break;
                }
            }
            let _ = self.zmodem_native.advance_receiver_file(data_len);

            self.zmodem_idle_frames = 0;
        }
        progress
    }

    fn drain_zmodem_outgoing(&mut self) -> bool {
        let mut progress = false;
        loop {
            if let Some(outgoing) = self.zmodem_native.get_outgoing() {
                progress = true;
                let len = outgoing.len();
                tracing::info!(">>> [ZMODEM ACK] Sending {} bytes to SSH", len);
                self.backend.write(outgoing);
                self.zmodem_native.advance_outgoing(len);
            } else {
                break;
            }
        }
        progress
    }

    /// Search terminal content using the current search bar query
    fn perform_search(&mut self) {
        self.search_state.query = self.search_bar.query.clone();
        self.search_state.use_regex = self.search_bar.use_regex;
        self.search_state.case_sensitive = self.search_bar.case_sensitive;
        self.search_state.results.clear();
        self.search_state.current_result = 0;

        if let Some(re) = self.search_state.compile_regex() {
            let total = self.emulator.total_rows();
            for row_idx in 0..total {
                if let Some(row) = self.emulator.get_row(row_idx) {
                    let line: String = row.cells.iter().map(|c| c.ch).collect();
                    for m in re.find_iter(&line) {
                        self.search_state.results.push(SearchResult {
                            buffer_row: row_idx,
                            col_start: m.start(),
                            col_end: m.end(),
                        });
                    }
                }
            }
        }

        // Update search bar display
        self.search_bar.result_count = self.search_state.results.len();
        self.search_bar.current_result = self.search_state.current_result;
    }

    /// Scroll the emulator to show the current search result
    fn scroll_to_current_result(&mut self) {
        if let Some(result) = self.search_state.current() {
            let target_row = result.buffer_row;
            let total = self.emulator.total_rows();
            let grid_rows = self.emulator.grid.rows;
            let bottom = total.saturating_sub(grid_rows);
            if target_row < bottom {
                self.emulator.scroll_offset = bottom - target_row + grid_rows / 2;
                let max_offset = self.emulator.scrollback.len();
                if self.emulator.scroll_offset > max_offset {
                    self.emulator.scroll_offset = max_offset;
                }
            } else {
                self.emulator.scroll_offset = 0;
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, theme: &Theme, other_window_open: bool) {
        self.drain_output();

        // On first frame render, if there is a pending SSH config, connect with correct size
        if let Some((config, sftp_slot)) = self.pending_ssh_config.take() {
            let panel_width = ui.available_width();
            let panel_height = ui.available_height();
            let cols = ((panel_width - 8.0) / self.view.cell_width).floor() as usize;
            let rows = (panel_height / self.view.cell_height).floor() as usize;

            if cols > 0 && rows > 0 {
                self.cols = cols;
                self.rows = rows;
                self.emulator.resize(cols, rows);

                tracing::info!("SSH connecting with correct size: {}x{}", cols, rows);

                self.sftp_slot = Some(sftp_slot.clone());
                let (output_tx, output_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                self.output_rx = Some(output_rx);
                let ssh_input_slot = self.ssh_input_tx.clone();
                let ssh_resize_slot = self.ssh_resize_tx.clone();
                let (sftp_tx, mut sftp_rx) = mpsc::unbounded_channel();

                tokio::spawn(async move {
                    match SshClient::connect(config, output_tx, sftp_tx, cols as u16, rows as u16).await {
                        Ok((input_tx, resize_tx)) => {
                            let mut slot = ssh_input_slot.lock().await;
                            *slot = Some(input_tx);
                            let mut slot = ssh_resize_slot.lock().await;
                            *slot = Some(resize_tx);
                            tracing::info!("SSH session connected");
                        }
                        Err(e) => {
                            tracing::error!("SSH connection failed: {}", e);
                        }
                    }
                });

                tokio::spawn(async move {
                    if let Some(sftp_session) = sftp_rx.recv().await {
                        let mut slot = sftp_slot.lock().await;
                        *slot = Some(SftpClient::from_session(sftp_session));
                        tracing::info!("SFTP session stored in slot");
                    }
                });

                self.initial_resize_done = true;
            }
        }

        ui.vertical(|ui| {
            // Search bar (if open)
            let search_action = self.search_bar.show(ui, theme);
            match search_action {
                SearchAction::QueryChanged => {
                    self.perform_search();
                    if !self.search_state.results.is_empty() {
                        self.scroll_to_current_result();
                    }
                }
                SearchAction::Next => {
                    self.search_state.next_result();
                    self.search_bar.current_result = self.search_state.current_result;
                    self.scroll_to_current_result();
                }
                SearchAction::Prev => {
                    self.search_state.prev_result();
                    self.search_bar.current_result = self.search_state.current_result;
                    self.scroll_to_current_result();
                }
                SearchAction::Close => {
                    self.search_state.results.clear();
                    self.search_state.current_result = 0;
                }
                SearchAction::None => {}
            }

            // Sync from UI Theme to TerminalTheme
            self.view.theme.colors = theme.terminal_colors;
            self.view.theme.bg = [
                (theme.bg.r() as u8),
                (theme.bg.g() as u8),
                (theme.bg.b() as u8),
                255,
            ];
            self.view.theme.fg = [
                (theme.fg.r() as u8),
                (theme.fg.g() as u8),
                (theme.fg.b() as u8),
                255,
            ];
            self.view.theme.selection_color = [
                theme.selection_bg.r(),
                theme.selection_bg.g(),
                theme.selection_bg.b(),
                theme.selection_bg.a(),
            ];

            // Use horizontal layout: terminal + scrollbar
            let panel_height = ui.available_height();
            let panel_width = ui.available_width();

            let mut zoom_changed = false;

            tracing::info!("=== TerminalPanel START: panel_height={} ===", panel_height);
            let resp_outer = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0; // no gap between terminal and scrollbar
                // Terminal area - allocate fixed size directly
                let terminal_width = panel_width - 8.0;
                let terminal_size = egui::vec2(terminal_width, panel_height);
                tracing::info!("TerminalPanel: allocating terminal_size={:?}", terminal_size);

                let (terminal_rect, terminal_resp) = ui.allocate_exact_size(terminal_size, Sense::click());
                tracing::info!("TerminalPanel: allocated terminal_rect={:?}", terminal_rect);

                // Render terminal in the allocated area with explicit size
                // Pass search matches to view
                self.view.search_match_cells = self.search_state.results.iter()
                    .flat_map(|r| (r.col_start..r.col_end).map(move |c| (r.buffer_row, c)))
                    .collect();
                let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(terminal_rect).layout(egui::Layout::top_down(egui::Align::LEFT)));
                tracing::info!("TerminalPanel: child_ui.available_size={:?}, child_ui.max_rect={:?}",
                    child_ui.available_size(), child_ui.max_rect());

                // Build fold render info (disabled in interactive mode)
                let is_inter = self.emulator.is_interactive();
                if is_inter != self.prev_interactive {
                    tracing::warn!(">>> INTERACTIVE TRANSITION: {} -> {}, osc133_available={}, prompt_state={:?}, alt_screen={}, interactive_child={}, finalized_rows={:?}, command_line_rows={}",
                        self.prev_interactive, is_inter, self.emulator.osc133_available, self.emulator.prompt_state, self.emulator.is_alt_screen(), self.emulator.interactive_child_active,
                        self.emulator.finalized_command_rows, self.emulator.command_line_rows.len());
                    self.prev_interactive = is_inter;
                }
                let fold_info_owned = if is_inter {
                    None
                } else {
                    let cursor_abs_row = self.emulator.scrollback.len() + self.emulator.grid.cursor_row;
                    let fold_rows = self.emulator.fold_command_rows();
                    let info = self.fold_manager.build_render_info(
                        fold_rows,
                        cursor_abs_row,
                    );
                    if info.blocks.is_empty() { None } else { Some(info) }
                };
                let fold_info_ref = fold_info_owned.as_ref();

                self.view.connected = !self.process_exited;
                let view_result = self.view.show_with_size(&mut child_ui, &mut self.emulator, Some(terminal_size), fold_info_ref);

                // Handle fold click events
                if let Some(prompt_line) = view_result.fold_left_click {
                    self.fold_manager.toggle(prompt_line);
                }
                if let Some((prompt_line, pos)) = view_result.fold_right_click {
                    self.show_fold_context_menu = true;
                    self.fold_context_menu_pos = pos;
                    self.fold_context_prompt_line = prompt_line;
                }

                // Detect right-click (fold context menu takes priority) — only when no popup is open
                if !other_window_open && !self.show_fold_context_menu && terminal_rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default()) {
                    if ui.ctx().input(|i| i.pointer.secondary_clicked()) {
                        if let Some(pos) = ui.ctx().pointer_latest_pos() {
                            tracing::info!(">>> Right-click detected at {:?}", pos);
                            self.show_context_menu = true;
                            self.context_menu_pos = pos;
                        }
                    }
                }

                // Handle mouse selection — only when no popup is open
                if !other_window_open {
                    self.view.handle_mouse_selection(ui, terminal_rect, terminal_rect.min, &mut self.emulator);
                }

                // Auto-copy selected text to clipboard on mouse release — only when no popup is open
                if !other_window_open {
                    let primary_released = ui.ctx().input(|i| i.pointer.primary_released());
                    if primary_released {
                        if let Some(text) = self.emulator.selected_text() {
                            if !text.is_empty() {
                                ui.output_mut(|o| o.commands.push(egui::OutputCommand::CopyText(text)));
                            }
                        }
                    }
                }

                // Scrollbar area
                ui.allocate_ui(egui::vec2(8.0, panel_height), |ui| {
                        ui.painter().rect_filled(ui.available_rect_before_wrap(), 0.0, egui::Color32::from_gray(230));

                        let scrollback_len = self.emulator.scrollback.len();
                        if scrollback_len > 0 {
                            let bar_height = panel_height;
                            let total_lines = self.emulator.total_rows();
                            let visible_lines = self.emulator.grid.rows;
                            let handle_height = (visible_lines as f32 / total_lines as f32 * bar_height).max(20.0);

                            let response = ui.allocate_response(egui::vec2(8.0, bar_height), egui::Sense::click_and_drag());

                            if response.dragged() {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    let y = (pos.y - response.rect.min.y).clamp(0.0, bar_height - handle_height);
                                    let ratio = y / (bar_height - handle_height);
                                    self.emulator.scroll_offset = ((1.0 - ratio) * scrollback_len as f32) as usize;
                                }
                            }

                            let painter = ui.painter();
                            let ratio = 1.0 - (self.emulator.scroll_offset as f32 / scrollback_len as f32);
                            let handle_y = response.rect.min.y + ratio * (bar_height - handle_height);
                            let handle_rect = egui::Rect::from_min_size(
                                egui::pos2(response.rect.min.x + 1.0, handle_y),
                                egui::vec2(6.0, handle_height)
                            );
                            painter.rect_filled(handle_rect, 2.0, egui::Color32::from_gray(160));
                        }
                    });

                    (terminal_rect, terminal_resp)
                }).inner;
            let (terminal_rect, resp) = resp_outer;
            self.terminal_rect_cache = terminal_rect;

            // Resize terminal when window size changes (skip during scaling)
            if !zoom_changed {
                let terminal_width = panel_width - 8.0;
                let new_cols = (terminal_width / self.view.cell_width).floor() as usize;
                let new_rows = (panel_height / self.view.cell_height).floor() as usize;

                // Force resize on first resize or when size changes
                if new_cols > 0 && new_rows > 0 && (!self.initial_resize_done || new_cols != self.cols || new_rows != self.rows) {
                    tracing::info!(">>> RESIZE: from {}x{} to {}x{}, emulator.grid before: {}x{}, total_rows={}, scrollback_len={}",
                        self.cols, self.rows, new_cols, new_rows,
                        self.emulator.grid.cols, self.emulator.grid.rows,
                        self.emulator.total_rows(), self.emulator.scrollback.len());

                    self.cols = new_cols;
                    self.rows = new_rows;
                    self.emulator.resize(self.cols, self.rows);
                    self.backend.resize(self.cols as u16, self.rows as u16);
                    self.initial_resize_done = true;

                    tracing::info!(">>> RESIZE AFTER: emulator.grid: {}x{}, total_rows={}, scrollback_len={}",
                        self.emulator.grid.cols, self.emulator.grid.rows,
                        self.emulator.total_rows(), self.emulator.scrollback.len());
                }
            } else {
                tracing::info!(">>> ZOOM: skipping resize");
            }

            // Detect file drag-and-drop
            let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
            let mut sftp_handled = false;
            if !dropped_files.is_empty() {
                tracing::info!("Terminal panel detected {} dropped files", dropped_files.len());
                if let Some(file) = dropped_files.first() {
                    if let Some(path) = &file.path {
                        // SSH connection: Windows uses SFTP, Linux/macOS uses ZMODEM
                        if matches!(self.backend, Backend::Ssh { .. }) {
                            sftp_handled = true;

                            if cfg!(windows) {
                                // Windows: use SFTP to upload
                                tracing::info!("Windows detected, using SFTP upload");
                                if let Some(slot) = self.sftp_slot.clone() {
                                    let local = path.clone();
                                    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                                    tokio::spawn(async move {
                                        if let Ok(mut client_opt) = slot.try_lock() {
                                            if let Some(client) = client_opt.as_mut() {
                                                let remote_dir = match client.realpath(".").await {
                                                    Ok(dir) => dir,
                                                    Err(e) => {
                                                        tracing::error!("Failed to get remote dir: {}", e);
                                                        return;
                                                    }
                                                };

                                                let remote_str = format!("{}/{}", remote_dir.to_string_lossy(), filename);
                                                let remote = PathBuf::from(remote_str);

                                                if let Err(e) = client.upload(&local, &remote).await {
                                                    tracing::error!("SFTP upload failed: {}", e);
                                                } else {
                                                    tracing::info!("SFTP upload success: {:?}", remote);
                                                }
                                            }
                                        }
                                    });
                                }
                            } else {
                                // Linux/macOS: use ZMODEM to upload
                                tracing::info!("Unix detected, using ZMODEM upload");
                                self.pending_upload = Some(path.clone());
                                self.zmodem_upload_triggered = true;
                                self.backend.write(b"rz\n");
                                tracing::info!("Sent 'rz' command to remote");
                            }
                        } else {
                            // Local terminal: insert file path
                            let path_str = if cfg!(windows) {
                                format!("\"{}\"", path.display())
                            } else {
                                path.display().to_string().replace(" ", "\\ ")
                            };
                            self.backend.write(path_str.as_bytes());
                        }
                    }
                }
            }

            // Auto-focus (only when no other window is open)
            if !resp.has_focus() && !other_window_open {
                resp.request_focus();
            }

            // Handle keyboard input
            if resp.has_focus() && !other_window_open {
                let events: Vec<egui::Event> = ui.ctx().input(|input| input.events.clone());

                // Record all key events
                for event in &events {
                    if let egui::Event::Key { key, pressed, modifiers, .. } = event {
                        tracing::info!("Key event: {:?} pressed={} ctrl={} shift={}",
                            key, pressed, modifiers.ctrl, modifiers.shift);
                    }
                }

                // Handle Ctrl+C first (both press and release count)
                let mut ctrl_c_found = false;
                for event in &events {
                    if let egui::Event::Key { key, pressed, modifiers, .. } = event {
                        if *key == egui::Key::C && modifiers.ctrl && !modifiers.shift {
                            ctrl_c_found = true;
                            tracing::info!("Ctrl+C event (pressed={})", pressed);
                            break;
                        }
                    }
                }

                if ctrl_c_found {
                    tracing::info!(">>> Ctrl+C detected, sending 0x03");
                    self.input_buffer.clear();
                    self.write_and_broadcast(b"\x03");
                    return;
                }

                for event in &events {
                    match event {
                        egui::Event::Key { key, pressed: true, modifiers, .. } => {
                            // Tab key - highest priority
                            if *key == egui::Key::Tab && !modifiers.ctrl && !modifiers.shift {
                                self.write_and_broadcast(b"\t");
                                continue;
                            }
                            // Ctrl+L clear screen
                            if *key == egui::Key::L && modifiers.ctrl && !modifiers.shift {
                                self.write_and_broadcast(b"\x0c");
                                continue;
                            }
                            // Ctrl+D (EOF) — may exit interactive child
                            if *key == egui::Key::D && modifiers.ctrl && !modifiers.shift {
                                self.emulator.clear_interactive_child();
                                self.write_and_broadcast(b"\x04");
                                continue;
                            }
                            // Ctrl+C interrupt
                            if *key == egui::Key::C && modifiers.ctrl && !modifiers.shift {
                                tracing::debug!("Ctrl+C pressed, sending 0x03");
                                self.emulator.clear_interactive_child();
                                self.input_buffer.clear();
                                self.write_and_broadcast(b"\x03");
                                continue;
                            }
                            // Ctrl+Shift+C copy
                            if *key == egui::Key::C && modifiers.ctrl && modifiers.shift {
                                if let Some(text) = self.emulator.selected_text() {
                                    ui.output_mut(|o| o.commands.push(egui::OutputCommand::CopyText(text)));
                                }
                                continue;
                            }
                            if *key == egui::Key::V && modifiers.ctrl && modifiers.shift {
                                continue;
                            }
                            if *key == egui::Key::F && modifiers.ctrl && modifiers.shift {
                                self.search_bar.open();
                                continue;
                            }
                            // PageUp/PageDown: scroll terminal view (Shift+PageUp/Down sends to shell)
                            if *key == egui::Key::PageUp && !modifiers.shift {
                                let page = self.emulator.grid.rows.max(1);
                                let max_offset = self.emulator.scrollback.len();
                                self.emulator.scroll_offset = (self.emulator.scroll_offset + page).min(max_offset);
                                continue;
                            }
                            if *key == egui::Key::PageDown && !modifiers.shift {
                                let page = self.emulator.grid.rows.max(1);
                                self.emulator.scroll_offset = self.emulator.scroll_offset.saturating_sub(page);
                                continue;
                            }
                            // Arrow keys: scroll back to cursor row first, then send to shell
                            if matches!(key, egui::Key::ArrowUp | egui::Key::ArrowDown | egui::Key::ArrowLeft | egui::Key::ArrowRight) {
                                self.emulator.scroll_offset = 0;
                            }
                            self.handle_key(*key);
                        }
                        egui::Event::Text(text) => {
                            // IME confirmation also produces Text events, handle normally
                            // Filter out Tab and Ctrl+C since they are already handled in Key events
                            if text == "\x03" {
                                tracing::debug!("Filtered Ctrl+C from Text event");
                            } else if text != "\t" {
                                // Interactive mode: just send, don't track input
                                if !self.emulator.is_interactive() {
                                    // Record prompt end position on first keypress
                                    if self.awaiting_prompt_capture {
                                        self.prompt_end_col = self.emulator.grid.cursor_col;
                                        self.prompt_row = self.emulator.grid.cursor_row;
                                        self.awaiting_prompt_capture = false;
                                    }
                                    self.input_buffer.push_str(text);
                                }
                                // Auto-scroll to bottom to show cursor on user input
                                self.emulator.scroll_offset = 0;
                                self.write_and_broadcast(text.as_bytes());
                            }
                        }
                        egui::Event::Ime(ime_event) => {
                            match ime_event {
                                egui::ImeEvent::Preedit(text) => {
                                    self.ime_preedit = text.clone();
                                }
                                egui::ImeEvent::Commit(text) => {
                                    self.ime_preedit.clear();
                                    if !self.emulator.is_interactive() {
                                        if self.awaiting_prompt_capture {
                                            self.prompt_end_col = self.emulator.grid.cursor_col;
                                            self.prompt_row = self.emulator.grid.cursor_row;
                                            self.awaiting_prompt_capture = false;
                                        }
                                        self.input_buffer.push_str(text);
                                    }
                                    self.emulator.scroll_offset = 0;
                                    self.write_and_broadcast(text.as_bytes());
                                }
                                egui::ImeEvent::Enabled => {
                                    self.ime_preedit.clear();
                                }
                                egui::ImeEvent::Disabled => {
                                    self.ime_preedit.clear();
                                }
                            }
                        }
                        egui::Event::Paste(text) => {
                            self.handle_paste(text.clone());
                        }
                        _ => {}
                    }
                }
            }

            // Always sync input_buffer from screen when prompt position is known.
            // This ensures Tab completion, shell-side edits, etc. are reflected.
            if !self.emulator.is_interactive() && !self.awaiting_prompt_capture {
                let screen_input = self.read_screen_input();
                if !screen_input.is_empty() || !self.input_buffer.is_empty() {
                    self.input_buffer = screen_input;
                }
            }

            // Refresh history suggestions and sync to view (disabled in interactive mode)
            if self.emulator.is_interactive() {
                self.current_suggestion = None;
                self.show_completion_popup = false;
                self.completion_candidates.clear();
                self.view.suggestion = None;
            } else {
                self.refresh_suggestion();
                self.view.suggestion = self.current_suggestion.clone();
            }

            // Set IME cursor position so the input method candidate window follows the terminal cursor
            {
                let cursor_x = terminal_rect.min.x + self.emulator.grid.cursor_col as f32 * self.view.cell_width;
                let cursor_y = terminal_rect.min.y + self.emulator.grid.cursor_row as f32 * self.view.cell_height;
                let cursor_rect = egui::Rect::from_min_size(
                    egui::pos2(cursor_x, cursor_y),
                    egui::vec2(1.0, self.view.cell_height),
                );
                ui.output_mut(|o| {
                    o.ime = Some(egui::output::IMEOutput {
                        rect: terminal_rect,
                        cursor_rect,
                    });
                });
            }

            // Render IME pre-edit text (input method candidate characters)
            if !self.ime_preedit.is_empty() {
                let cursor_x = terminal_rect.min.x + self.emulator.grid.cursor_col as f32 * self.view.cell_width;
                let cursor_y = terminal_rect.min.y + self.emulator.grid.cursor_row as f32 * self.view.cell_height;
                let preedit_rect = egui::Rect::from_min_size(
                    egui::pos2(cursor_x, cursor_y),
                    egui::vec2(self.ime_preedit.len() as f32 * self.view.cell_width, self.view.cell_height),
                );
                let painter = ui.painter();
                painter.rect_filled(preedit_rect, 0.0, egui::Color32::from_rgba_unmultiplied(60, 60, 80, 220));
                painter.text(
                    egui::pos2(cursor_x, cursor_y),
                    egui::Align2::LEFT_TOP,
                    &self.ime_preedit,
                    egui::FontId::monospace(self.view.font_size),
                    egui::Color32::from_rgb(255, 200, 50),
                );
            }
            // Zoom and scroll — use actual rendered terminal_rect to check if mouse is in this panel
            let pointer_over_me = ui.ctx().pointer_latest_pos()
                .map_or(false, |pos| terminal_rect.contains(pos));
            if pointer_over_me {
                ui.input(|i| {
                    if i.modifiers.ctrl && i.raw_scroll_delta.y != 0.0 {
                        // Ctrl+scroll: zoom
                        let raw = self.view.font_size + i.raw_scroll_delta.y * 0.05;
                        self.view.font_size = raw.round().clamp(8.0, 32.0);
                        self.view.cell_width = (self.view.font_size * 0.6).round();
                        self.view.cell_height = (self.view.font_size * 1.2).round();
                        zoom_changed = true;

                        let terminal_width = panel_width - 8.0;
                        let new_cols = (terminal_width / self.view.cell_width).floor() as usize;
                        let new_rows = (panel_height / self.view.cell_height).floor() as usize;
                        if new_cols > 0 && new_rows > 0 {
                            self.cols = new_cols;
                            self.rows = new_rows;
                            self.emulator.resize(self.cols, self.rows);
                            self.backend.resize(self.cols as u16, self.rows as u16);
                        }
                        self.emulator.scroll_offset = 0;
                    } else if !i.modifiers.ctrl && i.raw_scroll_delta.y != 0.0 {
                        // Normal scroll wheel: scroll
                        // raw_scroll_delta.y > 0 = scroll up (view history)
                        // macOS natural scrolling direction is opposite to Windows/Linux
                        let delta = if cfg!(target_os = "macos") {
                            -i.raw_scroll_delta.y
                        } else {
                            i.raw_scroll_delta.y
                        };
                        self.scroll_accumulator += delta;
                    }
                });
            }
            // Scroll one line per 20 accumulated pixels (platform-independent fixed threshold)
            let scroll_step = 20.0_f32;
            if self.scroll_accumulator.abs() >= scroll_step {
                let lines = (self.scroll_accumulator / scroll_step) as i32;
                if lines != 0 {
                    self.emulator.scroll_by(lines);
                    self.scroll_accumulator -= lines as f32 * scroll_step;
                }
            }

            // File drag and drop - only write path if SFTP has not handled it
            if !sftp_handled {
                ui.ctx().input(|i| {
                    if !i.raw.dropped_files.is_empty() {
                        for file in &i.raw.dropped_files {
                            if let Some(path) = &file.path {
                                let path_str = path.to_string_lossy();
                                self.backend.write(path_str.as_bytes());
                                self.backend.write(b" ");
                            }
                        }
                    }
                });
            }
        });

        // Upload success notification
        if let Some(message) = self.upload_success_message.clone() {
            egui::Window::new(t("terminal.upload_success_title"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, -100.0])
                .show(ui.ctx(), |ui| {
                    ui.label(&message);
                    ui.add_space(10.0);
                    if ui.button(t("terminal.ok")).clicked() {
                        self.upload_success_message = None;
                    }
                });
        }

        // Paste confirmation dialog
        if self.show_paste_confirm {
            // Enter key confirms paste
            let enter_pressed = ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
            let esc_pressed = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));

            if enter_pressed {
                self.do_paste();
                self.show_paste_confirm = false;
                self.pending_paste_text.clear();
            } else if esc_pressed {
                self.show_paste_confirm = false;
                self.pending_paste_text.clear();
            }

            if self.show_paste_confirm {
                egui::Window::new(t("terminal.paste_confirm_title"))
                    .collapsible(false)
                    .resizable(false)
                    .default_width(520.0)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        let line_count = self.pending_paste_text.lines().count();
                        ui.label(tf("terminal.paste_confirm_msg", &[&line_count.to_string()]));
                        ui.add_space(10.0);

                        ui.checkbox(&mut self.paste_convert_tabs, t("terminal.convert_tabs"));
                        ui.checkbox(&mut self.paste_remove_crlf, t("terminal.remove_crlf"));

                        ui.add_space(8.0);

                        // Preview of converted content
                        let mut preview = self.pending_paste_text.clone();
                        if self.paste_convert_tabs {
                            preview = preview.replace('\t', "    ");
                        }
                        if self.paste_remove_crlf {
                            preview = preview.replace("\r\n", "\n");
                        }
                        ui.label(egui::RichText::new(t("terminal.paste_preview")).strong());
                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut preview.as_str())
                                        .desired_width(f32::INFINITY)
                                        .font(egui::TextStyle::Monospace)
                                );
                            });

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button(t("terminal.paste_confirm")).clicked() {
                                self.do_paste();
                                self.show_paste_confirm = false;
                                self.pending_paste_text.clear();
                            }
                            if ui.button(t("terminal.paste_cancel")).clicked() {
                                self.show_paste_confirm = false;
                                self.pending_paste_text.clear();
                            }
                            ui.label(egui::RichText::new(t("terminal.paste_enter_hint")).weak().small());
                        });
                    });
            }
        }

        // Context menu (drawn at outermost layer to avoid being covered)
        if self.show_context_menu {
            let has_selection = self.emulator.selection.is_some();

            egui::Area::new("terminal_context_menu".into())
                .fixed_pos(self.context_menu_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        // Match the built-in context_menu style: no background on inactive buttons
                        let style = ui.style_mut();
                        let transparent = egui::Color32::TRANSPARENT;
                        style.visuals.widgets.inactive.weak_bg_fill = transparent;
                        style.visuals.widgets.inactive.bg_fill = transparent;

                        ui.set_min_width(150.0);

                        if ui.add_enabled(has_selection, egui::Button::new(t("terminal.ctx_copy"))).clicked() {
                            if let Some(text) = self.emulator.selected_text() {
                                ui.output_mut(|o| o.commands.push(egui::OutputCommand::CopyText(text)));
                            }
                            self.show_context_menu = false;
                        }

                        if ui.button(t("terminal.ctx_paste")).clicked() {
                            // Read from system clipboard
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                if let Ok(text) = clipboard.get_text() {
                                    self.handle_paste(text);
                                }
                            }
                            self.show_context_menu = false;
                        }

                        if ui.button(t("terminal.ctx_select_all")).clicked() {
                            let total = self.emulator.total_rows();
                            if total > 0 {
                                self.emulator.start_selection(0, 0, SelectionMode::Character);
                                if let Some(last_row) = self.emulator.get_row(total - 1) {
                                    self.emulator.extend_selection(total - 1, last_row.cells.len().saturating_sub(1));
                                }
                            }
                            self.show_context_menu = false;
                        }

                        if ui.button(t("terminal.ctx_clear")).clicked() {
                            self.backend.write(b"\x0c");
                            self.show_context_menu = false;
                        }

                        ui.separator();

                        if ui.button(t("terminal.ctx_jump_prev_cmd")).clicked() {
                            if let Some(row) = self.emulator.find_previous_prompt() {
                                self.emulator.scroll_to_row(row);
                            }
                            self.show_context_menu = false;
                        }
                    });
                });

            if ui.ctx().input(|i| i.pointer.primary_clicked()) {
                self.show_context_menu = false;
            }
        }

        // Fold context menu
        if self.show_fold_context_menu {
            egui::Area::new("fold_context_menu".into())
                .fixed_pos(self.fold_context_menu_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(150.0);

                        let prompt_line = self.fold_context_prompt_line;
                        let is_collapsed = self.fold_manager.is_collapsed(prompt_line);

                        if ui.button(if is_collapsed { t("terminal.ctx_expand") } else { t("terminal.ctx_collapse") }).clicked() {
                            self.fold_manager.toggle(prompt_line);
                            self.show_fold_context_menu = false;
                        }

                        if ui.button(t("terminal.ctx_copy_output")).clicked() {
                            let cursor_abs = self.emulator.scrollback.len() + self.emulator.grid.cursor_row;
                            let blocks = CommandFoldManager::build_blocks(
                                &self.emulator.command_line_rows,
                                cursor_abs,
                            );
                            if let Some(block) = blocks.iter().find(|b| b.prompt_line == prompt_line) {
                                let mut output = String::new();
                                for row_idx in block.output_start..=block.output_end {
                                    if let Some(row) = self.emulator.get_row(row_idx) {
                                        let line: String = row.cells.iter()
                                            .filter(|c| !c.wide_placeholder)
                                            .map(|c| c.ch)
                                            .collect();
                                        if !output.is_empty() { output.push('\n'); }
                                        output.push_str(line.trim_end());
                                    }
                                }
                                if !output.is_empty() {
                                    ui.output_mut(|o| o.commands.push(egui::OutputCommand::CopyText(output)));
                                }
                            }
                            self.show_fold_context_menu = false;
                        }

                        ui.separator();

                        if ui.button(t("terminal.ctx_collapse_all")).clicked() {
                            let cursor_abs = self.emulator.scrollback.len() + self.emulator.grid.cursor_row;
                            let blocks = CommandFoldManager::build_blocks(
                                &self.emulator.command_line_rows,
                                cursor_abs,
                            );
                            self.fold_manager.collapse_all(&blocks);
                            self.show_fold_context_menu = false;
                        }

                        if ui.button(t("terminal.ctx_expand_all")).clicked() {
                            self.fold_manager.expand_all();
                            self.show_fold_context_menu = false;
                        }
                    });
                });

            if ui.ctx().input(|i| i.pointer.primary_clicked()) {
                self.show_fold_context_menu = false;
            }
        }

        // Completion dropdown list
        if self.show_completion_popup && !self.completion_candidates.is_empty() {
            let cursor_x = self.terminal_rect_cache.min.x
                + self.emulator.grid.cursor_col as f32 * self.view.cell_width;
            // Top y coordinate of cursor row
            let cursor_top_y = self.terminal_rect_cache.min.y
                + self.emulator.grid.cursor_row as f32 * self.view.cell_height;
            // Bottom y coordinate of cursor row
            let cursor_bottom_y = cursor_top_y + self.view.cell_height;

            // Estimate popup height: header row + separator + candidate entries + margin
            let item_h = self.view.cell_height + 4.0;
            let header_h = self.view.cell_height + 6.0; // header + separator
            let popup_h = header_h + item_h * self.completion_candidates.len() as f32 + 12.0;

            // If there is not enough space below the cursor, show popup above the cursor row
            let space_below = self.terminal_rect_cache.max.y - cursor_bottom_y;
            let popup_y = if space_below < popup_h && cursor_top_y - self.terminal_rect_cache.min.y > space_below {
                // Show above: popup bottom aligns with cursor row top
                (cursor_top_y - popup_h).max(self.terminal_rect_cache.min.y)
            } else {
                // Show below: popup top aligns with cursor row bottom
                cursor_bottom_y
            };

            let popup_id = egui::Id::new("completion_popup");
            let area_resp = egui::Area::new(popup_id)
                .fixed_pos(egui::pos2(cursor_x, popup_y))
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    // Dark cyan background, 20% darker
                    let popup_bg = egui::Color32::from_rgb(0, 51, 51);
                    // Selected entry: orange tint 80%
                    let selected_bg = egui::Color32::from_rgb(255, 200, 130);
                    // Hover background: slightly brighter dark cyan
                    let hover_bg = egui::Color32::from_rgb(0, 77, 77);

                    egui::Frame::none()
                        .fill(popup_bg)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 100, 100)))
                        .rounding(egui::Rounding::same(6))
                        .inner_margin(egui::Margin::same(4))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 2],
                            blur: 8,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(100),
                        })
                        .show(ui, |ui| {
                            ui.set_min_width(280.0);
                            ui.set_max_width(500.0);

                            // Show session identifier (user@host)
                            if let Some(ref key) = self.session_key {
                                let label = key.replace("ssh://", "").replace("local://", "");
                                ui.horizontal(|ui| {
                                    let font = egui::FontId::monospace(self.view.font_size * 0.75);
                                    ui.painter().text(
                                        ui.cursor().min,
                                        egui::Align2::LEFT_TOP,
                                        format!("  {}", label),
                                        font,
                                        egui::Color32::from_rgb(120, 200, 200),
                                    );
                                    ui.allocate_space(egui::vec2(ui.available_width(), self.view.cell_height));
                                });
                                ui.separator();
                            }

                            let prefix = self.last_input_prefix.clone();
                            let mut clicked_idx: Option<usize> = None;

                            for (i, candidate) in self.completion_candidates.iter().enumerate() {
                                let is_selected = self.completion_selected == Some(i);

                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), self.view.cell_height + 4.0),
                                    Sense::click(),
                                );

                                if resp.clicked() {
                                    clicked_idx = Some(i);
                                }
                                if is_selected {
                                    ui.painter().rect_filled(rect, 4.0, selected_bg);
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(rect, 4.0, hover_bg);
                                }

                                // Selected entry uses black text, others use white
                                let text_color = if is_selected {
                                    egui::Color32::BLACK
                                } else {
                                    egui::Color32::WHITE
                                };
                                let highlight_color = if is_selected {
                                    egui::Color32::from_rgb(180, 80, 0) // highlight color for selected: dark orange
                                } else {
                                    egui::Color32::from_rgb(100, 220, 220) // highlight color for unselected: bright cyan
                                };

                                // Draw command text with matched prefix highlighted
                                let font = egui::FontId::monospace(self.view.font_size * 0.9);
                                let text_y = rect.min.y + 2.0;

                                if let Some(match_end) = candidate.find(&*prefix) {
                                    let before = &candidate[..match_end];
                                    let matched = &candidate[match_end..match_end + prefix.len()];
                                    let after = &candidate[match_end + prefix.len()..];

                                    let mut x = rect.min.x + 8.0;

                                    // Part before the prefix
                                    if !before.is_empty() {
                                        let galley = ui.painter().layout_no_wrap(
                                            before.to_string(), font.clone(), text_color,
                                        );
                                        ui.painter().galley(egui::pos2(x, text_y), galley.clone(), text_color);
                                        x += galley.size().x;
                                    }

                                    // Matched prefix part — highlighted
                                    if !matched.is_empty() {
                                        let galley = ui.painter().layout_no_wrap(
                                            matched.to_string(), font.clone(), highlight_color,
                                        );
                                        ui.painter().galley(egui::pos2(x, text_y), galley.clone(), highlight_color);
                                        x += galley.size().x;
                                    }

                                    // Part after the match
                                    if !after.is_empty() {
                                        let galley = ui.painter().layout_no_wrap(
                                            after.to_string(), font.clone(), text_color,
                                        );
                                        ui.painter().galley(egui::pos2(x, text_y), galley, text_color);
                                    }
                                } else {
                                    // fuzzy match but no exact substring match, display directly
                                    let galley = ui.painter().layout_no_wrap(
                                        candidate.clone(), font, text_color,
                                    );
                                    ui.painter().galley(egui::pos2(rect.min.x + 8.0, text_y), galley, text_color);
                                }
                            }

                            clicked_idx
                        }).inner
                });

            // Handle click selection
            if let Some(idx) = area_resp.inner {
                self.completion_selected = Some(idx);
                self.accept_completion();
            }

            // Close when clicking outside the list
            let popup_rect = area_resp.response.rect;
            if ui.ctx().input(|i| i.pointer.primary_clicked()) {
                if let Some(pos) = ui.ctx().pointer_latest_pos() {
                    if !popup_rect.contains(pos) {
                        self.show_completion_popup = false;
                    }
                }
            }
        }
    }

    fn handle_paste(&mut self, text: String) {
        if text.contains('\n') {
            self.pending_paste_text = text;
            self.show_paste_confirm = true;
        } else {
            if !self.emulator.is_interactive() {
                self.input_buffer.push_str(&text);
            }
            if self.emulator.bracketed_paste {
                self.write_and_broadcast(b"\x1b[200~");
                self.write_and_broadcast(text.as_bytes());
                self.write_and_broadcast(b"\x1b[201~");
            } else {
                self.write_and_broadcast(text.as_bytes());
            }
        }
    }

    fn do_paste(&mut self) {
        let mut text = self.pending_paste_text.clone();
        if self.paste_convert_tabs {
            text = text.replace('\t', "    ");
        }
        if self.paste_remove_crlf {
            text = text.replace("\r\n", "\n");
        }
        if self.emulator.bracketed_paste {
            self.write_and_broadcast(b"\x1b[200~");
            self.write_and_broadcast(text.as_bytes());
            self.write_and_broadcast(b"\x1b[201~");
        } else {
            self.write_and_broadcast(text.as_bytes());
        }
    }

    fn handle_key(&mut self, key: egui::Key) {
        // Intercept navigation keys when dropdown is visible
        if self.show_completion_popup && !self.completion_candidates.is_empty() {
            match key {
                egui::Key::ArrowUp => {
                    let cur = self.completion_selected.unwrap_or(0);
                    self.completion_selected = Some(if cur > 0 { cur - 1 } else { 0 });
                    return;
                }
                egui::Key::ArrowDown => {
                    let cur = self.completion_selected.map(|i| i + 1).unwrap_or(0);
                    let max = self.completion_candidates.len().saturating_sub(1);
                    self.completion_selected = Some(cur.min(max));
                    return;
                }
                egui::Key::Tab => {
                    self.accept_completion();
                    return;
                }
                egui::Key::Enter => {
                    if self.completion_selected.is_some() {
                        // User actively selected a candidate, accept completion
                        self.accept_completion();
                        return;
                    }
                    // User did not select, close list and execute Enter normally
                    self.dismiss_completion();
                    // Do not return; continue to normal Enter handling below
                }
                egui::Key::Escape => {
                    self.dismiss_completion();
                    return;
                }
                _ => {
                    // Other keys: close list and handle normally
                }
            }
        }

        let bytes: Option<&[u8]> = match key {
            egui::Key::Enter => {
                // In interactive mode (alt-screen or interactive child), send Enter directly without recording history
                if self.emulator.is_interactive() {
                    Some(b"\r" as &[u8])
                } else {
                // Read actual command from terminal screen (includes Tab-completed content)
                let cmd = if !self.awaiting_prompt_capture {
                    // Prompt position recorded, read from screen
                    let row_idx = self.prompt_row;
                    let start_col = self.prompt_end_col;
                    let cursor_col = self.emulator.grid.cursor_col;
                    let cursor_row = self.emulator.grid.cursor_row;

                    let mut screen_cmd = String::new();
                    if cursor_row == row_idx {
                        // Single-line command: from prompt_end_col to cursor_col
                        if let Some(row) = self.emulator.grid.cells.get(row_idx) {
                            let end = cursor_col.min(row.cells.len());
                            for i in start_col..end {
                                if !row.cells[i].wide_placeholder {
                                    screen_cmd.push(row.cells[i].ch);
                                }
                            }
                        }
                    } else if cursor_row > row_idx {
                        // Multi-line command (spans rows)
                        for r in row_idx..=cursor_row {
                            if let Some(row) = self.emulator.grid.cells.get(r) {
                                let sc = if r == row_idx { start_col } else { 0 };
                                let ec = if r == cursor_row { cursor_col.min(row.cells.len()) } else { row.cells.len() };
                                for i in sc..ec {
                                    if !row.cells[i].wide_placeholder {
                                        screen_cmd.push(row.cells[i].ch);
                                    }
                                }
                            }
                        }
                    }
                    let trimmed = screen_cmd.trim().to_string();
                    if trimmed.is_empty() {
                        // Fall back to input_buffer if screen read fails
                        self.input_buffer.trim().to_string()
                    } else {
                        trimmed
                    }
                } else {
                    // Prompt position not captured (user may have pressed Enter directly), use input_buffer
                    self.input_buffer.trim().to_string()
                };

                if !cmd.is_empty() {
                    if let Some(engine) = &mut self.completion_engine {
                        engine.add_history(&cmd);
                    }
                    if let Some(store) = &mut self.history_store {
                        if let Err(e) = store.push_and_save(&cmd) {
                            tracing::warn!("Failed to save history: {}", e);
                        }
                    }
                }
                self.input_buffer.clear();
                self.dismiss_completion();
                self.awaiting_prompt_capture = true;
                self.emulator.mark_command_line_with_cmd(&cmd);
                Some(b"\r" as &[u8])
                } // end else (not alt screen)
            }
            egui::Key::Backspace => {
                if !self.emulator.is_interactive() {
                    self.input_buffer.pop();
                }
                Some(b"\x7f")
            }
            egui::Key::Tab => {
                if !self.emulator.is_interactive() {
                    if self.awaiting_prompt_capture {
                        self.prompt_end_col = self.emulator.grid.cursor_col;
                        self.prompt_row = self.emulator.grid.cursor_row;
                        self.awaiting_prompt_capture = false;
                    }
                }
                Some(b"\t")
            }
            egui::Key::Escape => {
                self.dismiss_completion();
                Some(b"\x1b")
            }
            egui::Key::ArrowUp => {
                self.dismiss_completion();
                if self.emulator.application_cursor_keys { Some(b"\x1bOA") } else { Some(b"\x1b[A") }
            }
            egui::Key::ArrowDown => {
                self.dismiss_completion();
                if self.emulator.application_cursor_keys { Some(b"\x1bOB") } else { Some(b"\x1b[B") }
            }
            egui::Key::ArrowRight => {
                // If there is a ghost text suggestion and cursor is at end of line, accept suggestion
                if let Some(suggestion) = self.current_suggestion.take() {
                    if !suggestion.is_empty() {
                        let cursor_col = self.emulator.grid.cursor_col;
                        let row = &self.emulator.grid.cells[self.emulator.grid.cursor_row];
                        let line_end = row.cells.iter().rposition(|c| c.ch != ' ' && !c.wide_placeholder)
                            .map(|p| p + 1).unwrap_or(0);
                        if cursor_col >= line_end {
                            self.write_and_broadcast(suggestion.as_bytes());
                            self.dismiss_completion();
                            return;
                        }
                    }
                }
                if self.emulator.application_cursor_keys { Some(b"\x1bOC") } else { Some(b"\x1b[C") }
            }
            egui::Key::ArrowLeft => {
                if self.emulator.application_cursor_keys { Some(b"\x1bOD") } else { Some(b"\x1b[D") }
            }
            egui::Key::Home => Some(b"\x1b[H"),
            egui::Key::End => Some(b"\x1b[F"),
            egui::Key::PageUp => Some(b"\x1b[5~"),
            egui::Key::PageDown => Some(b"\x1b[6~"),
            egui::Key::Delete => Some(b"\x1b[3~"),
            egui::Key::F1 => Some(b"\x1bOP"),
            egui::Key::F2 => Some(b"\x1bOQ"),
            egui::Key::F3 => Some(b"\x1bOR"),
            egui::Key::F4 => Some(b"\x1bOS"),
            _ => None,
        };

        if let Some(b) = bytes {
            self.write_and_broadcast(b);
        }
    }

    /// Accept the currently selected completion candidate
    fn accept_completion(&mut self) {
        let idx = self.completion_selected.unwrap_or(0);
        if let Some(cmd) = self.completion_candidates.get(idx).cloned() {
            // First clear the current input line (send Ctrl+U to clear before cursor + Ctrl+K to clear after)
            self.write_and_broadcast(b"\x15"); // Ctrl+U: clear content before cursor
            self.write_and_broadcast(b"\x0b"); // Ctrl+K: clear content after cursor
            // Send the selected command
            self.write_and_broadcast(cmd.as_bytes());
            // Sync input_buffer
            self.input_buffer = cmd;
        }
        self.dismiss_completion();
    }

    /// Close the completion popup and reset state
    fn dismiss_completion(&mut self) {
        self.show_completion_popup = false;
        self.completion_candidates.clear();
        self.completion_selected = None;
        self.current_suggestion = None;
        self.last_input_prefix.clear();
    }

    /// Clear command history for the current session
    pub fn clear_history(&mut self) {
        if let Some(store) = &mut self.history_store {
            let _ = store.clear_all();
        }
        self.completion_engine = Some(CompletionEngine::new(CommandHistory::new()));
        self.dismiss_completion();
    }

    /// Initialize session history and completion engine
    fn init_history(&mut self, session_key: &str) {
        self.session_key = Some(session_key.to_string());
        match EncryptedHistoryStore::load(session_key) {
            Ok(store) => {
                // Clean history entries: strip any residual prompt prefixes
                let cleaned: std::collections::VecDeque<String> = store.entries().iter()
                    .map(|entry| Self::strip_prompt_from_entry(entry))
                    .filter(|e| !e.is_empty())
                    .collect();
                let history = CommandHistory::from_entries(cleaned);
                self.completion_engine = Some(CompletionEngine::new(history));
                self.history_store = Some(store);
                tracing::info!("History loaded for session: {}", session_key);
            }
            Err(e) => {
                tracing::warn!("Failed to load history for {}: {}", session_key, e);
                self.completion_engine = Some(CompletionEngine::new(CommandHistory::new()));
            }
        }
    }

    /// Strip shell prompt prefix from a history entry
    fn strip_prompt_from_entry(entry: &str) -> String {
        let line = entry.trim();
        if line.is_empty() {
            return String::new();
        }
        // Match "user@host:path$ cmd" or "[user@host path]$ cmd"
        // Also matches "$cmd" (no space) and "$ cmd" (with space)
        for marker in &["$ ", "# ", "% "] {
            if let Some(pos) = line.rfind(marker) {
                let rest = line[pos + marker.len()..].trim();
                if !rest.is_empty() {
                    return rest.to_string();
                }
            }
        }
        // Match trailing $, #, % immediately followed by command (no space)
        for marker in &['$', '#', '%'] {
            if let Some(pos) = line.rfind(*marker) {
                // Ensure the marker is preceded by a prompt characteristic (@, ], or :)
                if pos > 0 {
                    let before = line.as_bytes()[pos - 1];
                    if before == b'@' || before == b']' || before == b':' || before == b' ' || before == b'~' {
                        let rest = line[pos + 1..].trim();
                        if !rest.is_empty() {
                            return rest.to_string();
                        }
                    }
                }
            }
        }
        line.to_string()
    }

    /// Read the current user input from the screen using the known prompt position.
    /// This is more reliable than tracking keystrokes, as it reflects Tab completion,
    /// shell-side edits, and any other changes the shell makes.
    fn read_screen_input(&self) -> String {
        let start_col = self.prompt_end_col;
        let cursor_col = self.emulator.grid.cursor_col;
        let cursor_row = self.emulator.grid.cursor_row;
        let row_idx = self.prompt_row;

        let mut result = String::new();
        if cursor_row == row_idx {
            if let Some(row) = self.emulator.grid.cells.get(row_idx) {
                let end = cursor_col.min(row.cells.len());
                for i in start_col..end {
                    if !row.cells[i].wide_placeholder {
                        result.push(row.cells[i].ch);
                    }
                }
            }
        } else if cursor_row > row_idx {
            for r in row_idx..=cursor_row {
                if let Some(row) = self.emulator.grid.cells.get(r) {
                    let sc = if r == row_idx { start_col } else { 0 };
                    let ec = if r == cursor_row { cursor_col.min(row.cells.len()) } else { row.cells.len() };
                    for i in sc..ec {
                        if !row.cells[i].wide_placeholder {
                            result.push(row.cells[i].ch);
                        }
                    }
                }
            }
        }
        result.trim_end().to_string()
    }

    /// Refresh ghost text suggestion and dropdown candidate list
    fn refresh_suggestion(&mut self) {
        let prefix = self.input_buffer.clone();
        if prefix == self.last_input_prefix {
            return;
        }
        self.last_input_prefix = prefix.clone();
        if prefix.is_empty() {
            self.current_suggestion = None;
            self.show_completion_popup = false;
            self.completion_candidates.clear();
            self.completion_selected = None;
            return;
        }
        if let Some(engine) = &self.completion_engine {
            // ghost text: best prefix-matched suggestion
            self.current_suggestion = engine.inline_suggestion(&prefix);
            // dropdown list: prefix-matched candidates
            self.completion_candidates = engine.prefix_search(&prefix, 10);
            if self.completion_candidates.is_empty() {
                self.show_completion_popup = false;
            } else {
                self.show_completion_popup = true;
                // Default: no entry selected
                self.completion_selected = None;
            }
        } else {
            self.current_suggestion = None;
            self.show_completion_popup = false;
            self.completion_candidates.clear();
        }
    }
}
