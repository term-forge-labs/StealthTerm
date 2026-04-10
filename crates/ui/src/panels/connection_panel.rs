use egui::{Context, Ui};
use stealthterm_config::connections::{AuthMethod, ConnectionConfig, ConnectionStore};
use stealthterm_config::credentials::CredentialStore;
use stealthterm_config::i18n::t;
use crate::theme::Theme;
use std::path::PathBuf;

/// Password auto-fill suggestion
pub struct PasswordSuggestion {
    pub label: String,
    pub connection_id: String,
}

pub struct ConnectionPanel {
    pub config: ConnectionConfig,
    pub password_input: String,
    pub visible: bool,
    pub available_groups: Vec<String>,
    focus_requested: bool,
    /// Key path input
    pub key_path_input: String,
    /// Key passphrase input
    pub passphrase_input: String,
    /// Password fill candidate list
    pub password_suggestions: Vec<PasswordSuggestion>,
    /// Whether to show the password fill dropdown
    pub show_password_suggestions: bool,
}

impl ConnectionPanel {
    pub fn new() -> Self {
        Self {
            config: ConnectionConfig::default(),
            password_input: String::new(),
            visible: false,
            available_groups: Vec::new(),
            focus_requested: false,
            key_path_input: String::new(),
            passphrase_input: String::new(),
            password_suggestions: Vec::new(),
            show_password_suggestions: false,
        }
    }

    pub fn open_for_new(&mut self) {
        self.config = ConnectionConfig::default();
        self.password_input.clear();
        self.key_path_input.clear();
        self.passphrase_input.clear();
        self.password_suggestions.clear();
        self.show_password_suggestions = false;
        self.visible = true;
        self.focus_requested = true;
    }

    pub fn open_for_new_in_group(&mut self, group: String) {
        self.config = ConnectionConfig::default();
        self.config.group = Some(group);
        self.password_input.clear();
        self.key_path_input.clear();
        self.passphrase_input.clear();
        self.password_suggestions.clear();
        self.show_password_suggestions = false;
        self.visible = true;
        self.focus_requested = true;
    }

    pub fn open_for_edit(&mut self, config: ConnectionConfig) {
        if let AuthMethod::PublicKey { ref key_path } = config.auth {
            self.key_path_input = key_path.to_string_lossy().to_string();
        } else {
            self.key_path_input.clear();
        }
        self.config = config;
        self.password_input.clear();
        self.passphrase_input.clear();
        self.password_suggestions.clear();
        self.show_password_suggestions = false;
        self.visible = true;
        self.focus_requested = true;
    }

