use egui::Color32;
use super::{Theme, ThemeKind};

pub struct TokyoNightTheme;

impl TokyoNightTheme {
    pub fn theme() -> Theme {
        Theme {
            kind: ThemeKind::TokyoNight,
            bg: Color32::from_rgb(0xf5, 0xf5, 0xf5),           // main background: white
            fg: Color32::from_rgb(0x2e, 0x34, 0x40),           // foreground: dark text
            accent: Color32::from_rgb(0x7a, 0xa2, 0xf7),
            tab_bg: Color32::from_rgb(0xe0, 0xe0, 0xe0),       // tab bar: light gray
            tab_active_bg: Color32::from_rgb(0xf5, 0xf5, 0xf5), // active tab: white
            tab_fg: Color32::from_rgb(0x2e, 0x34, 0x40),       // tab text: dark
            sidebar_bg: Color32::from_rgb(0xf5, 0xf5, 0xf5),   // sidebar: white
            sidebar_fg: Color32::from_rgb(0x2e, 0x34, 0x40),   // sidebar text: dark
            input_bg: Color32::from_rgb(0x33, 0x36, 0x49),     // input box: mid gray-blue
            input_fg: Color32::from_rgb(0xc0, 0xca, 0xf5),
            status_bar_bg: Color32::from_rgb(0x33, 0x36, 0x49), // status bar: mid gray-blue
            status_bar_fg: Color32::from_rgb(0x9a, 0xa5, 0xce),
            selection_bg: Color32::from_rgba_premultiplied(0x44, 0x88, 0xff, 0x66),
            border_color: Color32::from_rgb(0x29, 0x2e, 0x42),
            terminal_colors: [
                0x1a1b26, 0xf7768e, 0x9ece6a, 0xe0af68,
                0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xa9b1d6,
                0x414868, 0xf7768e, 0x9ece6a, 0xe0af68,
                0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xc0caf5,
            ],
        }
    }
}
