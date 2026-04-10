use egui::Context;
use crate::theme::Theme;
use stealthterm_config::settings::Settings;
use stealthterm_config::i18n::{t, tf, set_lang, lang, Lang};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SettingsTab {
    Appearance,
    Terminal,
    Security,
    Language,
    About,
}

pub struct SettingsPanel {
    pub visible: bool,
    pub settings_changed: bool,
    active_tab: SettingsTab,
    // Password management
    password_input: String,
    password_confirm: String,
    old_password_input: String,
    password_message: String,
    password_message_is_error: bool,
    // Clear history confirm
    history_clear_confirm: bool,
    history_clear_message: String,
}

pub enum SettingsAction {
    ThemeChanged(String),
    FontSizeChanged(f32),
    LineNumbersToggled(bool),
    ClearHistory,
    LanguageChanged(String),
    None,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            settings_changed: false,
            active_tab: SettingsTab::Appearance,
            password_input: String::new(),
            password_confirm: String::new(),
            old_password_input: String::new(),
            password_message: String::new(),
            password_message_is_error: false,
            history_clear_confirm: false,
            history_clear_message: String::new(),
        }
    }

    pub fn show_ctx(&mut self, ctx: &Context, settings: &mut Settings) -> SettingsAction {
        if !self.visible {
            return SettingsAction::None;
        }

        let mut action = SettingsAction::None;
        let mut visible = self.visible;

        egui::Window::new(t("settings.title"))
            .resizable(true)
            .default_width(520.0)
            .default_height(480.0)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_TOP, [-10.0, 40.0])
            .open(&mut visible)
            .show(ctx, |ui| {
                // Tab strip
                ui.horizontal(|ui| {
                    let tabs = [
                        (SettingsTab::Appearance, t("settings.appearance")),
                        (SettingsTab::Terminal,   t("settings.terminal")),
                        (SettingsTab::Security,   t("settings.security")),
                        (SettingsTab::Language,   t("settings.language")),
                        (SettingsTab::About,      t("settings.about")),
                    ];
                    for (tab, label) in &tabs {
                        let selected = self.active_tab == *tab;
                        let text = egui::RichText::new(*label);
                        let text = if selected { text.strong() } else { text };
                        if ui.selectable_label(selected, text).clicked() {
                            self.active_tab = *tab;
                        }
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.active_tab {
                        SettingsTab::Appearance => {
                            self.show_appearance_tab(ui, settings, &mut action);
                        }
                        SettingsTab::Terminal => {
                            self.show_terminal_tab(ui, settings, &mut action);
                        }
                        SettingsTab::Security => {
                            self.show_security_tab(ui, settings, &mut action);
                        }
                        SettingsTab::Language => {
                            self.show_language_tab(ui, settings, &mut action);
                        }
                        SettingsTab::About => {
                            self.show_about_tab(ui, settings);
                        }
                    }

                    // Save button (shown on all tabs when there are unsaved changes)
                    if self.settings_changed {
                        ui.add_space(10.0);
                        if ui.button(egui::RichText::new(t("settings.save"))
                            .color(egui::Color32::from_rgb(0xAB, 0xB2, 0xBF))
                            .strong())
                            .clicked()
                        {
                            let _ = settings.save();
                            self.settings_changed = false;
                        }
                    }
                });
            });

        self.visible = visible;
        action
    }

    fn show_appearance_tab(&mut self, ui: &mut egui::Ui, settings: &mut Settings, action: &mut SettingsAction) {
        let muted = egui::Color32::from_rgb(0xAB, 0xB2, 0xBF);

        // Theme selector
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t("settings.theme")).color(muted));
            egui::ComboBox::from_id_salt("theme_selector")
                .selected_text(&settings.theme)
                .show_ui(ui, |ui| {
                    for (id, name) in Theme::available_themes() {
                        if ui.selectable_value(&mut settings.theme, id.to_string(), *name).clicked() {
                            *action = SettingsAction::ThemeChanged(id.to_string());
                            self.settings_changed = true;
                        }
                    }
                });
        });

        // Font size
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t("settings.font_size")).color(muted));
            if ui.add(egui::Slider::new(&mut settings.font_size, 8.0..=24.0)).changed() {
                *action = SettingsAction::FontSizeChanged(settings.font_size);
                self.settings_changed = true;
            }
        });
    }

    fn show_terminal_tab(&mut self, ui: &mut egui::Ui, settings: &mut Settings, action: &mut SettingsAction) {
        let muted = egui::Color32::from_rgb(0xAB, 0xB2, 0xBF);
        let red   = egui::Color32::from_rgb(0xE0, 0x6C, 0x75);
        let green = egui::Color32::from_rgb(0x98, 0xC3, 0x79);

        // Line numbers
        if ui.checkbox(&mut settings.show_line_numbers, t("settings.show_line_numbers")).changed() {
            *action = SettingsAction::LineNumbersToggled(settings.show_line_numbers);
        }

        // Cursor blink
        if ui.checkbox(&mut settings.cursor_blink, t("settings.cursor_blink")).changed() {
            self.settings_changed = true;
        }

        // Scrollback
        let lines_str = settings.scrollback_lines.to_string();
        ui.label(egui::RichText::new(tf("settings.scrollback", &[&lines_str])).color(muted));

        // Clear history
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if !self.history_clear_confirm {
                if ui.button(egui::RichText::new(t("settings.clear_history")).color(red)).clicked() {
                    self.history_clear_confirm = true;
                    self.history_clear_message.clear();
                }
            } else {
                ui.label(egui::RichText::new(t("settings.clear_history_confirm")).color(red));
                if ui.button(egui::RichText::new(t("settings.confirm")).color(red).strong()).clicked() {
                    *action = SettingsAction::ClearHistory;
                    self.history_clear_confirm = false;
                    self.history_clear_message = t("settings.clear_history_done").to_string();
                }
                if ui.button(t("settings.cancel")).clicked() {
                    self.history_clear_confirm = false;
                }
            }
        });
        if !self.history_clear_message.is_empty() {
            ui.label(egui::RichText::new(&self.history_clear_message).color(green));
        }
    }

    fn show_security_tab(&mut self, ui: &mut egui::Ui, settings: &mut Settings, action: &mut SettingsAction) {
        let _ = action; // security tab doesn't produce actions currently
        let muted = egui::Color32::from_rgb(0xAB, 0xB2, 0xBF);
        let red   = egui::Color32::from_rgb(0xE0, 0x6C, 0x75);

        if settings.has_access_password() {
            ui.label(egui::RichText::new(t("settings.password_set")).color(muted));
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("settings.current_password")).color(muted));
                ui.add(egui::TextEdit::singleline(&mut self.old_password_input)
                    .password(true).desired_width(150.0));
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("settings.new_password")).color(muted));
                ui.add(egui::TextEdit::singleline(&mut self.password_input)
                    .password(true).desired_width(150.0));
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("settings.confirm_password")).color(muted));
                ui.add(egui::TextEdit::singleline(&mut self.password_confirm)
                    .password(true).desired_width(150.0));
            });

            ui.horizontal(|ui| {
                if ui.button(t("settings.change_password")).clicked() {
                    if self.old_password_input.is_empty() {
                        self.password_message = t("settings.enter_current").to_string();
                        self.password_message_is_error = true;
                    } else if !settings.verify_access_password(&self.old_password_input) {
                        self.password_message = t("settings.wrong_current").to_string();
                        self.password_message_is_error = true;
                    } else if self.password_input.len() < 4 {
                        self.password_message = t("settings.too_short").to_string();
                        self.password_message_is_error = true;
                    } else if self.password_input != self.password_confirm {
                        self.password_message = t("settings.mismatch").to_string();
                        self.password_message_is_error = true;
                    } else {
                        settings.set_access_password(&self.password_input);
                        let _ = settings.save();
                        self.clear_password_fields();
                        self.password_message = t("settings.password_changed").to_string();
                        self.password_message_is_error = false;
                    }
                }
                if ui.button(egui::RichText::new(t("settings.remove_password")).color(red)).clicked() {
                    if self.old_password_input.is_empty() {
                        self.password_message = t("settings.enter_current").to_string();
                        self.password_message_is_error = true;
                    } else if !settings.verify_access_password(&self.old_password_input) {
                        self.password_message = t("settings.wrong_current").to_string();
                        self.password_message_is_error = true;
                    } else {
                        settings.clear_access_password();
                        let _ = settings.save();
                        self.clear_password_fields();
                        self.password_message = t("settings.password_removed").to_string();
                        self.password_message_is_error = false;
                    }
                }
            });
        } else {
            ui.label(egui::RichText::new(t("settings.set_password"))
                .color(egui::Color32::from_rgb(0xE5, 0xC0, 0x7B)));
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("settings.enter_password")).color(muted));
                ui.add(egui::TextEdit::singleline(&mut self.password_input)
                    .password(true).desired_width(150.0));
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("settings.confirm_password")).color(muted));
                ui.add(egui::TextEdit::singleline(&mut self.password_confirm)
                    .password(true).desired_width(150.0));
            });

            if ui.button(t("settings.set_password")).clicked() {
                if self.password_input.len() < 4 {
                    self.password_message = t("settings.too_short").to_string();
                    self.password_message_is_error = true;
                } else if self.password_input != self.password_confirm {
                    self.password_message = t("settings.mismatch").to_string();
                    self.password_message_is_error = true;
                } else {
                    settings.set_access_password(&self.password_input);
                    let _ = settings.save();
                    self.clear_password_fields();
                    self.password_message = t("settings.password_set_ok").to_string();
                    self.password_message_is_error = false;
                }
            }
        }

        if !self.password_message.is_empty() {
            let color = if self.password_message_is_error {
                egui::Color32::from_rgb(255, 80, 80)
            } else {
                egui::Color32::from_rgb(80, 200, 80)
            };
            ui.label(egui::RichText::new(&self.password_message).color(color));
        }

        // Auto-lock (only when password is set)
        if settings.has_access_password() {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("settings.auto_lock")).color(muted));
                let mins_str = settings.auto_lock_minutes.to_string();
                let label = if settings.auto_lock_minutes == 0 {
                    t("settings.no_auto_lock").to_string()
                } else {
                    tf("settings.minutes", &[&mins_str])
                };
                if ui.add(egui::Slider::new(&mut settings.auto_lock_minutes, 0..=60).text(label)).changed() {
                    self.settings_changed = true;
                }
            });
        }
    }

    fn show_language_tab(&mut self, ui: &mut egui::Ui, settings: &mut Settings, action: &mut SettingsAction) {
        let muted = egui::Color32::from_rgb(0xAB, 0xB2, 0xBF);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t("settings.language_label")).color(muted));

            let mut current = lang();

            let changed_en = ui.radio_value(&mut current, Lang::En, "English").clicked();
            let changed_zh = ui.radio_value(&mut current, Lang::Zh, "中文").clicked();

            if changed_en || changed_zh {
                set_lang(current);
                settings.language = current.code().to_string();
                self.settings_changed = true;
                *action = SettingsAction::LanguageChanged(settings.language.clone());
            }
        });
    }

    fn show_about_tab(&self, ui: &mut egui::Ui, _settings: &Settings) {
        let accent = egui::Color32::from_rgb(0x00, 0x96, 0xD6);
        let muted  = egui::Color32::from_rgb(0xAB, 0xB2, 0xBF);

        ui.label(egui::RichText::new(t("settings.version_info")).color(accent).strong());
        ui.label(egui::RichText::new(t("settings.modern_terminal")).color(muted));
    }

    fn clear_password_fields(&mut self) {
        self.password_input.clear();
        self.password_confirm.clear();
        self.old_password_input.clear();
    }
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}