    pub fn take_password(&mut self) -> Option<String> {
        if self.password_input.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.password_input))
        }
    }

    pub fn take_passphrase(&mut self) -> Option<String> {
        if self.passphrase_input.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.passphrase_input))
        }
    }

    /// Show using &mut Ui (for embedding in panels)
    pub fn show(&mut self, ui: &mut Ui, _theme: &Theme, connections: &ConnectionStore, credentials: Option<&CredentialStore>) -> Option<ConnectionConfig> {
        self.show_window_inner(ui.ctx(), connections, credentials)
    }

    /// Show using &egui::Context directly (for floating windows from app)
    pub fn show_ctx(&mut self, ctx: &Context, _theme: &Theme, connections: &ConnectionStore, credentials: Option<&CredentialStore>) -> Option<ConnectionConfig> {
        self.show_window_inner(ctx, connections, credentials)
    }

    /// Build the password fill candidate list
    fn build_password_suggestions(
        username: &str,
        current_id: &str,
        connections: &ConnectionStore,
        credentials: Option<&CredentialStore>,
    ) -> Vec<PasswordSuggestion> {
        let Some(creds) = credentials else { return Vec::new() };
        if username.is_empty() { return Vec::new(); }

        connections.connections.iter()
            .filter(|c| c.username == username && c.id != current_id)
            .filter(|c| creds.contains(&c.id))
            .map(|c| PasswordSuggestion {
                label: format!("{}@{} ({})", c.username, c.host, c.name),
                connection_id: c.id.clone(),
            })
            .collect()
    }

    fn show_window_inner(&mut self, ctx: &Context, connections: &ConnectionStore, credentials: Option<&CredentialStore>) -> Option<ConnectionConfig> {
        if !self.visible { return None; }

        let mut result = None;
        let mut should_close = false;
        let should_focus = self.focus_requested;
        let mut open = self.visible;

        let _window_response = egui::Window::new(t("conn.title"))
            .id(egui::Id::new("ssh_connection_window"))
            .resizable(true)
            .default_width(520.0)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("conn_grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        // Name
                        ui.label(t("conn.name"));
                        let name_resp = ui.text_edit_singleline(&mut self.config.name);
                        if should_focus {
                            name_resp.request_focus();
                        }
                        ui.end_row();

                        // Host
                        ui.label(t("conn.host"));
                        ui.text_edit_singleline(&mut self.config.host);
                        ui.end_row();

                        // Port
                        ui.label(t("conn.port"));
                        let mut port_str = self.config.port.to_string();
                        if ui.text_edit_singleline(&mut port_str).changed() {
                            if let Ok(p) = port_str.parse::<u16>() {
                                self.config.port = p;
                            }
                        }
                        ui.end_row();

                        // Username
                        ui.label(t("conn.username"));
                        ui.text_edit_singleline(&mut self.config.username);
                        ui.end_row();

                        // Auth method
                        ui.label(t("conn.auth_method"));
                        let auth_label = match &self.config.auth {
                            AuthMethod::Password => t("conn.auth_password"),
                            AuthMethod::PublicKey { .. } => t("conn.auth_pubkey"),
                        };
                        egui::ComboBox::from_id_salt("auth_method")
                            .selected_text(auth_label)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(matches!(self.config.auth, AuthMethod::Password), t("conn.auth_password")).clicked() {
                                    self.config.auth = AuthMethod::Password;
                                }
                                if ui.selectable_label(matches!(self.config.auth, AuthMethod::PublicKey { .. }), t("conn.auth_pubkey")).clicked() {
                                    if !matches!(self.config.auth, AuthMethod::PublicKey { .. }) {
                                        self.config.auth = AuthMethod::PublicKey {
                                            key_path: PathBuf::from(&self.key_path_input),
                                        };
                                    }
                                }
                            });
                        ui.end_row();

                        // Public key hint (separate row, immediately below auth method)
                        if matches!(self.config.auth, AuthMethod::PublicKey { .. }) {
                            ui.label("");
                            ui.label(egui::RichText::new(t("conn.pubkey_hint")).weak().small());
                            ui.end_row();
                        }

                        // Show different fields based on auth method
                        let is_password = matches!(self.config.auth, AuthMethod::Password);
                        if is_password {
                            // Password
                            ui.label(t("conn.password"));
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut self.password_input)
                                    .password(true)
                                    .desired_width(280.0));

                                // Password fill button
                                let suggestions = Self::build_password_suggestions(
                                    &self.config.username,
                                    &self.config.id,
                                    connections,
                                    credentials,
                                );
                                let has_suggestions = !suggestions.is_empty();
                                self.password_suggestions = suggestions;

                                if ui.add_enabled(
                                    has_suggestions,
                                    egui::Button::new("🔑").min_size(egui::vec2(28.0, 20.0)),
                                ).on_hover_text_at_pointer(t("conn.fill_password")).clicked() {
                                    self.show_password_suggestions = !self.show_password_suggestions;
                                }
                            });
                            ui.end_row();

                            // Password fill dropdown list
                            if self.show_password_suggestions && !self.password_suggestions.is_empty() {
                                ui.label("");
                                ui.vertical(|ui| {
                                    ui.group(|ui| {
                                        ui.label(t("conn.select_password"));
                                        let mut selected_id = None;
                                        for suggestion in &self.password_suggestions {
                                            if ui.selectable_label(false, &suggestion.label).clicked() {
                                                selected_id = Some(suggestion.connection_id.clone());
                                            }
                                        }
                                        if let Some(id) = selected_id {
                                            if let Some(creds) = credentials {
                                                if let Some(pwd) = creds.get(&id) {
                                                    self.password_input = pwd.to_string();
                                                }
                                            }
                                            self.show_password_suggestions = false;
                                        }
                                    });
                                });
                                ui.end_row();
                            }
                        } else {
                            // Public key auth
                            // Key path
                            ui.label(t("conn.key_file"));
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut self.key_path_input)
                                    .desired_width(280.0)
                                    .hint_text(t("conn.key_path_hint")));

                                if ui.button(t("conn.browse")).clicked() {
                                    let ssh_dir = dirs::home_dir()
                                        .map(|h| h.join(".ssh"))
                                        .unwrap_or_else(|| PathBuf::from("/root/.ssh"));
                                    let mut dialog = rfd::FileDialog::new();
                                    if ssh_dir.exists() {
                                        dialog = dialog.set_directory(&ssh_dir);
                                    }
                                    if let Some(path) = dialog.pick_file() {
                                        self.key_path_input = path.to_string_lossy().to_string();
                                    }
                                }
                            });
                            ui.end_row();

                            // Key passphrase (optional)
                            ui.label(t("conn.passphrase"));
                            ui.add(egui::TextEdit::singleline(&mut self.passphrase_input)
                                .password(true)
                                .desired_width(280.0)
                                .hint_text(t("conn.passphrase_hint")));
                            ui.end_row();
                        }

                        // Group
                        ui.label(t("conn.group"));
                        ui.horizontal(|ui| {
                            let mut group_str = self.config.group.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut group_str).changed() {
                                self.config.group = if group_str.is_empty() { None } else { Some(group_str.clone()) };
                            }
                            if !self.available_groups.is_empty() {
                                let popup_id = ui.make_persistent_id("group_popup");
                                let group_btn = ui.button("▼");
                                if group_btn.clicked() {
                                    ui.memory_mut(|mem| mem.toggle_popup(popup_id));
                                }
                                egui::popup_below_widget(ui, popup_id, &group_btn, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                                    ui.set_min_width(200.0);
                                    for g in &self.available_groups {
                                        if ui.selectable_label(false, g).clicked() {
                                            self.config.group = Some(g.clone());
                                            ui.memory_mut(|mem| mem.close_popup(popup_id));
                                        }
                                    }
                                });
                            }
                        });
                        ui.end_row();
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(t("conn.save")).clicked() {
                        // Sync key path to config before saving
                        if matches!(self.config.auth, AuthMethod::PublicKey { .. }) {
                            self.config.auth = AuthMethod::PublicKey {
                                key_path: PathBuf::from(&self.key_path_input),
                            };
                        }
                        result = Some(self.config.clone());
                        should_close = true;
                    }
                    if ui.button(t("conn.cancel")).clicked() {
                        should_close = true;
                    }
                });
            });

        if !open || should_close {
            self.visible = false;
            self.show_password_suggestions = false;
        }

        self.focus_requested = false;

        result
    }
}

impl Default for ConnectionPanel {
    fn default() -> Self {
        Self::new()
    }
}
