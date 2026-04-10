use egui::Color32;
use super::{Theme, ThemeKind};

pub struct DraculaTheme;

impl DraculaTheme {
    pub fn theme() -> Theme {
        // Background slightly darker than reference #272822, with a warm tone to avoid pure black
        // Foreground #d0d0c8: contrast ~10:1, clear and easy on the eyes
        // ANSI colors have ~10% reduced saturation vs official, to reduce neon glare
        Theme {
            kind: ThemeKind::Dracula,
            bg: Color32::from_rgb(0x2e, 0x2d, 0x25),              // #272822 tweaked: slight warm brown + slightly brighter
            fg: Color32::from_rgb(0xee, 0xee, 0xe6),              // warm white, contrast ~13:1
            accent: Color32::from_rgb(0xbd, 0x93, 0xf9),
            tab_bg: Color32::from_rgb(0xf0, 0xf0, 0xf0),          // light gray tab bar background
            tab_active_bg: Color32::from_rgb(0xff, 0xff, 0xff),    // active tab white
            tab_fg: Color32::from_rgb(0x50, 0x50, 0x50),
            sidebar_bg: Color32::from_rgb(0xff, 0xff, 0xff),
            sidebar_fg: Color32::from_rgb(0x2e, 0x34, 0x40),
            input_bg: Color32::from_rgb(0x20, 0x21, 0x1b),
            input_fg: Color32::from_rgb(0xee, 0xee, 0xe6),
            status_bar_bg: Color32::from_rgb(0x1b, 0x1c, 0x17),
            status_bar_fg: Color32::from_rgb(0x62, 0x72, 0xa4),
            selection_bg: Color32::from_rgba_premultiplied(0x44, 0x88, 0xff, 0x66),
            border_color: Color32::from_rgb(0x44, 0x47, 0x5a),
            terminal_colors: [
                // Normal 0-7
                0x22231e, 0xf06060, 0x50e878, 0xe8e080,
                0xb090e8, 0xf070b0, 0x80d8e8, 0xd0d0c8,
                // Bright 8-15
                0x626a84, 0xf07878, 0x68f090, 0xf0e898,
                0xc8a8f0, 0xf088c8, 0x98e8f0, 0xe0e0d8,
            ],
        }
    }
}
