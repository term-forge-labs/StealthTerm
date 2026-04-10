use egui::Color32;
use super::{Theme, ThemeKind};

pub struct CatppuccinTheme;

impl CatppuccinTheme {
    pub fn mocha() -> Theme {
        Theme {
            kind: ThemeKind::Catppuccin,
            bg: Color32::from_rgb(0x1e, 0x1e, 0x2e),
            fg: Color32::from_rgb(0xcd, 0xd6, 0xf4),
            accent: Color32::from_rgb(0x89, 0xb4, 0xfa),
            tab_bg: Color32::from_rgb(0x18, 0x18, 0x25),
            tab_active_bg: Color32::from_rgb(0x1e, 0x1e, 0x2e),
            tab_fg: Color32::from_rgb(0x6c, 0x70, 0x86),
            sidebar_bg: Color32::from_rgb(0x18, 0x18, 0x25),
            sidebar_fg: Color32::from_rgb(0xba, 0xc2, 0xde),
            input_bg: Color32::from_rgb(0x11, 0x11, 0x1b),
            input_fg: Color32::from_rgb(0xcd, 0xd6, 0xf4),
            status_bar_bg: Color32::from_rgb(0x18, 0x18, 0x25),
            status_bar_fg: Color32::from_rgb(0x6c, 0x70, 0x86),
            selection_bg: Color32::from_rgba_premultiplied(0x44, 0x88, 0xff, 0x66),
            border_color: Color32::from_rgb(0x31, 0x32, 0x44),
            terminal_colors: [
                0x1e1e2e, 0xf38ba8, 0xa6e3a1, 0xf9e2af,
                0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de,
                0x585b70, 0xf38ba8, 0xa6e3a1, 0xf9e2af,
                0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xcdd6f4,
            ],
        }
    }
}
