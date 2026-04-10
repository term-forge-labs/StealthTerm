use egui::{Context, Ui};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use stealthterm_sftp::{SftpClient, RemoteEntry};
use stealthterm_sftp::transfer::{TransferQueue, TransferStatus};
use crate::theme::Theme;
use crate::icons;
use stealthterm_config::i18n::t;

/// A local file entry for the browser
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

pub struct SftpPanel {
    pub visible: bool,
    pub local_path: PathBuf,
    pub remote_path: PathBuf,
    pub transfer_queue: TransferQueue,
    local_entries: Vec<FileEntry>,
    remote_entries: Vec<RemoteEntry>,
    local_needs_refresh: bool,
    remote_needs_refresh: bool,
    sftp_slot: Option<Arc<Mutex<Option<SftpClient>>>>,
    selected_local: Option<usize>,
    selected_remote: Option<usize>,
    remote_rx: Option<tokio::sync::mpsc::UnboundedReceiver<Vec<RemoteEntry>>>,
}

impl SftpPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            local_path: dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
            remote_path: PathBuf::from("/"),
            transfer_queue: TransferQueue::new(),
            local_entries: Vec::new(),
            remote_entries: Vec::new(),
            local_needs_refresh: true,
            remote_needs_refresh: true,
            sftp_slot: None,
            selected_local: None,
            selected_remote: None,
            remote_rx: None,
        }
    }

    pub fn set_sftp_slot(&mut self, slot: Arc<Mutex<Option<SftpClient>>>) {
        self.sftp_slot = Some(slot);
        self.remote_needs_refresh = true;
        tracing::info!("SFTP slot set, remote_needs_refresh = true");
    }

    fn refresh_local(&mut self) {
        self.local_entries.clear();
        if let Ok(entries) = std::fs::read_dir(&self.local_path) {
            let mut items: Vec<FileEntry> = entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let meta = e.metadata().ok();
                    FileEntry {
                        name: e.file_name().to_string_lossy().to_string(),
                        is_dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                        size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    }
                })
                .collect();
            items.sort_by(|a, b| {
                b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            self.local_entries = items;
        }
        self.local_needs_refresh = false;
    }

    fn refresh_remote(&mut self, ctx: &Context) {
        tracing::info!("refresh_remote called, sftp_slot: {}", self.sftp_slot.is_some());
        if let Some(slot) = &self.sftp_slot {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            self.remote_rx = Some(rx);

            let path = self.remote_path.clone();
            let slot_clone = slot.clone();
            let ctx_clone = ctx.clone();

            tracing::info!("Spawning task to list remote dir: {:?}", path);
            tokio::spawn(async move {
                tracing::info!("Task started, trying to lock slot");
                if let Ok(mut client_opt) = slot_clone.try_lock() {
                    tracing::info!("Slot locked, client present: {}", client_opt.is_some());
                    if let Some(client) = client_opt.as_mut() {
                        let mut client_clone = std::mem::replace(client, stealthterm_sftp::SftpClient::new());
                        if let Ok(entries) = client_clone.list_dir(&path).await {
                            tracing::info!("Remote dir listed: {} entries", entries.len());
                            let _ = tx.send(entries);
                        } else {
                            tracing::error!("Failed to list remote dir");
                        }
                        *client = client_clone;
                        ctx_clone.request_repaint();
                    }
                } else {
                    tracing::warn!("Failed to lock slot");
                }
            });
        }
        self.remote_needs_refresh = false;
    }

    fn format_size(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    fn start_upload(&mut self, local: PathBuf, remote: PathBuf) {
        if let Some(slot) = &self.sftp_slot {
            let slot_clone = slot.clone();
            tokio::spawn(async move {
                if let Ok(mut client_opt) = slot_clone.try_lock() {
                    if let Some(client) = client_opt.as_mut() {
                        let mut client_clone = std::mem::replace(client, stealthterm_sftp::SftpClient::new());
                        if let Err(e) = client_clone.upload(&local, &remote).await {
                            tracing::error!("Upload failed: {}", e);
                        } else {
                            tracing::info!("Upload success: {:?}", remote);
                        }
                        *client = client_clone;
                    }
                }
            });
        }
    }

    fn start_download(&mut self, remote: PathBuf, local: PathBuf) {
        if let Some(slot) = &self.sftp_slot {
            let slot_clone = slot.clone();
            tokio::spawn(async move {
                if let Ok(mut client_opt) = slot_clone.try_lock() {
                    if let Some(client) = client_opt.as_mut() {
                        let mut client_clone = std::mem::replace(client, stealthterm_sftp::SftpClient::new());
                        if let Err(e) = client_clone.download(&remote, &local).await {
                            tracing::error!("Download failed: {}", e);
                        } else {
                            tracing::info!("Download success: {:?}", local);
                        }
                        *client = client_clone;
                    }
                }
            });
        }
    }

    /// Show using &mut Ui (for embedding in panels)
    pub fn show(&mut self, ui: &mut Ui, _theme: &Theme) {
        self.show_window_inner(ui.ctx());
    }

    /// Show using &egui::Context directly (for floating windows from app)
    pub fn show_ctx(&mut self, ctx: &Context, _theme: &Theme) {
        tracing::info!("SFTP panel show_ctx called, visible: {}", self.visible);
        self.show_window_inner(ctx);
    }

    fn show_window_inner(&mut self, ctx: &Context) {
        if !self.visible { return; }

        tracing::info!("SFTP panel show_window_inner, remote_needs_refresh: {}", self.remote_needs_refresh);

        // Receive remote file list
        if let Some(rx) = &mut self.remote_rx {
            if let Ok(entries) = rx.try_recv() {
                self.remote_entries = entries;
                tracing::info!("Updated remote entries: {} files", self.remote_entries.len());
            }
        }

        if self.local_needs_refresh {
            self.refresh_local();
        }

        if self.remote_needs_refresh {
            self.refresh_remote(ctx);
        }

        let mut upload_request: Option<(PathBuf, PathBuf)> = None;
        let mut download_request: Option<(PathBuf, PathBuf)> = None;

        egui::Window::new(t("sftp.title"))
            .resizable(true)
            .default_size([700.0, 500.0])
            .collapsible(false)
            .open(&mut self.visible)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Local pane
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() / 2.0 - 4.0);
                        ui.heading(t("sftp.local"));

                        // Breadcrumb path bar
                        ui.horizontal(|ui| {
                            if ui.small_button(icons::ICON_FOLDER).clicked() {
                                if let Some(home) = dirs::home_dir() {
                                    self.local_path = home;
                                    self.local_needs_refresh = true;
                                }
                            }
                            ui.label(self.local_path.display().to_string());
                            if ui.small_button("\u{21bb}").clicked() {
                                self.local_needs_refresh = true;
                            }
                        });

                        ui.separator();

                        // File list
                        egui::ScrollArea::vertical()
                            .max_height(350.0)
                            .show(ui, |ui| {
                                // ".." to go up
                                if self.local_path.parent().is_some() {
                                    let resp = ui.selectable_label(false, "\u{1f4c1} ..");
                                    if resp.double_clicked() {
                                        if let Some(parent) = self.local_path.parent() {
                                            self.local_path = parent.to_path_buf();
                                            self.local_needs_refresh = true;
                                        }
                                    }
                                }

                                // We need to collect indices to avoid borrow issues
                                let count = self.local_entries.len();
                                let mut navigate_to: Option<PathBuf> = None;

                                for i in 0..count {
                                    let entry = &self.local_entries[i];
                                    let icon = if entry.is_dir { icons::ICON_FOLDER } else { icons::ICON_FILE };
                                    let size_str = if entry.is_dir {
                                        String::new()
                                    } else {
                                        Self::format_size(entry.size)
                                    };
                                    let label = format!("{} {}  {}", icon, entry.name, size_str);
                                    let selected = self.selected_local == Some(i);
                                    let resp = ui.selectable_label(selected, label);
                                    if resp.clicked() {
                                        self.selected_local = Some(i);
                                    }
                                    if resp.secondary_clicked() {
                                        self.selected_local = Some(i);
                                    }
                                    if resp.double_clicked() && entry.is_dir {
                                        navigate_to = Some(self.local_path.join(&entry.name));
                                    }
                                }

                                if let Some(path) = navigate_to {
                                    self.local_path = path;
                                    self.local_needs_refresh = true;
                                }
                            });
                    });

                    // Transfer buttons
                    ui.vertical(|ui| {
                        ui.add_space(150.0);

                        let can_upload = self.selected_local.is_some() && self.sftp_slot.is_some();
                        if ui.add_enabled(can_upload, egui::Button::new("→ Upload")).clicked() {
                            if let Some(idx) = self.selected_local {
                                if let Some(entry) = self.local_entries.get(idx) {
                                    if !entry.is_dir {
                                        let local = self.local_path.join(&entry.name);
                                        let remote = self.remote_path.join(&entry.name);
                                        upload_request = Some((local, remote));
                                    }
                                }
                            }
                        }

                        ui.add_space(10.0);

                        let can_download = self.selected_remote.is_some() && self.sftp_slot.is_some();
                        if ui.add_enabled(can_download, egui::Button::new(format!("← {}", t("sftp.download")))).clicked() {
                            if let Some(idx) = self.selected_remote {
                                if let Some(entry) = self.remote_entries.get(idx) {
                                    if !entry.is_dir {
                                        let remote = entry.path.clone();
                                        let local = self.local_path.join(&entry.name);
                                        download_request = Some((remote, local));
                                    }
                                }
                            }
                        }
                    });

                    ui.separator();

                    // Remote pane
                    ui.vertical(|ui| {
                        ui.heading(t("sftp.remote"));

                        if self.sftp_slot.is_some() {
                            ui.horizontal(|ui| {
                                if ui.small_button(icons::ICON_FOLDER).clicked() {
                                    self.remote_path = PathBuf::from("/");
                                    self.remote_needs_refresh = true;
                                }
                                ui.label(self.remote_path.display().to_string());
                                if ui.small_button("\u{21bb}").clicked() {
                                    self.remote_needs_refresh = true;
                                }
                            });

                            ui.separator();

                            egui::ScrollArea::vertical()
                                .max_height(350.0)
                                .show(ui, |ui| {
                                    if self.remote_path.parent().is_some() {
                                        let resp = ui.selectable_label(false, "\u{1f4c1} ..");
                                        if resp.double_clicked() {
                                            if let Some(parent) = self.remote_path.parent() {
                                                self.remote_path = parent.to_path_buf();
                                                self.remote_needs_refresh = true;
                                            }
                                        }
                                    }

                                    let mut navigate_to: Option<PathBuf> = None;
                                    for (i, entry) in self.remote_entries.iter().enumerate() {
                                        let icon = if entry.is_dir { icons::ICON_FOLDER } else { icons::ICON_FILE };
                                        let size_str = if entry.is_dir {
                                            String::new()
                                        } else {
                                            Self::format_size(entry.size)
                                        };
                                        let label = format!("{} {}  {}", icon, entry.name, size_str);
                                        let selected = self.selected_remote == Some(i);
                                        let resp = ui.selectable_label(selected, label);
                                        if resp.clicked() {
                                            self.selected_remote = Some(i);
                                        }
                                        if resp.secondary_clicked() {
                                            self.selected_remote = Some(i);
                                        }
                                        if resp.double_clicked() && entry.is_dir {
                                            navigate_to = Some(entry.path.clone());
                                        }
                                    }

                                    if let Some(path) = navigate_to {
                                        self.remote_path = path;
                                        self.remote_needs_refresh = true;
                                    }
                                });
                        } else {
                            ui.separator();
                            ui.add_space(20.0);
                            ui.centered_and_justified(|ui| {
                                ui.label(t("sftp.not_connected"));
                            });
                        }
                    });
                });

                ui.separator();

                // Transfer queue
                ui.heading(t("sftp.transfer_queue"));
                if self.transfer_queue.tasks.is_empty() {
                    ui.label(t("sftp.no_transfers"));
                } else {
                    for task in &self.transfer_queue.tasks {
                        ui.horizontal(|ui| {
                            ui.label(task.source.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default());
                            let progress = task.progress();
                            ui.add(egui::ProgressBar::new(progress)
                                .desired_width(200.0)
                                .show_percentage());
                            let status = match &task.status {
                                TransferStatus::Pending => t("sftp.status_pending"),
                                TransferStatus::InProgress { .. } => t("sftp.status_transferring"),
                                TransferStatus::Paused => t("sftp.status_paused"),
                                TransferStatus::Completed => t("sftp.status_done"),
                                TransferStatus::Failed(_) => t("sftp.status_failed"),
                            };
                            ui.label(status);
                        });
                    }
                }
            });

        // Process transfer requests outside the window closure
        if let Some((local, remote)) = upload_request {
            self.start_upload(local, remote);
        }
        if let Some((remote, local)) = download_request {
            self.start_download(remote, local);
        }
    }
}

impl Default for SftpPanel {
    fn default() -> Self {
        Self::new()
    }
}
