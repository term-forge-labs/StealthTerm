use eframe::CreationContext;
use egui::{CentralPanel, Color32, Context, Key, Modifiers, SidePanel, TopBottomPanel, Stroke, StrokeKind};
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::Mutex;

use stealthterm_config::connections::ConnectionStore;
use stealthterm_config::settings::Settings;
use stealthterm_sftp::SftpClient;
use stealthterm_ui::theme::Theme;
use stealthterm_ui::widgets::tab_bar::{Tab, TabBar, TabBarAction};
use stealthterm_ui::widgets::sidebar::{Sidebar, SidebarAction};
use stealthterm_ui::widgets::status_bar::StatusBar;
use stealthterm_ui::widgets::split_pane::{SplitPane, SplitDirection};
use stealthterm_ui::widgets::command_palette::{CommandPalette, CommandAction};
use stealthterm_ui::widgets::server_monitor::ServerMonitor;
use stealthterm_ui::panels::terminal_panel::TerminalPanel;
use stealthterm_ui::panels::connection_panel::ConnectionPanel;
use stealthterm_ui::panels::settings_panel::SettingsPanel;

use crate::event::AppEvent;

pub struct StealthTermApp {
    runtime: tokio::runtime::Runtime,
    settings: Settings,
    connections: ConnectionStore,
    theme: Theme,
    tab_bar: TabBar,
    sidebar: Sidebar,
    terminals: std::collections::HashMap<String, TerminalPanel>,
    sftp_sessions: std::collections::HashMap<String, Arc<Mutex<Option<SftpClient>>>>,
    connection_panel: ConnectionPanel,
    settings_panel: SettingsPanel,
    /// Batch mode: whether active
    batch_mode: bool,
    /// Batch mode: set of selected tab IDs
    batch_selected_tabs: std::collections::HashSet<String>,
    /// Whether batch selection window is visible
    batch_select_visible: bool,
    command_palette: CommandPalette,
    /// Optional split pane state; when Some, the active tab is split
    split_pane: Option<SplitPane>,
    /// The tab ID of the second pane in a split
    split_secondary_id: Option<String>,
    events: Vec<AppEvent>,
    font_size: f32,
    is_fullscreen: bool,
    start_time: std::time::Instant,
    show_main_menu: bool,
    main_menu_pos: egui::Pos2,
    /// Split screen button dropdown menu
    show_split_menu: bool,
    split_menu_pos: egui::Pos2,
    /// Split focus: false=main panel, true=secondary panel
    split_focus_secondary: bool,
    show_about: bool,
    prev_connection_panel_visible: bool,
    /// Whether the app is in locked state
    locked: bool,
    /// Lock screen password input field
    lock_password_input: String,
    /// Password error message
    lock_error_message: String,
    /// Last user activity time
    last_activity: std::time::Instant,
    /// Close confirmation dialog
    show_close_confirm: bool,
    allow_close: bool,
    /// Close confirmation dialog while in locked state
    show_lock_close_confirm: bool,
    /// Cached credential store (avoid loading from disk every frame)
    credential_store: Option<stealthterm_config::credentials::CredentialStore>,
    /// SSH configs per tab (for server monitoring)
    ssh_configs: std::collections::HashMap<String, stealthterm_ssh::config::SshConfig>,
    /// Server monitor for the active SSH tab
    server_monitor: Option<ServerMonitor>,
    /// Tab ID that the server monitor is tracking
    monitor_tab_id: Option<String>,
}

