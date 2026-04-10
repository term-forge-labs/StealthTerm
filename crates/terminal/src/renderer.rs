// Terminal renderer - draws the terminal grid using egui
// This is a placeholder that will be fleshed out in the UI crate integration


/// Theme colors for terminal rendering
pub struct TerminalTheme {
    pub colors: [u32; 16],
    pub fg: [u8; 4],
    pub bg: [u8; 4],
    pub cursor_color: [u8; 4],
    pub selection_color: [u8; 4],
    pub match_highlight_color: [u8; 4],
}

impl Default for TerminalTheme {
    fn default() -> Self {
        // Dracula theme
        Self {
            colors: [
                0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c,
                0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
                0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5,
                0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
            ],
            fg: [0xf8, 0xf8, 0xf2, 0xff],
            bg: [0x26, 0x25, 0x22, 0xff],
            cursor_color: [0xff, 0xa5, 0x00, 0xff],
            selection_color: [0x44, 0x88, 0xff, 0x66],
            match_highlight_color: [0xff, 0xa5, 0x00, 0x44],
        }
    }
}
