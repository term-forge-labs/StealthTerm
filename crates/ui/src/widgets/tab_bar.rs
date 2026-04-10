use egui::{Color32, Response, Stroke, Ui, Vec2};
use egui_twemoji::EmojiLabel;
use crate::theme::Theme;
use stealthterm_config::i18n::t;

pub type TabId = String;

#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub title: String,
    pub is_connected: bool,
    pub is_modified: bool,
    pub tab_type: TabType,
    pub ssh_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TabType {
    LocalTerminal,
    SshSession,
    Sftp,
}

impl Tab {
    pub fn new_local(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: "Terminal".to_string(),
            is_connected: true,
            is_modified: false,
            tab_type: TabType::LocalTerminal,
            ssh_host: None,
        }
    }

    pub fn new_ssh(id: impl Into<String>, host: &str) -> Self {
        Self {
            id: id.into(),
            title: host.to_string(),
            is_connected: false,
            is_modified: false,
            tab_type: TabType::SshSession,
            ssh_host: Some(host.to_string()),
        }
    }
}

pub struct TabBar {
    pub tabs: Vec<Tab>,
    pub active_tab: Option<TabId>,
    scroll_offset: usize,
    context_menu_tab: Option<TabId>,
    context_menu_pos: egui::Pos2,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: None,
            scroll_offset: 0,
            context_menu_tab: None,
            context_menu_pos: egui::Pos2::ZERO,
        }
    }

    pub fn add_tab(&mut self, tab: Tab) {
        self.active_tab = Some(tab.id.clone());
        self.tabs.push(tab);
    }

    pub fn close_tab(&mut self, id: &TabId) {
        self.tabs.retain(|t| &t.id != id);
        if self.active_tab.as_ref() == Some(id) {
            self.active_tab = self.tabs.last().map(|t| t.id.clone());
        }
    }

    /// Generate display names with numbering: duplicate tab titles get :1, :2 suffixes
    pub fn display_names(&self) -> Vec<(String, String)> {
        use std::collections::HashMap;
        // Count occurrences of each title
        let mut title_count: HashMap<&str, usize> = HashMap::new();
        for tab in &self.tabs {
            *title_count.entry(&tab.title).or_insert(0) += 1;
        }
        // Assign sequence numbers to duplicate titles
        let mut title_index: HashMap<&str, usize> = HashMap::new();
        self.tabs.iter().map(|tab| {
            let display = if title_count[tab.title.as_str()] > 1 {
                let idx = title_index.entry(&tab.title).or_insert(0);
                *idx += 1;
                format!("{}:{}", tab.title, idx)
            } else {
                tab.title.clone()
            };
            (tab.id.clone(), display)
        }).collect()
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active_tab.as_ref().and_then(|id| {
            self.tabs.iter().position(|t| &t.id == id)
        })
    }

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() { return; }
        let idx = self.active_index().unwrap_or(0);
        let next = (idx + 1) % self.tabs.len();
        self.active_tab = Some(self.tabs[next].id.clone());
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() { return; }
        let idx = self.active_index().unwrap_or(0);
        let prev = if idx == 0 { self.tabs.len() - 1 } else { idx - 1 };
        self.active_tab = Some(self.tabs[prev].id.clone());
    }

    /// Draw tab bar, returns Some(TabId) if tab was clicked/closed
    pub fn show(&mut self, ui: &mut Ui, theme: &Theme) -> TabBarAction {
        let mut action = TabBarAction::None;

        ui.horizontal(|ui| {
            // Add left margin to align with sidebar width (+ 5)
            ui.add_space(5.0);

            ui.style_mut().spacing.item_spacing = Vec2::new(2.0, 0.0);

            let available_width = ui.available_width() - 40.0; // reserve space for + button
            let tab_width = 150.0;
            let max_visible = (available_width / (tab_width + 2.0)).floor() as usize;

            // Ensure scroll_offset does not exceed tab count
            if self.scroll_offset >= self.tabs.len() {
                self.scroll_offset = self.tabs.len().saturating_sub(1);
            }
            if self.scroll_offset + max_visible > self.tabs.len() {
                self.scroll_offset = self.tabs.len().saturating_sub(max_visible.min(self.tabs.len()));
            }

            // Left arrow (colored emoji)
            if self.scroll_offset > 0 {
                ui.horizontal(|ui| {
                    if EmojiLabel::new("⬅").show(ui).clicked() {
                        self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    }
                });
            }

            let names: std::collections::HashMap<String, String> = self.display_names().into_iter().collect();
            let tab_ids: Vec<TabId> = self.tabs.iter().skip(self.scroll_offset).take(max_visible).map(|t| t.id.clone()).collect();

            for tab_id in &tab_ids {
                let tab = self.tabs.iter().find(|t| &t.id == tab_id).cloned();
                if let Some(tab) = tab {
                    let is_active = self.active_tab.as_ref() == Some(&tab.id);
                    let bg = if is_active { theme.tab_active_bg } else { theme.tab_bg };
                    let fg = if is_active { theme.fg } else { theme.tab_fg };
                    let display_name = names.get(&tab.id).cloned().unwrap_or_else(|| tab.title.clone());

                    let (tab_response, close_clicked) = Self::draw_tab(ui, &tab, &display_name, bg, fg, is_active);

                    if tab_response.clicked() {
                        action = TabBarAction::Activate(tab.id.clone());
                        self.active_tab = Some(tab.id.clone());
                    }
                    if close_clicked {
                        action = TabBarAction::Close(tab.id.clone());
                    }
                    if tab_response.middle_clicked() {
                        action = TabBarAction::Close(tab.id.clone());
                    }
                    // Right-click to open context menu
                    if tab_response.secondary_clicked() {
                        if let Some(pos) = tab_response.interact_pointer_pos() {
                            self.context_menu_tab = Some(tab.id.clone());
                            self.context_menu_pos = pos;
                        }
                    }
                }
            }

            // Right arrow (colored emoji)
            if self.scroll_offset + max_visible < self.tabs.len() {
                ui.horizontal(|ui| {
                    if EmojiLabel::new("➡").show(ui).clicked() {
                        self.scroll_offset += 1;
                    }
                });
            }
        });

        // Context menu
        if let Some(menu_tab_id) = self.context_menu_tab.clone() {
            let menu_tab = self.tabs.iter().find(|t| t.id == menu_tab_id).cloned();
            if let Some(tab) = menu_tab {
                egui::Area::new("tab_context_menu".into())
                    .fixed_pos(self.context_menu_pos)
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            // Match the built-in context_menu style: no background on inactive buttons
                            let style = ui.style_mut();
                            let transparent = egui::Color32::TRANSPARENT;
                            style.visuals.widgets.inactive.weak_bg_fill = transparent;
                            style.visuals.widgets.inactive.bg_fill = transparent;

                            ui.set_min_width(180.0);
                            if tab.tab_type == TabType::SshSession {
                                if ui.button(t("tab.duplicate_ssh")).clicked() {
                                    action = TabBarAction::DuplicateSsh(tab.id.clone());
                                    self.context_menu_tab = None;
                                }
                                ui.separator();
                            }
                            if ui.button(t("tab.close")).clicked() {
                                action = TabBarAction::Close(tab.id.clone());
                                self.context_menu_tab = None;
                            }
                            if ui.button(t("tab.close_others")).clicked() {
                                action = TabBarAction::CloseOthers(tab.id.clone());
                                self.context_menu_tab = None;
                            }
                            if ui.button(t("tab.close_left")).clicked() {
                                action = TabBarAction::CloseToTheLeft(tab.id.clone());
                                self.context_menu_tab = None;
                            }
                            if ui.button(t("tab.close_right")).clicked() {
                                action = TabBarAction::CloseToTheRight(tab.id.clone());
                                self.context_menu_tab = None;
                            }
                            ui.separator();
                            if ui.button(t("tab.close_all")).clicked() {
                                action = TabBarAction::CloseAll;
                                self.context_menu_tab = None;
                            }
                        });
                    });

                // Detect click to close menu (but not the menu itself)
                if ui.ctx().input(|i| i.pointer.primary_clicked()) {
                    self.context_menu_tab = None;
                }
            }
        }

        action
    }

    fn draw_tab(ui: &mut Ui, tab: &Tab, display_name: &str, _bg: Color32, _fg: Color32, active: bool) -> (Response, bool) {
        let mut close_clicked = false;
        let ppp = ui.ctx().pixels_per_point();

        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(150.0, 32.0),
            egui::Sense::click(),
        );

        if ui.is_rect_visible(rect) {
            // Snap rect to pixel grid to avoid sub-pixel blur
            let rect = egui::Rect::from_min_max(
                egui::pos2(
                    (rect.min.x * ppp).round() / ppp,
                    (rect.min.y * ppp).round() / ppp,
                ),
                egui::pos2(
                    (rect.max.x * ppp).round() / ppp,
                    (rect.max.y * ppp).round() / ppp,
                ),
            );

            // Draw background
            {
                let painter = ui.painter();
                if active {
                    // Active tab: white background
                    painter.rect_filled(rect, 0.0, Color32::from_rgb(0xff, 0xff, 0xff));
                    // Blue underline for active tab
                    let line_y = rect.bottom();
                    painter.line_segment(
                        [egui::pos2(rect.left(), line_y), egui::pos2(rect.right(), line_y)],
                        Stroke::new(5.0, Color32::from_rgb(0x00, 0x96, 0xD6)),
                    );
                } else if resp.hovered() {
                    painter.rect_filled(rect, 0.0, Color32::from_rgb(0xe8, 0xe8, 0xe8));
                }
            }

            // Snap text position to pixel grid
            let text_x = ((rect.left() + 8.0) * ppp).round() / ppp;
            let text_y = ((rect.center().y) * ppp).round() / ppp;
            let text_pos = egui::pos2(text_x, text_y);
            let icon = if tab.is_connected { "🟢" } else { "🔴" };
            let text_color = if active { Color32::from_rgb(0x2e, 0x34, 0x40) } else { Color32::from_rgb(0x88, 0x88, 0x88) };

            // Draw directly with painter, do not intercept clicks
            let painter = ui.painter();
            let label_text = format!("{} {}", icon, display_name);
            painter.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                label_text,
                egui::FontId::proportional(14.0),
                text_color,
            );

            // Overlay red rotated "disconnected" text when disconnected
            if !tab.is_connected {
                let clip_painter = ui.painter_at(rect);
                let galley = clip_painter.layout_no_wrap(
                    t("tab.disconnected").to_string(),
                    egui::FontId::proportional(13.0),
                    Color32::from_rgb(220, 50, 50),
                );
                let center = rect.center();
                let text_shape = egui::epaint::TextShape::new(center, galley, Color32::from_rgb(220, 50, 50))
                    .with_override_text_color(Color32::from_rgb(220, 50, 50))
                    .with_angle_and_anchor(-0.35, egui::Align2::CENTER_CENTER)
                    .with_opacity_factor(0.85);
                clip_painter.add(text_shape);
            }

            // Close button - larger for easier clicking
            let close_center_x = ((rect.right() - 12.0) * ppp).round() / ppp;
            let close_center_y = ((rect.center().y) * ppp).round() / ppp;
            let close_rect = egui::Rect::from_center_size(
                egui::pos2(close_center_x, close_center_y),
                egui::vec2(24.0, 24.0),
            );
            let close_resp = ui.interact(close_rect, ui.id().with(("close", &tab.id)), egui::Sense::click());
            if close_resp.clicked() {
                close_clicked = true;
            }

            // Draw close button - using colored emoji
            {
                let painter = ui.painter();
                // Show background on hover
                if close_resp.hovered() {
                    painter.circle_filled(close_rect.center(), 10.0, Color32::from_rgba_premultiplied(255, 0, 0, 50));
                }
                painter.text(
                    close_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "❌",
                    egui::FontId::proportional(12.0),
                    text_color,
                );
            }
        }

        (resp, close_clicked)
    }
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq)]
pub enum TabBarAction {
    None,
    Activate(TabId),
    Close(TabId),
    NewLocal,
    NewSsh,
    DuplicateSsh(TabId),
    CloseAll,
    CloseOthers(TabId),
    CloseToTheRight(TabId),
    CloseToTheLeft(TabId),
}
