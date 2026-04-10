use egui::{Context, ScrollArea};
use stealthterm_config::i18n::t;

pub struct PasteConfirmDialog {
    pub visible: bool,
    pub content: String,
    pub line_count: usize,
    pub confirmed: bool,
    pub cancelled: bool,
}

impl PasteConfirmDialog {
    pub fn new() -> Self {
        Self {
            visible: false,
            content: String::new(),
            line_count: 0,
            confirmed: false,
            cancelled: false,
        }
    }

    pub fn show(&mut self, content: String) {
        self.content = content;
        self.line_count = self.content.lines().count();
        self.visible = true;
        self.confirmed = false;
        self.cancelled = false;
    }

    pub fn show_ctx(&mut self, ctx: &Context) {
        if !self.visible {
            return;
        }

        egui::Window::new(t("paste.title"))
            .collapsible(false)
            .resizable(false)
            .default_width(500.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("{} {}", self.line_count, t("paste.line_count")));
                ui.add_space(8.0);

                ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.content.as_str())
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace)
                        );
                    });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(t("paste.confirm")).clicked() {
                        self.confirmed = true;
                        self.visible = false;
                    }
                    if ui.button(t("paste.cancel")).clicked() {
                        self.cancelled = true;
                        self.visible = false;
                    }
                });
            });
    }

    pub fn reset(&mut self) {
        self.confirmed = false;
        self.cancelled = false;
    }
}

impl Default for PasteConfirmDialog {
    fn default() -> Self {
        Self::new()
    }
}
