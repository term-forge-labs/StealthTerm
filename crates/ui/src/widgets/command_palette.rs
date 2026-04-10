use egui::{Context, Key};

use crate::theme::Theme;
use stealthterm_config::i18n::t;

/// A single command in the palette
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub shortcut: Option<String>,
    pub action: CommandAction,
}

/// Actions the command palette can trigger
#[derive(Debug, Clone, PartialEq)]
pub enum CommandAction {
    NewLocalTab,
    NewSshConnection,
    CloseTab,
    NextTab,
    PrevTab,
    ToggleSidebar,
    ToggleSearch,
    ToggleSftp,
    ToggleBatchMode,
    SplitHorizontal,
    SplitVertical,
    FontIncrease,
    FontDecrease,
    FontReset,
    Fullscreen,
    OpenSettings,
    SwitchTheme(String),
}

pub struct CommandPalette {
    pub visible: bool,
    pub query: String,
    pub selected_index: usize,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            selected_index: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.query.clear();
            self.selected_index = 0;
        }
    }

    pub fn open(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected_index = 0;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    fn build_commands() -> Vec<Command> {
        vec![
            Command { name: t("cmd.new_local_terminal").to_string(), shortcut: Some("Ctrl+T".into()), action: CommandAction::NewLocalTab },
            Command { name: t("cmd.new_ssh_connection").to_string(), shortcut: Some("Ctrl+N".into()), action: CommandAction::NewSshConnection },
            Command { name: t("cmd.close_tab").to_string(), shortcut: Some("Ctrl+W".into()), action: CommandAction::CloseTab },
            Command { name: t("cmd.next_tab").to_string(), shortcut: Some("Ctrl+Tab".into()), action: CommandAction::NextTab },
            Command { name: t("cmd.prev_tab").to_string(), shortcut: Some("Ctrl+Shift+Tab".into()), action: CommandAction::PrevTab },
            Command { name: t("cmd.toggle_sidebar").to_string(), shortcut: Some("Ctrl+B".into()), action: CommandAction::ToggleSidebar },
            Command { name: t("cmd.search_terminal").to_string(), shortcut: Some("Ctrl+Shift+F".into()), action: CommandAction::ToggleSearch },
            Command { name: t("cmd.toggle_sftp").to_string(), shortcut: None, action: CommandAction::ToggleSftp },
            Command { name: t("cmd.toggle_batch").to_string(), shortcut: None, action: CommandAction::ToggleBatchMode },
            Command { name: t("cmd.split_horizontal").to_string(), shortcut: Some("Ctrl+Shift+D".into()), action: CommandAction::SplitHorizontal },
            Command { name: t("cmd.split_vertical").to_string(), shortcut: Some("Ctrl+Shift+R".into()), action: CommandAction::SplitVertical },
            Command { name: t("cmd.font_increase").to_string(), shortcut: Some("Ctrl+=".into()), action: CommandAction::FontIncrease },
            Command { name: t("cmd.font_decrease").to_string(), shortcut: Some("Ctrl+-".into()), action: CommandAction::FontDecrease },
            Command { name: t("cmd.font_reset").to_string(), shortcut: Some("Ctrl+0".into()), action: CommandAction::FontReset },
            Command { name: t("cmd.toggle_fullscreen").to_string(), shortcut: Some("F11".into()), action: CommandAction::Fullscreen },
            Command { name: t("cmd.open_settings").to_string(), shortcut: None, action: CommandAction::OpenSettings },
            Command { name: format!("{} Tokyo Night", t("cmd.theme_prefix")), shortcut: None, action: CommandAction::SwitchTheme("tokyo-night".into()) },
            Command { name: format!("{} Catppuccin Mocha", t("cmd.theme_prefix")), shortcut: None, action: CommandAction::SwitchTheme("catppuccin".into()) },
            Command { name: format!("{} Dracula", t("cmd.theme_prefix")), shortcut: None, action: CommandAction::SwitchTheme("dracula".into()) },
            Command { name: format!("{} Solarized Dark", t("cmd.theme_prefix")), shortcut: None, action: CommandAction::SwitchTheme("solarized-dark".into()) },
            Command { name: format!("{} One Dark", t("cmd.theme_prefix")), shortcut: None, action: CommandAction::SwitchTheme("one-dark".into()) },
        ]
    }

    fn filtered_indices(commands: &[Command], query: &str) -> Vec<usize> {
        if query.is_empty() {
            return (0..commands.len()).collect();
        }
        let query_lower = query.to_lowercase();
        commands.iter().enumerate()
            .filter(|(_, cmd)| {
                let name_lower = cmd.name.to_lowercase();
                let mut chars = query_lower.chars();
                let mut current = chars.next();
                for c in name_lower.chars() {
                    if let Some(q) = current {
                        if c == q {
                            current = chars.next();
                        }
                    } else {
                        break;
                    }
                }
                current.is_none()
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Show the command palette overlay, returns the selected action if any
    pub fn show(&mut self, ctx: &Context, _theme: &Theme) -> Option<CommandAction> {
        if !self.visible {
            return None;
        }

        let mut action = None;

        // Handle keyboard navigation before rendering
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
                self.visible = false;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown) {
                self.selected_index = self.selected_index.saturating_add(1);
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowUp) {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
        });

        if !self.visible {
            return None;
        }

        let commands = Self::build_commands();
        let indices = Self::filtered_indices(&commands, &self.query);
        let count = indices.len();
        if count > 0 {
            self.selected_index = self.selected_index.min(count - 1);
        } else {
            self.selected_index = 0;
        }

        // Check for Enter
        let enter_pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
        if enter_pressed && !indices.is_empty() {
            action = Some(commands[indices[self.selected_index]].action.clone());
            self.visible = false;
            return action;
        }

        // Snapshot data for the closures
        let selected_index = self.selected_index;
        let mut new_selected = self.selected_index;
        let mut should_close = false;

        egui::Area::new(egui::Id::new("command_palette_overlay"))
            .fixed_pos(egui::Pos2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let screen = ui.ctx().screen_rect();
                ui.painter().rect_filled(
                    screen,
                    0.0,
                    egui::Color32::from_black_alpha(100),
                );

                let palette_width = (screen.width() * 0.5).min(500.0).max(300.0);
                let palette_x = (screen.width() - palette_width) / 2.0;
                let palette_y = screen.height() * 0.15;

                egui::Window::new("Command Palette")
                    .title_bar(false)
                    .resizable(false)
                    .fixed_pos(egui::Pos2::new(palette_x, palette_y))
                    .fixed_size(egui::Vec2::new(palette_width, 0.0))
                    .show(ui.ctx(), |ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text(t("cmd.search_placeholder"))
                                .desired_width(f32::INFINITY),
                        );
                        resp.request_focus();

                        ui.separator();

                        // Re-filter after potential query change
                        let indices = Self::filtered_indices(&commands, &self.query);
                        let count = indices.len();
                        let sel = if count > 0 { selected_index.min(count - 1) } else { 0 };

                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for (list_i, &cmd_i) in indices.iter().enumerate() {
                                    let cmd = &commands[cmd_i];
                                    let is_selected = list_i == sel;
                                    let mut text = cmd.name.clone();
                                    if let Some(shortcut) = &cmd.shortcut {
                                        text = format!("{}    {}", text, shortcut);
                                    }
                                    let resp = ui.selectable_label(is_selected, text);
                                    if resp.clicked() {
                                        action = Some(cmd.action.clone());
                                        should_close = true;
                                    }
                                    if resp.hovered() {
                                        new_selected = list_i;
                                    }
                                }
                            });

                        if indices.is_empty() && !self.query.is_empty() {
                            ui.label(t("cmd.no_results"));
                        }
                    });
            });

        self.selected_index = new_selected;
        if should_close {
            self.visible = false;
        }

        action
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}
