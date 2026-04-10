use egui::Color32;
use super::{Theme, ThemeKind};

pub struct OneDarkTheme;

impl OneDarkTheme {
    pub fn theme() -> Theme {
        Theme {
            kind: ThemeKind::OneDark,
            bg: Color32::from_rgb(0x28, 0x2c, 0x34),
            fg: Color32::from_rgb(0xab, 0xb2, 0xbf),
            accent: Color32::from_rgb(0x61, 0xaf, 0xef),
            tab_bg: Color32::from_rgb(0x21, 0x25, 0x2b),
            tab_active_bg: Color32::from_rgb(0x28, 0x2c, 0x34),
            tab_fg: Color32::from_rgb(0x5c, 0x63, 0x70),
            sidebar_bg: Color32::from_rgb(0x21, 0x25, 0x2b),
            sidebar_fg: Color32::from_rgb(0xab, 0xb2, 0xbf),
            input_bg: Color32::from_rgb(0x1b, 0x1f, 0x27),
            input_fg: Color32::from_rgb(0xab, 0xb2, 0xbf),
            status_bar_bg: Color32::from_rgb(0x21, 0x25, 0x2b),
            status_bar_fg: Color32::from_rgb(0x5c, 0x63, 0x70),
            selection_bg: Color32::from_rgba_premultiplied(0x44, 0x88, 0xff, 0x66),
            border_color: Color32::from_rgb(0x3e, 0x44, 0x51),
            terminal_colors: [
                0x282c34, 0xe06c75, 0x98c379, 0xe5c07b,
                0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
                0x5c6370, 0xe06c75, 0x98c379, 0xe5c07b,
                0x61afef, 0xc678dd, 0x56b6c2, 0xd4d4d4,
            ],
        }
    }
}
