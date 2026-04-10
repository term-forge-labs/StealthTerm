use egui::{Key, Ui};
use crate::theme::Theme;
use stealthterm_config::i18n::t;

/// Actions returned by the search bar UI
#[derive(Debug, PartialEq)]
pub enum SearchAction {
    None,
    QueryChanged,
    Next,
    Prev,
    Close,
}

pub struct SearchBar {
    pub visible: bool,
    pub query: String,
    pub use_regex: bool,
    pub case_sensitive: bool,
    pub result_count: usize,
    pub current_result: usize,
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            use_regex: false,
            case_sensitive: false,
            result_count: 0,
            current_result: 0,
        }
    }

    pub fn open(&mut self) {
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.result_count = 0;
        self.current_result = 0;
    }

    /// Show the search bar and return the action taken
    pub fn show(&mut self, ui: &mut Ui, _theme: &Theme) -> SearchAction {
        if !self.visible { return SearchAction::None; }

        let mut action = SearchAction::None;

        let frame = egui::Frame::none()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(egui::Margin::symmetric(8, 4));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let before = self.query.clone();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(t("search.placeholder"))
                        .desired_width(200.0),
                );
                if self.query != before {
                    action = SearchAction::QueryChanged;
                }

                // Enter key navigates to next result
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    action = SearchAction::Next;
                    resp.request_focus();
                }

                // Options
                if ui.checkbox(&mut self.use_regex, ".*").changed() {
                    action = SearchAction::QueryChanged;
                }
                if ui.checkbox(&mut self.case_sensitive, "Aa").changed() {
                    action = SearchAction::QueryChanged;
                }

                // Navigation
                if ui.small_button("^").clicked() {
                    action = SearchAction::Prev;
                }
                if ui.small_button("v").clicked() {
                    action = SearchAction::Next;
                }

                // Result count
                if self.result_count > 0 {
                    ui.label(format!("{}/{}", self.current_result + 1, self.result_count));
                } else if !self.query.is_empty() {
                    ui.label(t("search.no_results"));
                }

                // Close button
                if ui.small_button("x").clicked() {
                    action = SearchAction::Close;
                }

                // Close on Escape
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    action = SearchAction::Close;
                }
            });
        });

        if action == SearchAction::Close {
            self.close();
        }

        action
    }
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}
