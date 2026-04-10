use egui::Ui;
use egui_twemoji::EmojiLabel;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::sync::Mutex;
use stealthterm_config::connections::{ConnectionConfig, ConnectionStore, ConnectionType};
use stealthterm_config::i18n::t;
use stealthterm_sftp::{SftpClient, RemoteEntry};
use crate::theme::Theme;
use crate::icons;

#[derive(Debug, PartialEq)]
pub enum SidebarAction {
    None,
    OpenConnection(String),
    NewConnection,
    NewConnectionInGroup(String),
    EditConnection(String),
    DeleteConnection(String),
    CopyConnection(String),
    PasteConnection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarTab {
    Ssh,
    Sftp,
}

pub struct Sidebar {
    pub visible: bool,
    pub collapsed: bool,
    pub active_tab: SidebarTab,
    pub search_query: String,
    pub expanded_groups: std::collections::HashSet<String>,
    pub width: f32,
    pub current_remote_path: Option<String>,
    pub active_connection_id: Option<String>,
    pub sftp_slot: Option<Arc<Mutex<Option<SftpClient>>>>,
    pub copied_connection_id: Option<String>,
    selected_file: Option<PathBuf>,
    local_path: PathBuf,
    remote_path: PathBuf,
    remote_entries: Vec<RemoteEntry>,
    remote_rx: Option<tokio::sync::mpsc::UnboundedReceiver<Vec<RemoteEntry>>>,
    path_rx: Option<tokio::sync::mpsc::UnboundedReceiver<PathBuf>>,
    initial_path_requested: bool,
    refresh_requested: bool,
    upload_done_rx: Option<tokio::sync::mpsc::UnboundedReceiver<()>>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            visible: true,
            collapsed: false,
            active_tab: SidebarTab::Ssh,
            search_query: String::new(),
            expanded_groups: std::collections::HashSet::new(),
            width: 220.0,
            current_remote_path: None,
            active_connection_id: None,
            sftp_slot: None,
            copied_connection_id: None,
            selected_file: None,
            local_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            remote_path: PathBuf::from("/"),
            remote_entries: Vec::new(),
            remote_rx: None,
            path_rx: None,
            initial_path_requested: false,
            refresh_requested: false,
            upload_done_rx: None,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn toggle_collapse(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn set_active_connection(&mut self, connection_id: Option<String>, remote_path: Option<String>) {
        self.active_connection_id = connection_id;
        self.current_remote_path = remote_path.clone();

        // Only update on first set or when path actually changes
        if let Some(path) = remote_path {
            let new_path = PathBuf::from(&path);
            // If current path is root and new path is not root, update (initialization)
            // Or if new path differs from current and is not root, update
            if (self.remote_path == PathBuf::from("/") && new_path != PathBuf::from("/")) {
                tracing::info!("Updating remote_path from / to {:?}", new_path);
                self.remote_path = new_path;
                self.remote_entries.clear();
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, store: &ConnectionStore, theme: &Theme) -> SidebarAction {
        let mut action = SidebarAction::None;

        if !self.visible {
            return action;
        }

        let frame = egui::Frame::none()
            .fill(theme.sidebar_bg)
            .inner_margin(if self.collapsed {
                egui::Margin::symmetric(0, 4)
            } else {
                egui::Margin::symmetric(2, 4)
            });

        frame.show(ui, |ui| {
            if self.collapsed {
                ui.add_space(6.0);
                let btn_size = egui::vec2(ui.available_width().min(32.0), 28.0);
                for (emoji, tab) in [("▶", None), ("📂", Some(SidebarTab::Sftp)), ("🔐", Some(SidebarTab::Ssh))] {
                    let w = ui.available_width();
                    let pad = ((w - btn_size.x) / 2.0).max(0.0);
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        let resp = super::emoji_button::emoji_button(ui, emoji, btn_size);
                        if resp.hovered() {
                            ui.painter().rect_filled(resp.rect, 4.0, egui::Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x20));
                        }
                        if resp.clicked() {
                            if let Some(t) = tab {
                                self.active_tab = t;
                            }
                            self.collapsed = false;
                        }
                    });
                    ui.add_space(2.0);
                }
                return;
            }

            // Use actual available width instead of hardcoded self.width
            let avail_w = ui.available_width();

            // Header: [◀] ... [Files] [SSH] ... [➕]
            ui.horizontal(|ui| {
                // Collapse button — left
                if EmojiLabel::new("◀").show(ui).clicked() {
                    self.collapsed = true;
                }

                // Center area: push tabs to center using available space
                let remaining = ui.available_width();
                // Estimate tab widths (~50 each) + plus button (~20)
                let tabs_width = 120.0;
                let plus_width = if self.active_tab == SidebarTab::Ssh { 24.0 } else { 0.0 };
                let center_offset = ((remaining - tabs_width - plus_width) / 2.0).max(4.0);
                ui.add_space(center_offset);

                if ui.selectable_label(self.active_tab == SidebarTab::Sftp, t("sidebar.file_manager")).clicked() {
                    self.active_tab = SidebarTab::Sftp;
                }
                if ui.selectable_label(self.active_tab == SidebarTab::Ssh, t("sidebar.ssh_connections")).clicked() {
                    self.active_tab = SidebarTab::Ssh;
                }

                // Plus button — right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.active_tab == SidebarTab::Ssh {
                        if EmojiLabel::new("➕").show(ui).clicked() {
                            action = SidebarAction::NewConnection;
                        }
                    }
                });
            });

