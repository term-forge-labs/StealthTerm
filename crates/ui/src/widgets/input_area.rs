use egui::{Color32, Key, Ui};
use crate::theme::Theme;

/// Multi-line input area for AI collaboration input
pub struct InputArea {
    pub text: String,
    pub placeholder: String,
    pub history_suggestion: Option<String>,
}

impl InputArea {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            placeholder: "Type command... (Ctrl+Enter to send)".to_string(),
            history_suggestion: None,
        }
    }

    /// Returns the text if Ctrl+Enter was pressed
    pub fn show(&mut self, ui: &mut Ui, theme: &Theme) -> Option<String> {
        let mut send = None;

        let frame = egui::Frame::none()
            .fill(theme.input_bg)
            .inner_margin(egui::Margin::symmetric(8, 6));

        frame.show(ui, |ui| {
            ui.set_min_height(60.0);

            let response = ui.add(
                egui::TextEdit::multiline(&mut self.text)
                    .hint_text(&self.placeholder)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .text_color(theme.input_fg)
                    .frame(false),
            );

            // Handle Ctrl+Enter to send
            if response.has_focus() {
                let ctx = ui.ctx();
                if ctx.input(|i| i.key_pressed(Key::Enter) && i.modifiers.ctrl) {
                    if !self.text.trim().is_empty() {
                        let content = self.text.clone();
                        self.text.clear();
                        send = Some(content);
                    }
                }
            }

            // Show history suggestion in gray
            if let Some(suggestion) = &self.history_suggestion {
                if !suggestion.is_empty() && !self.text.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("{}{}", self.text, suggestion))
                            .color(Color32::from_gray(80))
                            .italics(),
                    );
                }
            }

            // Send button
            ui.with_layout(egui::Layout::right_to_left(egui::Align::BOTTOM), |ui| {
                if ui.button("Send (Ctrl+\u{23ce})").clicked() {
                    if !self.text.trim().is_empty() {
                        let content = self.text.clone();
                        self.text.clear();
                        send = Some(content);
                    }
                }
            });
        });

        send
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.history_suggestion = None;
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl Default for InputArea {
    fn default() -> Self {
        Self::new()
    }
}
