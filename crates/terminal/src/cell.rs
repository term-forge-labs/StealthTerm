use serde::{Deserialize, Serialize};

/// Terminal color representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    /// Default terminal color
    Default,
    /// Named 16 colors (0-15)
    Indexed(u8),
    /// 256-color palette (0-255)
    Palette(u8),
    /// 24-bit TrueColor
    Rgb(u8, u8, u8),
}

impl Color {
    pub fn to_rgba(&self, is_fg: bool, theme_colors: &[u32; 16], default_fg: [u8; 4], default_bg: [u8; 4]) -> [u8; 4] {
        match self {
            Color::Default => {
                if is_fg {
                    default_fg
                } else {
                    default_bg
                }
            }
            Color::Indexed(i) | Color::Palette(i) => {
                let idx = *i as usize;
                if idx < 16 {
                    let c = theme_colors[idx];
                    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]
                } else if idx < 232 {
                    // 216-color cube (6x6x6) - standard xterm palette
                    let i = idx - 16;
                    let levels = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
                    let r = levels[i / 36];
                    let g = levels[(i / 6) % 6];
                    let b = levels[i % 6];
                    [r, g, b, 255]
                } else {
                    // Grayscale ramp (24 shades)
                    let v = 8 + (idx - 232) * 10;
                    [v as u8, v as u8, v as u8, 255]
                }
            }
            Color::Rgb(r, g, b) => [*r, *g, *b, 255],
        }
    }
}

/// Cell attributes (SGR flags)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CellAttributes {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub strikethrough: bool,
    pub dim: bool,
    pub invisible: bool,
}

/// A single terminal cell (character + styling)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttributes,
    /// Wide character occupies 2 columns; second cell is placeholder
    pub wide: bool,
    pub wide_placeholder: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttributes::default(),
            wide: false,
            wide_placeholder: false,
        }
    }
}

impl Cell {
    pub fn blank() -> Self {
        Self::default()
    }

    pub fn with_char(ch: char) -> Self {
        Self { ch, ..Default::default() }
    }

    pub fn is_blank(&self) -> bool {
        self.ch == ' ' && self.fg == Color::Default && self.bg == Color::Default
    }
}