impl StealthTermApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        let settings = Settings::load().unwrap_or_default();
        stealthterm_config::i18n::set_lang(stealthterm_config::i18n::Lang::from_code(&settings.language));
        let connections = ConnectionStore::load().unwrap_or_default();
        let theme = Theme::from_name(&settings.theme);
        theme.apply_to_egui(&cc.egui_ctx);

        // Apply modern UI style - clean and minimal
        let mut style = (*cc.egui_ctx.style()).clone();

        // Moderate corner radius
        style.visuals.window_corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);

        // Subtle shadow
        style.visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: egui::Color32::from_black_alpha(25),
        };
        style.visuals.popup_shadow = egui::epaint::Shadow {
            offset: [0, 1],
            blur: 4,
            spread: 0,
            color: egui::Color32::from_black_alpha(20),
        };

        // Keep original spacing
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.window_margin = egui::Margin::same(8);

        // Scrollbar width (used by egui ScrollArea)
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating_width = 4.0;
        style.spacing.scroll.bar_inner_margin = 1.0;
        style.spacing.scroll.bar_outer_margin = 1.0;

        cc.egui_ctx.set_style(style);

        // HiDPI support: use native system scaling
        let native_ppp = cc.egui_ctx.native_pixels_per_point().unwrap_or(1.0);
        let ppp = native_ppp.max(1.0);
        cc.egui_ctx.set_pixels_per_point(ppp);

        let font_size = settings.font_size;

        // Initialize tokio runtime — stored in struct so it lives as long as the app
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        // Enter runtime context so tokio::spawn works during construction
        let _guard = runtime.enter();

        let locked = settings.has_access_password();

        let mut app = Self {
            runtime,
            settings,
            connections,
            theme,
            tab_bar: TabBar::new(),
            sidebar: Sidebar::new(),
            terminals: std::collections::HashMap::new(),
            sftp_sessions: std::collections::HashMap::new(),
            connection_panel: ConnectionPanel::new(),
            settings_panel: SettingsPanel::new(),
            batch_mode: false,
            batch_selected_tabs: std::collections::HashSet::new(),
            batch_select_visible: false,
            command_palette: CommandPalette::new(),
            split_pane: None,
            split_secondary_id: None,
            events: Vec::new(),
            font_size,
            is_fullscreen: false,
            start_time: std::time::Instant::now(),
            show_main_menu: false,
            main_menu_pos: egui::Pos2::ZERO,
            show_split_menu: false,
            split_menu_pos: egui::Pos2::ZERO,
            split_focus_secondary: false,
            show_about: false,
            prev_connection_panel_visible: false,
            locked,
            lock_password_input: String::new(),
            lock_error_message: String::new(),
            last_activity: std::time::Instant::now(),
            show_close_confirm: false,
            show_lock_close_confirm: false,
            allow_close: false,
            credential_store: stealthterm_config::credentials::CredentialStore::load().ok(),
            ssh_configs: std::collections::HashMap::new(),
            server_monitor: None,
            monitor_tab_id: None,
        };

        // Auto-open local terminal when not locked; defer until unlock when locked
        if !locked {
            app.open_local_tab();
        }

        app
    }

    fn open_local_tab(&mut self) {
        let id = Uuid::new_v4().to_string();
        let tab = Tab::new_local(&id);
        self.tab_bar.add_tab(tab);
        let mut panel = TerminalPanel::new(80, 24, self.font_size);
        panel.spawn_local_shell();
        self.terminals.insert(id, panel);
    }

    fn open_ssh_tab(&mut self, conn: &stealthterm_config::connections::ConnectionConfig, password: Option<&str>) {
        let id = Uuid::new_v4().to_string();
        let tab = Tab::new_ssh(&id, &conn.name);
        self.tab_bar.add_tab(tab);
        let mut panel = TerminalPanel::new(80, 24, self.font_size);
        let passphrase = self.credential_store.as_ref()
            .and_then(|cs| cs.get(&format!("key_passphrase:{}", conn.id)).map(|s| s.to_string()));
        let ssh_config = Self::to_ssh_config(conn, password, passphrase.as_deref());
        self.ssh_configs.insert(id.clone(), ssh_config.clone());
        let sftp_slot = Arc::new(Mutex::new(None));
        self.sftp_sessions.insert(id.clone(), sftp_slot.clone());
        panel.spawn_ssh_session(ssh_config, sftp_slot);
        self.terminals.insert(id, panel);
    }

    fn to_ssh_config(conn: &stealthterm_config::connections::ConnectionConfig, password: Option<&str>, passphrase: Option<&str>) -> stealthterm_ssh::config::SshConfig {
        use stealthterm_config::connections::AuthMethod;
        use stealthterm_ssh::config::{SshAuth, SshConfig};

        let auth = match &conn.auth {
            AuthMethod::Password => SshAuth::Password(password.unwrap_or("").to_string()),
            AuthMethod::PublicKey { key_path } => SshAuth::PublicKey {
                key_path: key_path.clone(),
                passphrase: passphrase.map(|s| s.to_string()),
            },
        };

        SshConfig {
            host: conn.host.clone(),
            port: conn.port,
            username: conn.username.clone(),
            auth,
            terminal_type: conn.terminal_type.clone(),
            keepalive_secs: conn.keepalive_interval,
            proxy_jump: None,
        }
    }

    /// Update tab connection status from terminal panel backends
    fn sync_tab_status(&mut self) {
        for tab in &mut self.tab_bar.tabs {
            if let Some(panel) = self.terminals.get(&tab.id) {
                tab.is_connected = panel.is_connected();
            }
        }
    }

    fn switch_theme(&mut self, name: &str, ctx: &Context) {
        self.theme = Theme::from_name(name);
        self.theme.apply_to_egui(ctx);
        self.settings.theme = name.to_string();
        let _ = self.settings.save();
    }

    fn handle_split(&mut self, direction: SplitDirection) {
        if self.split_pane.is_some() {
            // Already split — close the split
            self.split_pane = None;
            self.split_secondary_id = None;
            self.split_focus_secondary = false;
        } else {
            // Split screen: active tab + next tab (cyclic)
            if let Some(active_id) = &self.tab_bar.active_tab {
                // Find the position of the active tab among terminal tabs
                let terminal_tab_ids: Vec<String> = self.tab_bar.tabs.iter()
                    .filter(|t| self.terminals.contains_key(&t.id))
                    .map(|t| t.id.clone())
                    .collect();
                if terminal_tab_ids.len() < 2 {
                    return; // need at least 2 terminal tabs to split
                }
                if let Some(idx) = terminal_tab_ids.iter().position(|id| id == active_id) {
                    let next_idx = (idx + 1) % terminal_tab_ids.len();
                    let sec_id = terminal_tab_ids[next_idx].clone();
                    self.split_pane = Some(SplitPane::new(direction));
                    self.split_secondary_id = Some(sec_id);
                }
            }
        }
    }

    fn handle_global_keys(&mut self, ctx: &Context) {
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::CTRL, Key::T) {
                self.events.push(AppEvent::NewLocalTab);
            }
            if i.consume_key(Modifiers::CTRL, Key::N) {
                self.events.push(AppEvent::NewConnection);
            }
            if i.consume_key(Modifiers::CTRL, Key::W) {
                if let Some(id) = self.tab_bar.active_tab.clone() {
                    self.events.push(AppEvent::CloseTab(id));
                }
            }
            if i.consume_key(Modifiers::CTRL, Key::B) {
                self.events.push(AppEvent::ToggleSidebar);
            }
            if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::F) {
                self.events.push(AppEvent::ToggleSearch);
            }
            if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::P) {
                self.events.push(AppEvent::CommandPalette);
            }
            if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::D) {
                self.events.push(AppEvent::SplitHorizontal);
            }
            if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::R) {
                self.events.push(AppEvent::SplitVertical);
            }
            if i.consume_key(Modifiers::CTRL, Key::Equals) {
                self.events.push(AppEvent::FontIncrease);
            }
            if i.consume_key(Modifiers::CTRL, Key::Minus) {
                self.events.push(AppEvent::FontDecrease);
            }
            if i.consume_key(Modifiers::CTRL, Key::Num0) {
                self.events.push(AppEvent::FontReset);
            }
            if i.consume_key(Modifiers::NONE, Key::F11) {
                self.events.push(AppEvent::Fullscreen);
            }
        });
    }

    fn process_events(&mut self, ctx: &Context) {
        let events: Vec<AppEvent> = self.events.drain(..).collect();
        for event in events {
            match event {
                AppEvent::NewLocalTab => self.open_local_tab(),
                AppEvent::NewSshTab(config) => {
                    self.open_ssh_tab(&config, None);
                }
                AppEvent::CloseTab(id) => {
                    self.tab_bar.close_tab(&id);
                    self.terminals.remove(&id);
                    self.sftp_sessions.remove(&id);
                    self.ssh_configs.remove(&id);
                    if self.monitor_tab_id.as_ref() == Some(&id) {
                        self.server_monitor = None;
                        self.monitor_tab_id = None;
                    }

                    // If the closed tab is the active tab, clear the sidebar sftp_slot
                    if self.tab_bar.active_tab.as_ref() != Some(&id) {
                        // Active tab has switched to another tab, check if there is still an SSH connection
                        if let Some(active_id) = &self.tab_bar.active_tab {
                            if !self.sftp_sessions.contains_key(active_id) {
                                self.sidebar.sftp_slot = None;
                            }
                        } else {
                            // No active tab left, clear
                            self.sidebar.sftp_slot = None;
                        }
                    }

                    // If the closed tab was the split secondary, close split
                    if self.split_secondary_id.as_ref() == Some(&id) {
                        self.split_pane = None;
                        self.split_secondary_id = None;
                    }
                }
                AppEvent::CloseAllTabs => {
                    let tab_ids: Vec<String> = self.tab_bar.tabs.iter().map(|t| t.id.clone()).collect();
                    for id in tab_ids {
                        self.tab_bar.close_tab(&id);
                        self.terminals.remove(&id);
                        self.sftp_sessions.remove(&id);
                    }
                    self.sidebar.sftp_slot = None;
                    self.split_pane = None;
                    self.split_secondary_id = None;
                }
                AppEvent::CloseOtherTabs(keep_id) => {
                    let to_close: Vec<String> = self.tab_bar.tabs.iter()
                        .filter(|t| t.id != keep_id)
                        .map(|t| t.id.clone()).collect();
                    for id in to_close {
                        self.tab_bar.close_tab(&id);
                        self.terminals.remove(&id);
                        self.sftp_sessions.remove(&id);
                        if self.split_secondary_id.as_ref() == Some(&id) {
                            self.split_pane = None;
                            self.split_secondary_id = None;
                        }
                    }
                    self.tab_bar.active_tab = Some(keep_id);
                }
                AppEvent::CloseTabsToTheRight(ref_id) => {
                    let pos = self.tab_bar.tabs.iter().position(|t| t.id == ref_id);
                    if let Some(idx) = pos {
                        let to_close: Vec<String> = self.tab_bar.tabs.iter()
                            .skip(idx + 1)
                            .map(|t| t.id.clone()).collect();
                        for id in to_close {
                            self.tab_bar.close_tab(&id);
                            self.terminals.remove(&id);
                            self.sftp_sessions.remove(&id);
                            if self.split_secondary_id.as_ref() == Some(&id) {
                                self.split_pane = None;
                                self.split_secondary_id = None;
                            }
                        }
                    }
                }
                AppEvent::CloseTabsToTheLeft(ref_id) => {
                    let pos = self.tab_bar.tabs.iter().position(|t| t.id == ref_id);
                    if let Some(idx) = pos {
                        let to_close: Vec<String> = self.tab_bar.tabs.iter()
                            .take(idx)
                            .map(|t| t.id.clone()).collect();
                        for id in to_close {
                            self.tab_bar.close_tab(&id);
                            self.terminals.remove(&id);
                            self.sftp_sessions.remove(&id);
                            if self.split_secondary_id.as_ref() == Some(&id) {
                                self.split_pane = None;
                                self.split_secondary_id = None;
                            }
                        }
                        self.tab_bar.active_tab = Some(ref_id);
                    }
                }
                AppEvent::ActivateTab(id) => {
                    self.tab_bar.active_tab = Some(id);
                    // Close split when switching tabs
                    self.split_pane = None;
                    self.split_secondary_id = None;
                }
                AppEvent::NextTab => self.tab_bar.next_tab(),
                AppEvent::PrevTab => self.tab_bar.prev_tab(),
                AppEvent::SplitHorizontal => {
                    self.handle_split(SplitDirection::Horizontal);
                }
                AppEvent::SplitVertical => {
                    self.handle_split(SplitDirection::Vertical);
                }
                AppEvent::ToggleSidebar => self.sidebar.toggle(),
                AppEvent::ToggleSftp => {
                    // SFTP panel removed - use sidebar file manager instead
                }
                AppEvent::ToggleBatchMode => {
                    if self.batch_mode {
                        // Exit batch mode
                        self.batch_mode = false;
                        self.batch_selected_tabs.clear();
                    } else {
                        // Open selection window
                        self.batch_select_visible = true;
                        // Default: select all SSH terminal tabs
                        self.batch_selected_tabs = self.tab_bar.tabs.iter()
                            .filter(|t| matches!(t.tab_type, stealthterm_ui::widgets::tab_bar::TabType::SshSession))
                            .map(|t| t.id.clone())
                            .collect();
                    }
                }
                AppEvent::ToggleSearch => {
                    if let Some(id) = &self.tab_bar.active_tab.clone() {
                        if let Some(panel) = self.terminals.get_mut(id) {
                            panel.search_bar.open();
                        }
                    }
                }
                AppEvent::FontIncrease => {
                    self.font_size = (self.font_size + 1.0).min(32.0).round();
                    for panel in self.terminals.values_mut() {
                        panel.view.font_size = self.font_size;
                        panel.view.cached_font_size = 0.0; // force recalc from font metrics
                    }
                }
                AppEvent::FontDecrease => {
                    self.font_size = (self.font_size - 1.0).max(8.0).round();
                    for panel in self.terminals.values_mut() {
                        panel.view.font_size = self.font_size;
                        panel.view.cached_font_size = 0.0;
                    }
                }
                AppEvent::FontReset => {
                    self.font_size = self.settings.font_size.round();
                    for panel in self.terminals.values_mut() {
                        panel.view.font_size = self.font_size;
                        panel.view.cached_font_size = 0.0;
                    }
                }
                AppEvent::Fullscreen => {
                    self.is_fullscreen = !self.is_fullscreen;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.is_fullscreen));
                }
                AppEvent::CommandPalette => {
                    self.command_palette.toggle();
                }
                AppEvent::NewConnection => {
                    self.connection_panel.available_groups = self.connections.groups()
                        .iter().map(|s| s.to_string()).collect();
                    self.connection_panel.open_for_new();
                }
                AppEvent::NewConnectionInGroup(group) => {
                    self.connection_panel.available_groups = self.connections.groups()
                        .iter().map(|s| s.to_string()).collect();
                    self.connection_panel.open_for_new_in_group(group);
                }
                AppEvent::OpenConnection(id) => {
                    if let Some(conn) = self.connections.find_by_id(&id).cloned() {
                        // Read password from encrypted store
                        let password = if let Ok(creds) = stealthterm_config::credentials::CredentialStore::load() {
                            creds.get(&conn.id).map(|s| s.to_string())
                        } else {
                            None
                        };
                        self.open_ssh_tab(&conn, password.as_deref());
                    }
                }
                AppEvent::EditConnection(id) => {
                    if let Some(conn) = self.connections.find_by_id(&id).cloned() {
                        self.connection_panel.available_groups = self.connections.groups()
                            .iter().map(|s| s.to_string()).collect();
                        self.connection_panel.open_for_edit(conn);
                    }
                }
                AppEvent::DeleteConnection(id) => {
                    self.connections.remove(&id);
                    let _ = self.connections.save();
                }
                AppEvent::PasteConnection => {
                    if let Some(src_id) = self.sidebar.copied_connection_id.clone() {
                        if let Some(src) = self.connections.find_by_id(&src_id).cloned() {
                            let mut copy = src;
                            copy.id = uuid::Uuid::new_v4().to_string();
                            copy.name = format!("{}{}", copy.name, stealthterm_config::i18n::t("misc.copy_suffix"));
                            self.connections.add(copy);
                            let _ = self.connections.save();
                        }
                    }
                }
                AppEvent::DuplicateSshTab(tab_id) => {
                    // Find the SSH host info of the original tab
                    if let Some(tab) = self.tab_bar.tabs.iter().find(|t| t.id == tab_id) {
                        if let Some(host) = &tab.ssh_host {
                            // Look up the corresponding connection config in the connection list
                            if let Some(conn) = self.connections.connections.iter().find(|c| &c.host == host).cloned() {
                                let password = if let Ok(creds) = stealthterm_config::credentials::CredentialStore::load() {
                                    creds.get(&conn.id).map(|s| s.to_string())
                                } else {
                                    None
                                };
                                self.open_ssh_tab(&conn, password.as_deref());
                            }
                        }
                    }
                }
                AppEvent::OpenLocalTerminal => {
                    self.open_local_tab();
                }
                AppEvent::OpenSettings => {
                    self.settings_panel.visible = true;
                }
                AppEvent::ToggleMainMenu => {
                    self.show_main_menu = !self.show_main_menu;
                }
                AppEvent::ShowAbout => {
                    self.show_about = true;
                    self.show_main_menu = false;
                }
            }
        }
    }

    fn show_lock_screen(&mut self, ctx: &Context) {
        let title_bar_height = 38.0;
        let sidebar_bg = self.theme.sidebar_bg;

        // Title bar with icon, name, and window controls
        TopBottomPanel::top("lock_title_bar")
            .exact_height(title_bar_height)
            .frame(egui::Frame::none().fill(sidebar_bg))
            .show(ctx, |ui| {
                let title_rect = ui.max_rect();

                // Drag to move window
                let drag_resp = ui.interact(title_rect, ui.id().with("lock_title_drag"), egui::Sense::click_and_drag());
                if drag_resp.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag_resp.double_clicked() {
                    let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                }

                ui.allocate_ui_at_rect(title_rect, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;

                        // Left: icon + name
                        ui.add_space(10.0);
                        {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                            if ui.is_rect_visible(rect) {
                                let svg_bytes: &[u8] = include_bytes!("../../../assets/icon.svg");
                                let source = egui::ImageSource::Bytes {
                                    uri: "stealthterm_icon.svg".into(),
                                    bytes: egui::load::Bytes::Static(svg_bytes),
                                };
                                egui::Image::new(source).fit_to_exact_size(rect.size()).paint_at(ui, rect);
                            }
                        }
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("StealthTerm").size(13.0).color(Color32::from_rgb(0x00, 0x96, 0xD6)));

                        // Right: window controls
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let wc_btn = egui::vec2(46.0, title_bar_height);

                            // Close button
                            {
                                let (rect, resp) = ui.allocate_exact_size(wc_btn, egui::Sense::click());
                                let c = rect.center();
                                let s = 5.0;
                                if resp.hovered() {
                                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(0xe8, 0x11, 0x23));
                                    ui.painter().line_segment([c - egui::vec2(s, s), c + egui::vec2(s, s)], Stroke::new(1.2, Color32::WHITE));
                                    ui.painter().line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], Stroke::new(1.2, Color32::WHITE));
                                } else {
                                    let col = Color32::from_gray(0x70);
                                    ui.painter().line_segment([c - egui::vec2(s, s), c + egui::vec2(s, s)], Stroke::new(1.2, col));
                                    ui.painter().line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], Stroke::new(1.2, col));
                                }
                                if resp.clicked() {
                                    self.show_lock_close_confirm = true;
                                }
                            }

                            // Maximize/Restore button
                            {
                                let (rect, resp) = ui.allocate_exact_size(wc_btn, egui::Sense::click());
                                let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                                if resp.hovered() {
                                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x30));
                                }
                                let c = rect.center();
                                let col = Color32::from_gray(0x70);
                                if is_maximized {
                                    let s = 4.0;
                                    let off = 2.0;
                                    ui.painter().rect_stroke(egui::Rect::from_min_size(c + egui::vec2(-s + off, -s), egui::vec2(s * 2.0 - off, s * 2.0 - off)), 0.0, Stroke::new(1.2, col), StrokeKind::Middle);
                                    ui.painter().rect_filled(egui::Rect::from_min_size(c + egui::vec2(-s, -s + off), egui::vec2(s * 2.0 - off, s * 2.0 - off)), 0.0, sidebar_bg);
                                    ui.painter().rect_stroke(egui::Rect::from_min_size(c + egui::vec2(-s, -s + off), egui::vec2(s * 2.0 - off, s * 2.0 - off)), 0.0, Stroke::new(1.2, col), StrokeKind::Middle);
                                } else {
                                    let s = 5.0;
                                    ui.painter().rect_stroke(egui::Rect::from_center_size(c, egui::vec2(s * 2.0, s * 2.0)), 0.0, Stroke::new(1.2, col), StrokeKind::Middle);
                                }
                                if resp.clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                                }
                            }

                            // Minimize button
                            {
                                let (rect, resp) = ui.allocate_exact_size(wc_btn, egui::Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x30));
                                }
                                let c = rect.center();
                                let col = Color32::from_gray(0x70);
                                ui.painter().line_segment([c - egui::vec2(5.0, 0.0), c + egui::vec2(5.0, 0.0)], Stroke::new(1.2, col));
                                if resp.clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                                }
                            }
                        });
                    });
                });
            });

        // Lock screen close confirmation dialog
        if self.show_lock_close_confirm {
            egui::Window::new(stealthterm_config::i18n::t("close.title"))
                .collapsible(false)
                .resizable(false)
                .fixed_size([400.0, 200.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        egui_twemoji::EmojiLabel::new(egui::RichText::new("⚠").size(64.0)).show(ui);
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new(stealthterm_config::i18n::t("close.warning")).size(16.0));
                    });
                    ui.add_space(20.0);
                    let btn_w: f32 = 90.0;
                    let btn_h: f32 = 36.0;
                    let gap: f32 = 40.0;
                    let total_w = btn_w * 2.0 + gap;
                    let left_pad = (ui.available_width() - total_w) / 2.0;
                    ui.horizontal(|ui| {
                        ui.add_space(left_pad.max(0.0));
                        if ui.add_sized([btn_w, btn_h], egui::Button::new(egui::RichText::new(stealthterm_config::i18n::t("close.ok")).size(16.0))).clicked() {
                            self.show_lock_close_confirm = false;
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        ui.add_space(gap);
                        if ui.add_sized([btn_w, btn_h], egui::Button::new(egui::RichText::new(stealthterm_config::i18n::t("close.cancel")).size(16.0))).clicked() {
                            self.show_lock_close_confirm = false;
                        }
                    });
                });
        }

        // Main lock screen content
        CentralPanel::default().show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 0.0, Color32::WHITE);

            ui.allocate_ui_at_rect(rect, |ui| {
                ui.vertical_centered(|ui| {
                    let center_y = rect.center().y - 150.0;
                    ui.add_space(center_y.max(0.0));

                    // Colored emoji 🔒 + colored gradient StealthTerm
                    ui.horizontal(|ui| {
                        let total_width = 56.0 + 8.0 + 200.0;
                        let avail = ui.available_width();
                        if avail > total_width {
                            ui.add_space((avail - total_width) / 2.0);
                        }
                        egui_twemoji::EmojiLabel::new(egui::RichText::new("🔒").size(56.0)).show(ui);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("StealthTerm")
                                .size(56.0)
                                .color(Color32::from_rgb(0x00, 0x96, 0xD6))
                                .strong()
                        );
                    });
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new(stealthterm_config::i18n::t("lock.enter_password")).size(28.0).color(Color32::from_rgb(0x66, 0x66, 0x66)));
                    ui.add_space(40.0);

                    let input = egui::TextEdit::singleline(&mut self.lock_password_input)
                        .password(true)
                        .desired_width(500.0)
                        .font(egui::FontId::proportional(28.0))
                        .hint_text(stealthterm_config::i18n::t("lock.password_hint"));
                    let resp = ui.add(input);

                    if !resp.has_focus() && self.lock_error_message.is_empty() && self.lock_password_input.is_empty() {
                        resp.request_focus();
                    }

                    let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

                    ui.add_space(20.0);
                    let unlock_clicked = ui.button(egui::RichText::new(stealthterm_config::i18n::t("lock.unlock")).size(28.0)).clicked();

                    if enter_pressed || unlock_clicked {
                        if self.settings.verify_access_password(&self.lock_password_input) {
                            self.locked = false;
                            self.lock_password_input.clear();
                            self.lock_error_message.clear();
                            if self.tab_bar.tabs.is_empty() {
                                self.events.push(AppEvent::OpenLocalTerminal);
                            }
                            self.last_activity = std::time::Instant::now();
                        } else {
                            self.lock_error_message = stealthterm_config::i18n::t("lock.wrong_password").to_string();
                            self.lock_password_input.clear();
                        }
                    }

                    if !self.lock_error_message.is_empty() {
                        ui.add_space(16.0);
                        ui.label(egui::RichText::new(&self.lock_error_message).size(26.0).color(Color32::from_rgb(255, 80, 80)));
                    }
                });
            });
        });
    }
}

