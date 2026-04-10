use egui::{Context, ProgressBar};
use stealthterm_config::i18n::t;

#[derive(Debug, Clone, PartialEq)]
pub enum TransferStatus {
    Transferring,
    Paused,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct TransferItem {
    pub filename: String,
    pub size: u64,
    pub transferred: u64,
    pub speed: f64,
    pub status: TransferStatus,
}

impl TransferItem {
    pub fn progress(&self) -> f32 {
        if self.size == 0 {
            0.0
        } else {
            self.transferred as f32 / self.size as f32
        }
    }

    pub fn remaining_time(&self) -> String {
        if self.speed <= 0.0 || self.status != TransferStatus::Transferring {
            return "--:--".to_string();
        }
        let remaining = (self.size - self.transferred) as f64 / self.speed;
        let mins = (remaining / 60.0) as u32;
        let secs = (remaining % 60.0) as u32;
        format!("{:02}:{:02}", mins, secs)
    }

    pub fn speed_str(&self) -> String {
        if self.speed < 1024.0 {
            format!("{:.0} B/s", self.speed)
        } else if self.speed < 1024.0 * 1024.0 {
            format!("{:.1} KB/s", self.speed / 1024.0)
        } else {
            format!("{:.1} MB/s", self.speed / 1024.0 / 1024.0)
        }
    }
}

pub struct TransferQueuePanel {
    pub visible: bool,
    transfers: Vec<TransferItem>,
}

impl TransferQueuePanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            transfers: Vec::new(),
        }
    }

    pub fn add_transfer(&mut self, item: TransferItem) {
        self.transfers.push(item);
    }

    pub fn show(&mut self, ctx: &Context) {
        if !self.visible {
            return;
        }

        egui::Window::new(t("transfer.title"))
            .open(&mut self.visible)
            .default_width(600.0)
            .show(ctx, |ui| {
                if self.transfers.is_empty() {
                    ui.label(t("transfer.no_items"));
                    return;
                }

                for (_idx, item) in self.transfers.iter().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let status_icon = match item.status {
                                TransferStatus::Transferring => "⏵",
                                TransferStatus::Paused => "⏸",
                                TransferStatus::Completed => "✓",
                                TransferStatus::Failed => "✗",
                            };
                            ui.label(status_icon);
                            ui.label(&item.filename);
                        });

                        ui.add(ProgressBar::new(item.progress()).show_percentage());

                        ui.horizontal(|ui| {
                            ui.label(format!("{}: {}", t("transfer.speed"), item.speed_str()));
                            ui.label(format!("{}: {}", t("transfer.remaining"), item.remaining_time()));

                            if item.status == TransferStatus::Transferring {
                                if ui.small_button(t("transfer.pause")).clicked() {
                                    // TODO: implement pause
                                }
                            } else if item.status == TransferStatus::Paused {
                                if ui.small_button(t("transfer.resume")).clicked() {
                                    // TODO: implement resume
                                }
                            }

                            if ui.small_button(t("transfer.cancel")).clicked() {
                                // TODO: implement cancel
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            });
    }
}

impl Default for TransferQueuePanel {
    fn default() -> Self {
        Self::new()
    }
}