            ui.separator();

            // Search
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text(t("sidebar.search_placeholder"))
                        .desired_width(f32::INFINITY)
                        .text_color(theme.sidebar_fg),
                );
                if !self.search_query.is_empty() {
                    if EmojiLabel::new("❌").show(ui).clicked() {
                        self.search_query.clear();
                    }
                }
            });

            ui.separator();
            ui.add_space(4.0);

            // Detect file drag-and-drop (when on the file manager tab)
            let dropped_files = if self.active_tab == SidebarTab::Sftp {
                let files = ui.input(|i| i.raw.dropped_files.clone());
                if !files.is_empty() {
                    tracing::info!("Sidebar detected {} dropped files", files.len());
                    // Consume the drop event to prevent it from propagating to the terminal
                    ui.ctx().input_mut(|i| i.raw.dropped_files.clear());
                }
                files
            } else {
                Vec::new()
            };

            // Wrap content in ScrollArea to fill remaining height
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(avail_w);

                    // Show content based on active tab
                    if self.active_tab == SidebarTab::Sftp {
                        // File manager view
                        self.show_file_list_content(ui, theme);
                    } else {
                        // SSH connection list
                        self.show_connection_list_content(ui, store, theme, &mut action);
                    }
                });

            // Handle dropped files
            if !dropped_files.is_empty() {
                for file in &dropped_files {
                    if let Some(path) = &file.path {
                        self.upload_file(path.clone(), ui.ctx());
                    }
                }
            }
        });

        action
    }

    fn show_file_list_content(&mut self, ui: &mut Ui, theme: &Theme) {
        // Check for remote entries from async task
        if let Some(rx) = &mut self.remote_rx {
            if let Ok(entries) = rx.try_recv() {
                tracing::info!("Received {} remote entries", entries.len());
                self.remote_entries = entries;
                self.refresh_requested = false;
            }
        }

        // Check for initial path from async task
        if let Some(rx) = &mut self.path_rx {
            if let Ok(path) = rx.try_recv() {
                tracing::info!("Received initial remote path: {:?}", path);
                self.remote_path = path;
                self.remote_entries.clear();
                self.path_rx = None; // clear to avoid reprocessing
                // Immediately refresh file list
                self.refresh_remote(ui.ctx());
            }
        }

        let has_sftp = if let Some(slot) = &self.sftp_slot {
            if let Ok(client_opt) = slot.try_lock() {
                client_opt.is_some()
            } else {
                true  // lock failure means it's in use, keep state
            }
        } else {
            false
        };

        // If no SFTP connection, clear remote file list and status
        if !has_sftp {
            if !self.remote_entries.is_empty() || self.remote_path != PathBuf::from("/") {
                self.remote_entries.clear();
                self.remote_path = PathBuf::from("/");
                self.initial_path_requested = false;
                self.refresh_requested = false;
                self.path_rx = None;
                self.remote_rx = None;
            }
        }

        if has_sftp {
            EmojiLabel::new(egui::RichText::new(t("sidebar.remote_files")).color(theme.sidebar_fg)).show(ui);

            // Fetch working directory on first connection
            if !self.initial_path_requested && self.remote_path == PathBuf::from("/") {
                self.initial_path_requested = true;
                self.get_initial_remote_path(ui.ctx());
            }

            self.show_remote_files(ui, theme);
        } else {
            EmojiLabel::new(egui::RichText::new(t("sidebar.local_files")).color(theme.sidebar_fg)).show(ui);
            self.show_local_files(ui);
        }
    }

    fn get_initial_remote_path(&mut self, ctx: &egui::Context) {
        tracing::info!("get_initial_remote_path called");
        if let Some(slot) = &self.sftp_slot {
            tracing::info!("SFTP slot exists");
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            self.path_rx = Some(rx);
            let slot_clone = slot.clone();
            let ctx_clone = ctx.clone();

            tokio::spawn(async move {
                tracing::info!("Async task started for realpath");
                if let Ok(mut client_opt) = slot_clone.try_lock() {
                    tracing::info!("Got lock on SFTP client");
                    if let Some(client) = client_opt.as_mut() {
                        tracing::info!("SFTP client exists, calling realpath");
                        let mut client_clone = std::mem::replace(client, SftpClient::new());
                        match client_clone.realpath(".").await {
                            Ok(path) => {
                                tracing::info!("Got initial remote path: {:?}", path);
                                let _ = tx.send(path);
                            }
                            Err(e) => {
                                tracing::error!("realpath failed: {}", e);
                            }
                        }
                        *client = client_clone;
                        ctx_clone.request_repaint();
                    } else {
                        tracing::warn!("SFTP client is None");
                    }
                } else {
                    tracing::warn!("Failed to lock SFTP client");
                }
            });
        } else {
            tracing::warn!("SFTP slot is None");
        }
    }

    fn show_local_files(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(self.local_path.display().to_string());
            if ui.small_button("🔄").clicked() {
                // Refresh handled automatically
            }
        });
        ui.separator();

        if self.local_path.parent().is_some() {
            if ui.button("📁 ..").clicked() {
                if let Some(parent) = self.local_path.parent() {
                    self.local_path = parent.to_path_buf();
                }
            }
        }

        if let Ok(entries) = std::fs::read_dir(&self.local_path) {
            let mut items: Vec<_> = entries.flatten().collect();
            items.sort_by_key(|e| {
                let is_dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false);
                (!is_dir, e.file_name())
            });

            for entry in items {
                if let Ok(metadata) = entry.metadata() {
                    let icon = if metadata.is_dir() { "📂" } else { "📄" };
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_selected = self.selected_file.as_ref() == Some(&entry.path());

                    let label_text = format!("{} {}", icon, name);

                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(200, 200, 200)
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    let resp = ui.horizontal(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg_color);
                        EmojiLabel::new(label_text).show(ui)
                    }).inner;

                    if resp.clicked() {
                        self.selected_file = Some(entry.path());
                        if metadata.is_dir() {
                            self.local_path = entry.path();
                        }
                    }
                }
            }
        }
    }

    fn show_remote_files(&mut self, ui: &mut Ui, _theme: &Theme) {
        tracing::info!("show_remote_files: path={:?}, entries={}, initial_requested={}",
            self.remote_path, self.remote_entries.len(), self.initial_path_requested);

        // Check if upload completed — auto-refresh
        if let Some(rx) = &mut self.upload_done_rx {
            if rx.try_recv().is_ok() {
                self.upload_done_rx = None;
                self.refresh_remote(ui.ctx());
            }
        }

        ui.horizontal(|ui| {
            ui.label(self.remote_path.display().to_string());
            if ui.small_button("🔄").clicked() {
                self.refresh_remote(ui.ctx());
            }
            ui.add_space(12.0);
            if ui.small_button(format!("📤 {}", t("sidebar.upload"))).clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.upload_file(path, ui.ctx());
                }
            }
        });
        ui.separator();

        // If still waiting for initial path, show hint and keep requesting repaint
        if self.remote_path == PathBuf::from("/") && self.initial_path_requested {
            ui.label(t("sidebar.loading_working_dir"));
            ui.ctx().request_repaint();
            return;
        }

        // Refresh on first load (only once)
        if self.remote_entries.is_empty() && self.remote_rx.is_none() && !self.refresh_requested {
            self.refresh_requested = true;
            self.refresh_remote(ui.ctx());
        }

        if self.remote_path.parent().is_some() {
            if ui.button("📁 ..").clicked() {
                if let Some(parent) = self.remote_path.parent() {
                    self.remote_path = parent.to_path_buf();
                    self.refresh_remote(ui.ctx());
                }
            }
        }

        let mut navigate_to: Option<PathBuf> = None;
        let download_file = std::cell::RefCell::new(None::<(PathBuf, String)>);

        for (_i, entry) in self.remote_entries.iter().enumerate() {
            let icon = if entry.is_dir { "📂" } else { "📄" };
            let is_selected = self.selected_file.as_ref() == Some(&entry.path);

            let label_text = format!("{} {}", icon, entry.name);

            let bg_color = if is_selected {
                egui::Color32::from_rgb(200, 200, 200)
            } else {
                egui::Color32::TRANSPARENT
            };

            // First draw the row normally (including colored emoji)
            let inner_resp = ui.horizontal(|ui| {
                let rect = ui.available_rect_before_wrap();
                ui.painter().rect_filled(rect, 0.0, bg_color);
                EmojiLabel::new(label_text).show(ui)
            });

            // Overlay a transparent interaction layer over the entire row
            let resp = ui.interact(
                inner_resp.response.rect,
                ui.id().with(format!("file_interact_{}", entry.name)),
                egui::Sense::click()
            );

            tracing::info!(">>> File entry: {:?}, clicked={}, secondary_clicked={}",
                entry.path, resp.clicked(), resp.secondary_clicked());

            if resp.clicked() {
                tracing::info!(">>> LEFT CLICKED: {:?}", entry.path);
                self.selected_file = Some(entry.path.clone());
                if entry.is_dir {
                    navigate_to = Some(entry.path.clone());
                }
            }

            if resp.secondary_clicked() {
                tracing::info!(">>> RIGHT CLICKED: {:?}", entry.path);
                self.selected_file = Some(entry.path.clone());
            }

            let entry_path = entry.path.clone();
            let entry_name = entry.name.clone();
            let is_dir = entry.is_dir;

            let menu_response = resp.context_menu(|ui| {
                tracing::info!(">>> Context menu callback: path={:?}, is_dir={}", entry_path, is_dir);
                if !is_dir {
                    let btn = ui.button(t("sidebar.download"));
                    tracing::info!(">>> Button: clicked={}, hovered={}", btn.clicked(), btn.hovered());
                    if btn.clicked() {
                        tracing::info!(">>> DOWNLOAD CLICKED!");
                        *download_file.borrow_mut() = Some((entry_path.clone(), entry_name.clone()));
                        ui.close_menu();
                    }
                }
            });

            if menu_response.is_some() {
                tracing::info!(">>> Menu opened for: {:?}", entry_path);
            }
        }

        if let Some(path) = navigate_to {
            self.remote_path = path;
            self.refresh_remote(ui.ctx());
        }

        let download = download_file.borrow_mut().take();
        if let Some((remote_path, filename)) = download {
            tracing::info!(">>> download_file is Some, calling download_file()");
            self.download_file(remote_path, filename, ui.ctx());
        }
    }

    fn refresh_remote(&mut self, ctx: &egui::Context) {
        if let Some(slot) = &self.sftp_slot {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            self.remote_rx = Some(rx);
            let path = self.remote_path.clone();
            let slot_clone = slot.clone();
            let ctx_clone = ctx.clone();

            tokio::spawn(async move {
                if let Ok(mut client_opt) = slot_clone.try_lock() {
                    if let Some(client) = client_opt.as_mut() {
                        let mut client_clone = std::mem::replace(client, SftpClient::new());
                        if let Ok(entries) = client_clone.list_dir(&path).await {
                            let _ = tx.send(entries);
                            ctx_clone.request_repaint();
                        }
                        *client = client_clone;
                    }
                }
            });
        }
    }

    fn upload_file(&mut self, local_path: PathBuf, ctx: &egui::Context) {
        if let Some(slot) = &self.sftp_slot {
            let filename = local_path.file_name().unwrap().to_string_lossy().to_string();
            // Use forward slashes to join remote path
            let remote_path_str = format!("{}/{}", self.remote_path.to_string_lossy(), filename);
            let remote_path = PathBuf::from(&remote_path_str);
            let slot_clone = slot.clone();
            let ctx_clone = ctx.clone();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            self.upload_done_rx = Some(rx);

            tracing::info!("Uploading {} to {:?}", local_path.display(), remote_path_str);

            tokio::spawn(async move {
                if let Ok(mut client_opt) = slot_clone.try_lock() {
                    if let Some(client) = client_opt.as_mut() {
                        let mut client_clone = std::mem::replace(client, SftpClient::new());
                        match client_clone.upload(&local_path, &remote_path).await {
                            Ok(_) => {
                                tracing::info!("Upload completed: {}", filename);
                                let _ = tx.send(());
                            }
                            Err(e) => tracing::error!("Upload failed: {}", e),
                        }
                        *client = client_clone;
                        ctx_clone.request_repaint();
                    }
                }
            });
        }
    }

    fn download_file(&mut self, remote_path: PathBuf, filename: String, ctx: &egui::Context) {
        tracing::info!(">>> download_file called: remote_path={:?}, filename={}", remote_path, filename);

        if let Some(slot) = &self.sftp_slot {
            tracing::info!(">>> sftp_slot exists, opening file dialog");
            // Use file dialog to let user choose save location
            if let Some(local_path) = rfd::FileDialog::new()
                .set_file_name(&filename)
                .save_file()
            {
                tracing::info!(">>> User selected save path: {:?}", local_path);
                let slot_clone = slot.clone();
                let ctx_clone = ctx.clone();

                tracing::info!("Downloading {:?} to {:?}", remote_path, local_path);

                tokio::spawn(async move {
                    match slot_clone.try_lock() {
                        Ok(mut client_opt) => {
                            if let Some(client) = client_opt.as_mut() {
                                let mut client_clone = std::mem::replace(client, SftpClient::new());
                                match client_clone.download(&remote_path, &local_path).await {
                                    Ok(_) => tracing::info!("Download completed: {}", filename),
                                    Err(e) => tracing::error!("Download failed: {}", e),
                                }
                                *client = client_clone;
                                ctx_clone.request_repaint();
                            } else {
                                tracing::error!("SFTP client is None");
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to lock SFTP client: {:?}", e);
                        }
                    }
                });
            }
        } else {
            tracing::error!("No SFTP slot available");
        }
    }

    fn show_connection_list_content(&mut self, ui: &mut Ui, store: &ConnectionStore, theme: &Theme, action: &mut SidebarAction) {
        let query = self.search_query.to_lowercase();
        let mut groups: Vec<Option<String>> = vec![None];
        let all_groups: Vec<String> = store.groups().iter().map(|s| s.to_string()).collect();
        groups.extend(all_groups.iter().map(|g| Some(g.clone())));

            for group in &groups {
                let conns: Vec<&ConnectionConfig> = store.connections.iter()
                    .filter(|c| c.group.as_deref() == group.as_deref())
                    .filter(|c| {
                        query.is_empty()
                            || c.name.to_lowercase().contains(&query)
                            || c.host.to_lowercase().contains(&query)
                            || c.username.to_lowercase().contains(&query)
                    })
                    .collect();

                if conns.is_empty() { continue; }

                if let Some(group_name) = group {
                    let is_expanded = self.expanded_groups.contains(group_name);
                    let folder_icon = if is_expanded { "📂" } else { "📁" };
                    let arrow = if is_expanded { "▼" } else { "▶" };

                    let inner_resp = ui.horizontal(|ui| {
                        let arrow_resp = ui.label(arrow);
                        ui.add_space(4.0);
                        let label_resp = EmojiLabel::new(format!("{} {}", folder_icon, group_name)).show(ui);
                        arrow_resp.union(label_resp)
                    });

                    // Use unique ID to cover the entire row interaction area
                    let resp = ui.interact(
                        inner_resp.response.rect,
                        ui.id().with(("group_interact", group_name.as_str())),
                        egui::Sense::click(),
                    );

                    if resp.clicked() {
                        if is_expanded {
                            self.expanded_groups.remove(group_name);
                        } else {
                            self.expanded_groups.insert(group_name.clone());
                        }
                    }

                    resp.context_menu(|ui| {
                        if ui.button(t("sidebar.new_ssh")).clicked() {
                            *action = SidebarAction::NewConnectionInGroup(group_name.clone());
                            ui.close_menu();
                        }
                    });

                    if !is_expanded { continue; }
                }

                for conn in conns {
                    let icon = match conn.connection_type {
                        ConnectionType::Ssh => "🔐",
                        ConnectionType::Local => "💻",
                        _ => "🔌",
                    };
                    let status_icon = "";

                    let is_selected = self.active_connection_id.as_ref() == Some(&conn.id);
                    let label_text = format!("  {} {} {}", status_icon, icon, conn.name);

                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(200, 200, 200)
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    let inner_resp = ui.horizontal(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, bg_color);
                        EmojiLabel::new(egui::RichText::new(label_text).color(theme.sidebar_fg)).show(ui)
                    });

                    // Use unique ID to cover the entire row interaction area, avoid context_menu ID conflict
                    let resp = ui.interact(
                        inner_resp.response.rect,
                        ui.id().with(("conn_interact", &conn.id)),
                        egui::Sense::click(),
                    );

                    if resp.clicked() {
                        self.active_connection_id = Some(conn.id.clone());
                    }
                    if resp.double_clicked() {
                        *action = SidebarAction::OpenConnection(conn.id.clone());
                    }

                    resp.context_menu(|ui| {
                        if ui.button(t("sidebar.open")).clicked() {
                            *action = SidebarAction::OpenConnection(conn.id.clone());
                            ui.close_menu();
                        }
                        if ui.button(t("sidebar.edit")).clicked() {
                            *action = SidebarAction::EditConnection(conn.id.clone());
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(t("sidebar.copy")).clicked() {
                            *action = SidebarAction::CopyConnection(conn.id.clone());
                            ui.close_menu();
                        }
                        if ui.button(t("sidebar.delete")).clicked() {
                            *action = SidebarAction::DeleteConnection(conn.id.clone());
                            ui.close_menu();
                        }
                    });
                }
            }

            // Blank area: fill remaining space, support right-click paste
            let remaining = ui.available_rect_before_wrap();
            let blank_resp = ui.allocate_rect(remaining, egui::Sense::click());
            blank_resp.context_menu(|ui| {
                if ui.button(t("sidebar.new_ssh")).clicked() {
                    *action = SidebarAction::NewConnection;
                    ui.close_menu();
                }
                if self.copied_connection_id.is_some() {
                    ui.separator();
                    if ui.button(t("sidebar.paste")).clicked() {
                        *action = SidebarAction::PasteConnection;
                        ui.close_menu();
                    }
                }
            });
    }
}


impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