impl eframe::App for StealthTermApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Enter tokio runtime context for this frame
        let _guard = self.runtime.enter();

        // Intercept window close: show confirmation dialog
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.locked {
                self.show_lock_close_confirm = true;
            } else {
                self.show_close_confirm = true;
            }
        }

        // Detect user activity (mouse move, click, keyboard events)
        let has_activity = ctx.input(|i| {
            i.pointer.is_moving() || i.pointer.any_pressed() || i.events.iter().any(|e| {
                matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Ime(_))
            })
        });
        if has_activity {
            self.last_activity = std::time::Instant::now();
        }

        // Auto-lock on idle
        if !self.locked && self.settings.has_access_password() && self.settings.auto_lock_minutes > 0 {
            let idle_secs = self.last_activity.elapsed().as_secs();
            if idle_secs >= self.settings.auto_lock_minutes as u64 * 60 {
                self.locked = true;
                self.lock_password_input.clear();
                self.lock_error_message.clear();
            }
        }

        // Locked state: show only lock screen, block all other UI
        if self.locked {
            self.show_lock_screen(ctx);
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }

        self.handle_global_keys(ctx);
        self.process_events(ctx);

        // Sync tab connection indicators from backend state
        self.sync_tab_status();

        let theme = self.theme.clone();

        // ============================================================
        // Top: custom title bar (no system decorations)
        // Layout:
        //   macOS:         [traffic lights] [icon + "StealthTerm" + tab] ... [toolbar]
        //   Windows/Linux: [icon + "StealthTerm" + tab] ... [toolbar] [window controls]
        // ============================================================
        let title_bar_height = 38.0;
        TopBottomPanel::top("title_bar")
            .exact_height(title_bar_height)
            .frame(egui::Frame::none().fill(theme.sidebar_bg))
            .show(ctx, |ui| {
            let title_rect = ui.max_rect();

            // Window drag: entire title bar is draggable
            let drag_resp = ui.interact(title_rect, ui.id().with("title_drag"), egui::Sense::click_and_drag());
            if drag_resp.dragged() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag_resp.double_clicked() {
                let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
            }

            ui.allocate_ui_at_rect(title_rect, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;

                // === macOS: traffic light buttons on the left ===
                if cfg!(target_os = "macos") {
                    ui.add_space(12.0);
                    let r = 6.0; // circle radius
                    let gap = 8.0;
                    let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    let btn_size = egui::vec2(r * 2.0 + 4.0, r * 2.0 + 4.0);

                    // Close (red)
                    {
                        let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
                        let c = rect.center();
                        let base_color = Color32::from_rgb(0xFF, 0x5F, 0x57);
                        ui.painter().circle_filled(c, r, base_color);
                        if resp.hovered() {
                            // × icon
                            let s = 3.0;
                            let stroke = Stroke::new(1.5, Color32::from_rgba_premultiplied(0x40, 0x00, 0x00, 0xD0));
                            ui.painter().line_segment([c - egui::vec2(s, s), c + egui::vec2(s, s)], stroke);
                            ui.painter().line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], stroke);
                        }
                        if resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    ui.add_space(gap);
                    // Minimize (yellow)
                    {
                        let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
                        let c = rect.center();
                        let base_color = Color32::from_rgb(0xFE, 0xBC, 0x2E);
                        ui.painter().circle_filled(c, r, base_color);
                        if resp.hovered() {
                            // − icon
                            let stroke = Stroke::new(1.5, Color32::from_rgba_premultiplied(0x60, 0x40, 0x00, 0xD0));
                            ui.painter().line_segment([c - egui::vec2(4.0, 0.0), c + egui::vec2(4.0, 0.0)], stroke);
                        }
                        if resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    }
                    ui.add_space(gap);
                    // Maximize/Fullscreen (green)
                    {
                        let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
                        let c = rect.center();
                        let base_color = Color32::from_rgb(0x28, 0xC8, 0x40);
                        ui.painter().circle_filled(c, r, base_color);
                        if resp.hovered() {
                            // diagonal arrows icon (↗↙)
                            let s = 3.5;
                            let stroke = Stroke::new(1.5, Color32::from_rgba_premultiplied(0x00, 0x40, 0x00, 0xD0));
                            // ↗
                            ui.painter().line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], stroke);
                            // small arrow heads
                            ui.painter().line_segment([c + egui::vec2(s, -s), c + egui::vec2(s - 2.5, -s)], stroke);
                            ui.painter().line_segment([c + egui::vec2(s, -s), c + egui::vec2(s, -s + 2.5)], stroke);
                        }
                        if resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                        }
                    }
                    ui.add_space(8.0);
                }

                // === Left: App icon + name + active tab title ===
                ui.add_space(10.0);
                // App icon (logo)
                {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        let svg_bytes: &[u8] = include_bytes!("../../../assets/icon.svg");
                        let source = egui::ImageSource::Bytes {
                            uri: "stealthterm_icon.svg".into(),
                            bytes: egui::load::Bytes::Static(svg_bytes),
                        };
                        egui::Image::new(source).fit_to_exact_size(rect.size()).paint_at(ui, rect);
                    }
                }
                ui.add_space(8.0);
                // "StealthTerm" label
                ui.label(egui::RichText::new("StealthTerm").size(13.0).color(Color32::from_rgb(0x00, 0x96, 0xD6)));
                // Active tab name
                if let Some(active_id) = &self.tab_bar.active_tab {
                    if let Some(tab) = self.tab_bar.tabs.iter().find(|t| &t.id == active_id) {
                        ui.label(egui::RichText::new(" -- ").size(13.0).color(Color32::from_gray(0x60)));
                        ui.label(egui::RichText::new(&tab.title).size(13.0).color(Color32::from_gray(0xb0)));
                    }
                }

                // === Right side: toolbar + window controls ===
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    // --- Window controls: Windows/Linux only (rightmost) ---
                    if !cfg!(target_os = "macos") {
                    let wc_btn = egui::vec2(46.0, title_bar_height);

                    // Close button
                    {
                        let (rect, resp) = ui.allocate_exact_size(wc_btn, egui::Sense::click());
                        let c = rect.center();
                        let s = 5.0;
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(0xe8, 0x11, 0x23));
                            ui.painter().line_segment([c - egui::vec2(s, s), c + egui::vec2(s, s)], Stroke::new(1.2, Color32::WHITE));
                            ui.painter().line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], Stroke::new(1.2, Color32::WHITE));
                        } else {
                            let col = Color32::from_gray(0x70);
                            ui.painter().line_segment([c - egui::vec2(s, s), c + egui::vec2(s, s)], Stroke::new(1.2, col));
                            ui.painter().line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], Stroke::new(1.2, col));
                        }
                        if resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }

                    // Maximize/Restore button
                    {
                        let (rect, resp) = ui.allocate_exact_size(wc_btn, egui::Sense::click());
                        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x30));
                        }
                        let c = rect.center();
                        let col = Color32::from_gray(0x70);
                        if is_maximized {
                            let s = 4.0;
                            let off = 2.0;
                            // Back rect
                            ui.painter().rect_stroke(egui::Rect::from_min_size(c + egui::vec2(-s + off, -s), egui::vec2(s * 2.0 - off, s * 2.0 - off)), 0.0, Stroke::new(1.2, col), StrokeKind::Middle);
                            // Front rect
                            ui.painter().rect_filled(egui::Rect::from_min_size(c + egui::vec2(-s, -s + off), egui::vec2(s * 2.0 - off, s * 2.0 - off)), 0.0, theme.sidebar_bg);
                            ui.painter().rect_stroke(egui::Rect::from_min_size(c + egui::vec2(-s, -s + off), egui::vec2(s * 2.0 - off, s * 2.0 - off)), 0.0, Stroke::new(1.2, col), StrokeKind::Middle);
                        } else {
                            let s = 5.0;
                            ui.painter().rect_stroke(egui::Rect::from_center_size(c, egui::vec2(s * 2.0, s * 2.0)), 0.0, Stroke::new(1.2, col), StrokeKind::Middle);
                        }
                        if resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                        }
                    }

                    // Minimize button
                    {
                        let (rect, resp) = ui.allocate_exact_size(wc_btn, egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x30));
                        }
                        let c = rect.center();
                        let col = Color32::from_gray(0x70);
                        ui.painter().line_segment([c - egui::vec2(5.0, 0.0), c + egui::vec2(5.0, 0.0)], Stroke::new(1.2, col));
                        if resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    }

                    // --- Separator between window controls and toolbar ---
                    // (only shown on Windows/Linux where controls are on the right)
                    if !cfg!(target_os = "macos") {
                    ui.add_space(24.0);
                    {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
                        ui.painter().line_segment(
                            [rect.center_top(), rect.center_bottom()],
                            Stroke::new(1.0, Color32::from_gray(0xd0)),
                        );
                    }
                    ui.add_space(24.0);
                    } // end separator if
                    } // end window controls if (not macos)

                    // --- Toolbar buttons (left of window controls) ---
                    let tb_btn = egui::vec2(32.0, 28.0);
                    let tb_gap = 6.0; // uniform gap between toolbar buttons
                    let icon_color = Color32::from_gray(0x60);

                    // Three-dot menu
                    {
                        let (rect, resp) = ui.allocate_exact_size(tb_btn, egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 4.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x20));
                        }
                        let c = rect.center();
                        let dot_r = 1.5;
                        let sp = 5.0;
                        ui.painter().circle_filled(c - egui::vec2(0.0, sp), dot_r, icon_color);
                        ui.painter().circle_filled(c, dot_r, icon_color);
                        ui.painter().circle_filled(c + egui::vec2(0.0, sp), dot_r, icon_color);
                        if resp.clicked() {
                            self.main_menu_pos = resp.rect.left_bottom();
                            self.events.push(AppEvent::ToggleMainMenu);
                        }
                    }

                    // Settings button
                    ui.add_space(tb_gap);
                    {
                        let resp = stealthterm_ui::widgets::emoji_button::emoji_button(ui, "⚙", tb_btn);
                        if resp.hovered() {
                            ui.painter().rect_filled(resp.rect, 4.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x20));
                        }
                        if resp.clicked() {
                            self.events.push(AppEvent::OpenSettings);
                        }
                        resp.on_hover_text(stealthterm_config::i18n::t("toolbar.settings"));
                    }

                    // Local terminal button
                    ui.add_space(tb_gap);
                    {
                        let resp = stealthterm_ui::widgets::emoji_button::emoji_button(ui, "🖥", tb_btn);
                        if resp.hovered() {
                            ui.painter().rect_filled(resp.rect, 4.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x20));
                        }
                        if resp.clicked() {
                            self.events.push(AppEvent::OpenLocalTerminal);
                        }
                        resp.on_hover_text(stealthterm_config::i18n::t("toolbar.open_local_terminal"));
                    }

                    // Split screen button
                    ui.add_space(tb_gap);
                    let is_split = self.split_pane.is_some();
                    {
                        let (rect, split_resp) = ui.allocate_exact_size(tb_btn, egui::Sense::click());
                        if split_resp.hovered() {
                            ui.painter().rect_filled(rect, 4.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x20));
                        }
                        let split_icon_color = if is_split {
                            Color32::from_rgb(0x50, 0xfa, 0x7b)
                        } else {
                            icon_color
                        };
                        let c = rect.center();
                        let s = 6.0;
                        let stroke = Stroke::new(1.2, split_icon_color);
                        ui.painter().rect_stroke(egui::Rect::from_center_size(c, egui::vec2(s * 2.0, s * 2.0)), 1.0, stroke, StrokeKind::Middle);
                        ui.painter().line_segment([c - egui::vec2(0.0, s), c + egui::vec2(0.0, s)], stroke);

                        if split_resp.clicked() {
                            if is_split {
                                self.events.push(AppEvent::SplitHorizontal);
                            } else {
                                self.show_split_menu = !self.show_split_menu;
                                self.split_menu_pos = split_resp.rect.left_bottom();
                            }
                        }
                        split_resp.on_hover_text(if is_split { stealthterm_config::i18n::t("toolbar.close_split") } else { stealthterm_config::i18n::t("toolbar.split_screen") });
                    }

                    // Batch execute button
                    ui.add_space(tb_gap);
                    {
                        let resp = stealthterm_ui::widgets::emoji_button::emoji_button(ui, "📡", tb_btn);
                        let rect = resp.rect;
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 4.0, Color32::from_rgba_premultiplied(0x80, 0x80, 0x80, 0x20));
                        }
                        if resp.clicked() {
                            self.events.push(AppEvent::ToggleBatchMode);
                        }
                        if self.batch_mode {
                            let label_rect = egui::Rect::from_min_size(
                                rect.left_center() - egui::vec2(36.0, 8.0),
                                egui::vec2(32.0, 16.0),
                            );
                            ui.painter().text(label_rect.center(), egui::Align2::CENTER_CENTER, stealthterm_config::i18n::t("toolbar.batch_active"), egui::FontId::proportional(11.0), Color32::from_rgb(0xff, 0x55, 0x55));
                        }
                        resp.on_hover_text(if self.batch_mode { stealthterm_config::i18n::t("toolbar.close_batch") } else { stealthterm_config::i18n::t("toolbar.batch_mode") });
                    }
                });
            });
            });

            // Subtle bottom border
            let bottom_y = title_rect.bottom();
            ui.painter().line_segment(
                [egui::pos2(title_rect.left(), bottom_y), egui::pos2(title_rect.right(), bottom_y)],
                Stroke::new(1.0, Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x20)),
            );
        });

        // Split screen dropdown menu
        if self.show_split_menu {
            let area_resp = egui::Area::new("split_menu".into())
                .fixed_pos(self.split_menu_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(120.0);
                        if ui.button(stealthterm_config::i18n::t("split.horizontal")).clicked() {
                            self.events.push(AppEvent::SplitHorizontal);
                            self.show_split_menu = false;
                        }
                        if ui.button(stealthterm_config::i18n::t("split.vertical")).clicked() {
                            self.events.push(AppEvent::SplitVertical);
                            self.show_split_menu = false;
                        }
                    });
                });
            // Close when clicking outside the menu (use any_pressed to avoid same-frame conflict with button clicked)
            if ctx.input(|i| i.pointer.any_pressed()) && !area_resp.response.contains_pointer() {
                self.show_split_menu = false;
            }
        }

        // ============================================================
        // Sidebar (below title bar, left side)
        // ============================================================
        if self.sidebar.visible {
            if let Some(tab_id) = &self.tab_bar.active_tab {
                if let Some(sftp_slot) = self.sftp_sessions.get(tab_id) {
                    self.sidebar.sftp_slot = Some(sftp_slot.clone());
                }
                if let Some(panel) = self.terminals.get(tab_id) {
                    let remote_path = panel.current_dir.to_string_lossy().to_string();
                    self.sidebar.set_active_connection(Some(tab_id.clone()), Some(remote_path));
                }
            }

            let width = if self.sidebar.collapsed { 40.0 } else { self.sidebar.width };
            SidePanel::left("sidebar")
                .resizable(false)
                .exact_width(width)
                .show_separator_line(false)
                .frame(egui::Frame::NONE.fill(theme.sidebar_bg))
                .show(ctx, |ui| {
                    let action = self.sidebar.show(ui, &self.connections, &theme);
                    match action {
                        SidebarAction::OpenConnection(id) => self.events.push(AppEvent::OpenConnection(id)),
                        SidebarAction::NewConnection => self.events.push(AppEvent::NewConnection),
                        SidebarAction::NewConnectionInGroup(group) => self.events.push(AppEvent::NewConnectionInGroup(group)),
                        SidebarAction::EditConnection(id) => self.events.push(AppEvent::EditConnection(id)),
                        SidebarAction::DeleteConnection(id) => self.events.push(AppEvent::DeleteConnection(id)),
                        SidebarAction::CopyConnection(id) => {
                            self.sidebar.copied_connection_id = Some(id);
                        }
                        SidebarAction::PasteConnection => self.events.push(AppEvent::PasteConnection),
                        SidebarAction::None => {}
                    }
                });
        }

        // ============================================================
        // Tab bar (below title bar, right of sidebar)
        // ============================================================
        TopBottomPanel::top("tab_bar")
            .exact_height(32.0)
            .frame(egui::Frame::none().fill(theme.tab_bg))
            .show(ctx, |ui| {
                let action = self.tab_bar.show(ui, &theme);
                match action {
                    TabBarAction::NewLocal => self.events.push(AppEvent::NewLocalTab),
                    TabBarAction::Close(id) => self.events.push(AppEvent::CloseTab(id)),
                    TabBarAction::Activate(id) => self.events.push(AppEvent::ActivateTab(id)),
                    TabBarAction::NewSsh => self.events.push(AppEvent::NewConnection),
                    TabBarAction::DuplicateSsh(id) => self.events.push(AppEvent::DuplicateSshTab(id)),
                    TabBarAction::CloseAll => self.events.push(AppEvent::CloseAllTabs),
                    TabBarAction::CloseOthers(id) => self.events.push(AppEvent::CloseOtherTabs(id)),
                    TabBarAction::CloseToTheRight(id) => self.events.push(AppEvent::CloseTabsToTheRight(id)),
                    TabBarAction::CloseToTheLeft(id) => self.events.push(AppEvent::CloseTabsToTheLeft(id)),
                    TabBarAction::None => {}
                }
            });

        // Bottom: status bar
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let (connected, cols, rows) = if let Some(id) = &self.tab_bar.active_tab {
                if let Some(panel) = self.terminals.get(id) {
                    (panel.is_connected(), panel.cols, panel.rows)
                } else {
                    (false, 80, 24)
                }
            } else {
                (false, 80, 24)
            };
            let uptime_secs = self.start_time.elapsed().as_secs();

            // Update server monitor: start/stop based on active SSH tab
            let active_tab = self.tab_bar.active_tab.clone();
            let should_monitor = active_tab.as_ref()
                .and_then(|id| self.ssh_configs.get(id).cloned());

            match (&self.monitor_tab_id, &active_tab) {
                (Some(mid), Some(aid)) if mid == aid => {
                    // Same tab, just poll
                }
                _ => {
                    // Tab changed — restart monitor if SSH, or stop if not
                    self.server_monitor = None;
                    self.monitor_tab_id = None;
                    if let Some(config) = should_monitor {
                        if let Some(ref aid) = active_tab {
                            self.server_monitor = Some(ServerMonitor::start(config, ctx.clone(), &self.runtime));
                            self.monitor_tab_id = Some(aid.clone());
                        }
                    }
                }
            }

            // Poll for new stats
            if let Some(monitor) = &mut self.server_monitor {
                monitor.poll();
            }

            let server_stats = self.server_monitor.as_ref().and_then(|m| m.stats());
            StatusBar::show(ui, &theme, connected, "UTF-8", cols, rows, uptime_secs, server_stats);
        });

        // Central: active terminal (with optional split)
        CentralPanel::default()
            .frame(egui::Frame::none().fill(self.theme.bg))
            .show(ctx, |ui| {
            if let Some(tab_id) = self.tab_bar.active_tab.clone() {
                // Check if any other window is open
                let other_window_open = self.connection_panel.visible || self.batch_select_visible || self.settings_panel.visible;

                if let Some(split) = &mut self.split_pane {
                    // Split view: primary + secondary terminal
                    let sec_id = self.split_secondary_id.clone();
                    let theme_a = theme.clone();
                    let theme_b = theme.clone();
                    let focus_secondary = self.split_focus_secondary;
                    // When main panel is not focused, treat as another window open (blocks keyboard input)
                    let primary_blocked = other_window_open || focus_secondary;
                    let secondary_blocked = other_window_open || !focus_secondary;

                    // We need to extract both panels mutably
                    // Use raw pointers to avoid double-borrow (safe because IDs differ)
                    let terminals_ptr = &mut self.terminals as *mut std::collections::HashMap<String, TerminalPanel>;
                    let (rect_a, rect_b) = split.show(ui,
                        |ui| {
                            let r = ui.available_rect_before_wrap();
                            let terminals = unsafe { &mut *terminals_ptr };
                            if let Some(panel) = terminals.get_mut(&tab_id) {
                                panel.show(ui, &theme_a, primary_blocked);
                            }
                            r
                        },
                        |ui| {
                            let r = ui.available_rect_before_wrap();
                            let terminals = unsafe { &mut *terminals_ptr };
                            if let Some(sec) = &sec_id {
                                if let Some(panel) = terminals.get_mut(sec) {
                                    panel.show(ui, &theme_b, secondary_blocked);
                                }
                            }
                            r
                        },
                    );
                    // Click to switch focus
                    if let Some(pos) = ui.ctx().pointer_latest_pos() {
                        if ui.ctx().input(|i| i.pointer.primary_pressed()) {
                            if rect_a.contains(pos) && self.split_focus_secondary {
                                self.split_focus_secondary = false;
                            } else if rect_b.contains(pos) && !self.split_focus_secondary {
                                self.split_focus_secondary = true;
                            }
                        }
                    }
                } else if let Some(panel) = self.terminals.get_mut(&tab_id) {
                    panel.show(ui, &theme, other_window_open);
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(stealthterm_config::i18n::t("app.no_terminal"))
                            .color(Color32::WHITE)
                    );
                });
            }
        });

        // Batch mode: broadcast active terminal input to other selected terminals
        if self.batch_mode {
            if let Some(active_id) = &self.tab_bar.active_tab {
                if self.batch_selected_tabs.contains(active_id) {
                    let active_id = active_id.clone();
                    // Take the broadcast buffer from the active terminal
                    let buffers: Vec<Vec<u8>> = if let Some(panel) = self.terminals.get_mut(&active_id) {
                        panel.broadcast_buffer.drain(..).collect()
                    } else {
                        Vec::new()
                    };
                    // Send to other selected terminals
                    if !buffers.is_empty() {
                        for (tab_id, panel) in self.terminals.iter() {
                            if *tab_id != active_id && self.batch_selected_tabs.contains(tab_id) {
                                for data in &buffers {
                                    panel.write(data);
                                }
                            }
                        }
                    }
                }
            }
            // Clean up selected state for closed tabs
            let existing_ids: std::collections::HashSet<String> = self.tab_bar.tabs.iter().map(|t| t.id.clone()).collect();
            self.batch_selected_tabs.retain(|id| existing_ids.contains(id));
            if self.batch_selected_tabs.len() <= 1 {
                self.batch_mode = false;
                self.batch_selected_tabs.clear();
            }
        } else {
            // Also clear broadcast buffer in non-batch mode to avoid memory leak
            if let Some(active_id) = &self.tab_bar.active_tab {
                if let Some(panel) = self.terminals.get_mut(active_id) {
                    panel.broadcast_buffer.clear();
                }
            }
        }

        // Command palette overlay (renders on top of everything)
        {
            let theme_c = self.theme.clone();
            if let Some(action) = self.command_palette.show(ctx, &theme_c) {
                match action {
                    CommandAction::NewLocalTab => self.events.push(AppEvent::NewLocalTab),
                    CommandAction::NewSshConnection => self.events.push(AppEvent::NewConnection),
                    CommandAction::CloseTab => {
                        if let Some(id) = self.tab_bar.active_tab.clone() {
                            self.events.push(AppEvent::CloseTab(id));
                        }
                    }
                    CommandAction::NextTab => self.events.push(AppEvent::NextTab),
                    CommandAction::PrevTab => self.events.push(AppEvent::PrevTab),
                    CommandAction::ToggleSidebar => self.events.push(AppEvent::ToggleSidebar),
                    CommandAction::ToggleSearch => self.events.push(AppEvent::ToggleSearch),
                    CommandAction::ToggleSftp => self.events.push(AppEvent::ToggleSftp),
                    CommandAction::ToggleBatchMode => self.events.push(AppEvent::ToggleBatchMode),
                    CommandAction::SplitHorizontal => self.events.push(AppEvent::SplitHorizontal),
                    CommandAction::SplitVertical => self.events.push(AppEvent::SplitVertical),
                    CommandAction::FontIncrease => self.events.push(AppEvent::FontIncrease),
                    CommandAction::FontDecrease => self.events.push(AppEvent::FontDecrease),
                    CommandAction::FontReset => self.events.push(AppEvent::FontReset),
                    CommandAction::Fullscreen => self.events.push(AppEvent::Fullscreen),
                    CommandAction::OpenSettings => self.events.push(AppEvent::OpenSettings),
                    CommandAction::SwitchTheme(name) => self.switch_theme(&name, ctx),
                }
            }
        }

        // Floating panels
        let connection_panel_was_visible = self.prev_connection_panel_visible;
        let connection_panel_is_visible = self.connection_panel.visible;

        if self.connection_panel.visible {
            let theme_c = self.theme.clone();
            let connections_snap = self.connections.clone();
            if let Some(config) = self.connection_panel.show_ctx(ctx, &theme_c, &connections_snap, self.credential_store.as_ref()) {
                let password = self.connection_panel.take_password();
                let passphrase = self.connection_panel.take_passphrase();

                // Save password/key passphrase to encrypted store
                if let Some(ref mut creds) = self.credential_store {
                    if let Some(pwd) = &password {
                        let _ = creds.store(&config.id, pwd);
                    }
                    if let Some(pp) = &passphrase {
                        let _ = creds.store(&format!("key_passphrase:{}", config.id), pp);
                    }
                }

                self.connections.update(config.clone());
                let _ = self.connections.save();
                self.open_ssh_tab(&config, password.as_deref());
            }
        }

        // Detect connection panel close, restore terminal focus
        if connection_panel_was_visible && !connection_panel_is_visible {
            if let Some(active_id) = &self.tab_bar.active_tab {
                if self.terminals.contains_key(active_id) {
                    let terminal_id = egui::Id::new(format!("terminal_{}", active_id));
                    ctx.memory_mut(|mem| mem.request_focus(terminal_id));
                }
            }
        }

        self.prev_connection_panel_visible = connection_panel_is_visible;

        // SFTP panel removed - use sidebar file manager instead


        // Batch selection window
        if self.batch_select_visible {
            let display_names = self.tab_bar.display_names();
            // Only show SSH tabs (batch execute targets SSH sessions only)
            let terminal_tabs: Vec<_> = self.tab_bar.tabs.iter()
                .filter(|t| matches!(t.tab_type, stealthterm_ui::widgets::tab_bar::TabType::SshSession))
                .map(|t| t.id.clone())
                .collect();

            let mut open = true;
            egui::Window::new(stealthterm_config::i18n::t("batch.title"))
                .resizable(true)
                .default_width(400.0)
                .default_height(500.0)
                .collapsible(false)
                .anchor(egui::Align2::RIGHT_TOP, [-10.0, 40.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(stealthterm_config::i18n::t("batch.select_hint")).color(Color32::from_rgb(0xAB, 0xB2, 0xBF)));
                    ui.add_space(8.0);

                    // Select all / deselect all
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new(stealthterm_config::i18n::t("batch.select_all")).color(Color32::from_rgb(0xAB, 0xB2, 0xBF))).clicked() {
                            for id in &terminal_tabs {
                                self.batch_selected_tabs.insert(id.clone());
                            }
                        }
                        if ui.button(egui::RichText::new(stealthterm_config::i18n::t("batch.deselect_all")).color(Color32::from_rgb(0xAB, 0xB2, 0xBF))).clicked() {
                            self.batch_selected_tabs.clear();
                        }
                    });
                    ui.add_space(4.0);
                    ui.separator();

                    egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                        for tab_id in &terminal_tabs {
                            let display = display_names.iter()
                                .find(|(id, _)| id == tab_id)
                                .map(|(_, name)| name.as_str())
                                .unwrap_or("?");
                            let mut checked = self.batch_selected_tabs.contains(tab_id);
                            let is_connected = self.terminals.get(tab_id).map_or(false, |p| p.is_connected());
                            let icon = if is_connected { "🟢" } else { "⚪" };
                            if ui.checkbox(&mut checked, format!("{} {}", icon, display)).changed() {
                                if checked {
                                    self.batch_selected_tabs.insert(tab_id.clone());
                                } else {
                                    self.batch_selected_tabs.remove(tab_id);
                                }
                            }
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new(stealthterm_config::i18n::t("batch.confirm")).color(Color32::from_rgb(0xAB, 0xB2, 0xBF))).clicked() {
                            self.batch_mode = true;
                            self.batch_select_visible = false;
                        }
                        if ui.button(egui::RichText::new(stealthterm_config::i18n::t("batch.cancel")).color(Color32::from_rgb(0xAB, 0xB2, 0xBF))).clicked() {
                            self.batch_select_visible = false;
                            self.batch_selected_tabs.clear();
                        }
                    });
                });
            if !open {
                self.batch_select_visible = false;
            }
        }

        if self.settings_panel.visible {
            let action = self.settings_panel.show_ctx(ctx, &mut self.settings);
            match action {
                stealthterm_ui::panels::settings_panel::SettingsAction::ThemeChanged(name) => {
                    self.switch_theme(&name, ctx);
                }
                stealthterm_ui::panels::settings_panel::SettingsAction::FontSizeChanged(size) => {
                    self.font_size = size.round();
                    for panel in self.terminals.values_mut() {
                        panel.view.font_size = self.font_size;
                        panel.view.cached_font_size = 0.0;
                    }
                }
                stealthterm_ui::panels::settings_panel::SettingsAction::ClearHistory => {
                    // Clear history files for all sessions
                    let _ = stealthterm_config::EncryptedHistoryStore::clear_all_sessions();
                    // Clear in-memory history for all currently open terminals
                    for panel in self.terminals.values_mut() {
                        panel.clear_history();
                    }
                }
                stealthterm_ui::panels::settings_panel::SettingsAction::LanguageChanged(_code) => {
                    let _ = self.settings.save();
                }
                _ => {}
            }
        }

        // Main menu
        if self.show_main_menu {
            egui::Area::new("main_menu".into())
                .fixed_pos(self.main_menu_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        if ui.button(stealthterm_config::i18n::t("menu.about")).clicked() {
                            self.events.push(AppEvent::ShowAbout);
                        }
                    });
                });
        }

        // About dialog
        if self.show_about {
            let mut open = true;
            egui::Window::new(stealthterm_config::i18n::t("about.title"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("StealthTerm");
                        ui.add_space(10.0);
                        ui.label(stealthterm_config::i18n::t("about.version"));
                        ui.label(stealthterm_config::i18n::t("about.description"));
                        ui.add_space(10.0);
                        if ui.button(stealthterm_config::i18n::t("about.ok")).clicked() {
                            self.show_about = false;
                        }
                    });
                });
            if !open {
                self.show_about = false;
            }
        }

        // Close confirmation dialog
        if self.show_close_confirm {
            let ssh_count = self.tab_bar.tabs.iter()
                .filter(|t| matches!(t.tab_type, stealthterm_ui::widgets::tab_bar::TabType::SshSession))
                .count();
            egui::Window::new(stealthterm_config::i18n::t("close.title"))
                .collapsible(false)
                .resizable(false)
                .fixed_size([500.0, 300.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        egui_twemoji::EmojiLabel::new(egui::RichText::new("⚠").size(128.0)).show(ui);
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new(format!("{} {}", ssh_count, stealthterm_config::i18n::t("close.ssh_running"))).size(18.0).strong());
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(stealthterm_config::i18n::t("close.warning")).size(15.0).weak());
                    });
                    ui.add_space(20.0);
                    let btn_w: f32 = 90.0;
                    let btn_h: f32 = 36.0;
                    let gap: f32 = 40.0;
                    let total_w = btn_w * 2.0 + gap;
                    let left_pad = (ui.available_width() - total_w) / 2.0;
                    ui.horizontal(|ui| {
                        ui.add_space(left_pad.max(0.0));
                        if ui.add_sized([btn_w, btn_h], egui::Button::new(egui::RichText::new(stealthterm_config::i18n::t("close.ok")).size(16.0))).clicked() {
                            self.show_close_confirm = false;
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        ui.add_space(gap);
                        if ui.add_sized([btn_w, btn_h], egui::Button::new(egui::RichText::new(stealthterm_config::i18n::t("close.cancel")).size(16.0))).clicked() {
                            self.show_close_confirm = false;
                        }
                    });
                });
        }

        ctx.request_repaint();
    }
}
