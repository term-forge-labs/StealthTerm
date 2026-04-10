use egui::{Color32, Ui};
use crate::theme::Theme;
use crate::widgets::server_monitor::{ServerStats, format_rate, format_bytes};
use stealthterm_config::i18n::t;

pub struct StatusBar;

impl StatusBar {
    pub fn show(
        ui: &mut Ui,
        theme: &Theme,
        connected: bool,
        encoding: &str,
        cols: usize,
        rows: usize,
        uptime_secs: u64,
        server_stats: Option<&ServerStats>,
    ) {
        let frame = egui::Frame::none()
            .fill(theme.sidebar_bg)
            .inner_margin(egui::Margin::symmetric(8, 3));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // All items right-aligned
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text_color = Color32::BLACK;

                    // Uptime (rightmost)
                    let hours = uptime_secs / 3600;
                    let minutes = (uptime_secs % 3600) / 60;
                    let seconds = uptime_secs % 60;
                    ui.label(egui::RichText::new(format!("{:02}:{:02}:{:02}", hours, minutes, seconds)).color(text_color).small());
                    ui.separator();

                    ui.label(egui::RichText::new(format!("{}×{}", cols, rows)).color(text_color).small());
                    ui.separator();

                    ui.label(egui::RichText::new(encoding).color(text_color).small());
                    ui.separator();

                    // Connection status
                    let (icon, label, color) = if connected {
                        ("\u{1f7e2}", t("status.connected"), Color32::from_rgb(0x9e, 0xce, 0x6a))
                    } else {
                        ("\u{26aa}", t("status.disconnected"), text_color)
                    };
                    ui.label(egui::RichText::new(format!("{} {}", icon, label)).color(color).small());
                    ui.separator();

                    // Server monitoring stats (right next to connection status)
                    if let Some(stats) = server_stats {
                        // Network
                        ui.label(egui::RichText::new(format!(
                            "{}{} {}{}", t("monitor.net_down"), format_rate(stats.net_rx_rate),
                            t("monitor.net_up"), format_rate(stats.net_tx_rate),
                        )).color(text_color).small());
                        ui.separator();

                        // Disks
                        for disk in stats.disks.iter().rev() {
                            ui.label(egui::RichText::new(format!(
                                "{}: {:.0}%", disk.mount, disk.used_percent
                            )).color(text_color).small());
                            ui.separator();
                        }

                        // Memory
                        let mem_pct = if stats.mem_total > 0 {
                            100.0 * stats.mem_used as f32 / stats.mem_total as f32
                        } else {
                            0.0
                        };
                        ui.label(egui::RichText::new(format!(
                            "{}: {}/{} ({:.0}%)",
                            t("monitor.mem"),
                            format_bytes(stats.mem_used),
                            format_bytes(stats.mem_total),
                            mem_pct,
                        )).color(text_color).small());
                        ui.separator();

                        // CPU
                        ui.label(egui::RichText::new(format!("{}: {:.0}%", t("monitor.cpu"), stats.cpu_percent)).color(text_color).small());
                    }
                });
            });
        });
    }
}
