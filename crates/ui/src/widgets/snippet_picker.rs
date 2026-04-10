use egui::Ui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use stealthterm_config::i18n::t;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSnippet {
    pub name: String,
    pub content: String,
    pub category: String,
}

pub struct SnippetPicker {
    pub visible: bool,
    snippets: Vec<SavedSnippet>,
    selected_category: String,
}

impl SnippetPicker {
    pub fn new() -> Self {
        Self {
            visible: false,
            snippets: Self::load_snippets(),
            selected_category: t("snippet.category_all").to_string(),
        }
    }

    fn snippets_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stealthterm")
            .join("snippets.json")
    }

    fn load_snippets() -> Vec<SavedSnippet> {
        let path = Self::snippets_path();
        if !path.exists() {
            return Vec::new();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_snippet(&mut self, name: String, content: String, category: String) {
        self.snippets.push(SavedSnippet { name, content, category });
        self.save_to_disk();
    }

    fn save_to_disk(&self) {
        let path = Self::snippets_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.snippets) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<String> {
        let mut selected = None;

        ui.horizontal(|ui| {
            ui.label(t("snippet.category_label"));
            egui::ComboBox::from_id_salt("snippet_category")
                .selected_text(&self.selected_category)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.selected_category, t("snippet.category_all").to_string(), t("snippet.category_all"));
                    ui.selectable_value(&mut self.selected_category, t("snippet.category_review").to_string(), t("snippet.category_review"));
                    ui.selectable_value(&mut self.selected_category, t("snippet.category_bugfix").to_string(), t("snippet.category_bugfix"));
                    ui.selectable_value(&mut self.selected_category, t("snippet.category_feature").to_string(), t("snippet.category_feature"));
                    ui.selectable_value(&mut self.selected_category, t("snippet.category_docs").to_string(), t("snippet.category_docs"));
                });
        });

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for snippet in &self.snippets {
                if self.selected_category != t("snippet.category_all") && snippet.category != self.selected_category {
                    continue;
                }

                if ui.button(&snippet.name).clicked() {
                    selected = Some(snippet.content.clone());
                }
            }
        });

        selected
    }
}

impl Default for SnippetPicker {
    fn default() -> Self {
        Self::new()
    }
}
