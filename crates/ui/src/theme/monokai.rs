use egui::Color32;
use super::{Theme, ThemeKind};

pub struct MonokaiTheme;

impl MonokaiTheme {
    pub fn theme() -> Theme {
        // Background slightly darker than reference #272822, with a warm tone
        Theme {
            kind: ThemeKind::Monokai,
            bg: Color32::from_rgb(0x26, 0x27, 0x21),
            fg: Color32::from_rgb(0xee, 0xee, 0xe6),
            accent: Color32::from_rgb(0xf9, 0x26, 0x72),
            tab_bg: Color32::from_rgb(0x20, 0x21, 0x1b),
            tab_active_bg: Color32::from_rgb(0x26, 0x27, 0x21),
            tab_fg: Color32::from_rgb(0x75, 0x71, 0x5e),
            sidebar_bg: Color32::from_rgb(0xff, 0xff, 0xff),
            sidebar_fg: Color32::from_rgb(0x2e, 0x34, 0x40),
            input_bg: Color32::from_rgb(0x20, 0x21, 0x1b),
            input_fg: Color32::from_rgb(0xee, 0xee, 0xe6),
            status_bar_bg: Color32::from_rgb(0x20, 0x21, 0x1b),
            status_bar_fg: Color32::from_rgb(0x75, 0x71, 0x5e),
            selection_bg: Color32::from_rgba_premultiplied(0x44, 0x88, 0xff, 0x66),
            border_color: Color32::from_rgb(0x49, 0x48, 0x3e),
            terminal_colors: [
                // Normal
                0x22231e, 0xe8286a, 0x98d828, 0xe0b068,
                0x60c8e0, 0xa078e8, 0x90d8c8, 0xd0d0c8,
                // Bright
                0x706858, 0xe8286a, 0x98d828, 0xe0b068,
                0x60c8e0, 0xa078e8, 0x90d8c8, 0xe0e0d8,
            ],
        }
    }
}
