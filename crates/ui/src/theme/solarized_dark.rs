use egui::Color32;
use super::{Theme, ThemeKind};

pub struct SolarizedDarkTheme;

impl SolarizedDarkTheme {
    pub fn theme() -> Theme {
        Theme {
            kind: ThemeKind::SolarizedDark,
            bg: Color32::from_rgb(0x00, 0x2b, 0x36),
            fg: Color32::from_rgb(0x83, 0x94, 0x96),
            accent: Color32::from_rgb(0x26, 0x8b, 0xd2),
            tab_bg: Color32::from_rgb(0x00, 0x1e, 0x26),
            tab_active_bg: Color32::from_rgb(0x00, 0x2b, 0x36),
            tab_fg: Color32::from_rgb(0x58, 0x6e, 0x75),
            sidebar_bg: Color32::from_rgb(0x00, 0x1e, 0x26),
            sidebar_fg: Color32::from_rgb(0x93, 0xa1, 0xa1),
            input_bg: Color32::from_rgb(0x07, 0x36, 0x42),
            input_fg: Color32::from_rgb(0x83, 0x94, 0x96),
            status_bar_bg: Color32::from_rgb(0x00, 0x1e, 0x26),
            status_bar_fg: Color32::from_rgb(0x58, 0x6e, 0x75),
            selection_bg: Color32::from_rgba_premultiplied(0x44, 0x88, 0xff, 0x66),
            border_color: Color32::from_rgb(0x07, 0x36, 0x42),
            terminal_colors: [
                0x073642, 0xdc322f, 0x859900, 0xb58900,
                0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
                0x002b36, 0xcb4b16, 0x586e75, 0x657b83,
                0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
            ],
        }
    }
}
