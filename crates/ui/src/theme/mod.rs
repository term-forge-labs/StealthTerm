use egui::{Color32, Stroke, Visuals};
use serde::{Deserialize, Serialize};

pub mod tokyo_night;
pub mod catppuccin;
pub mod dracula;
pub mod monokai;
pub mod solarized_dark;
pub mod one_dark;

pub use tokyo_night::TokyoNightTheme;
pub use catppuccin::CatppuccinTheme;
pub use dracula::DraculaTheme;
pub use monokai::MonokaiTheme;
pub use solarized_dark::SolarizedDarkTheme;
pub use one_dark::OneDarkTheme;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThemeKind {
    Dracula,
    Monokai,
    TokyoNight,
    Catppuccin,
    SolarizedDark,
    OneDark,
    Custom,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub kind: ThemeKind,
    pub bg: Color32,
    pub fg: Color32,
    pub accent: Color32,
    pub tab_bg: Color32,
    pub tab_active_bg: Color32,
    pub tab_fg: Color32,
    pub sidebar_bg: Color32,
    pub sidebar_fg: Color32,
    pub input_bg: Color32,
    pub input_fg: Color32,
    pub status_bar_bg: Color32,
    pub status_bar_fg: Color32,
    pub selection_bg: Color32,
    pub border_color: Color32,
    pub terminal_colors: [u32; 16],
}

impl Theme {
    pub fn tokyo_night() -> Self {
        TokyoNightTheme::theme()
    }

    pub fn catppuccin() -> Self {
        CatppuccinTheme::mocha()
    }

    pub fn monokai() -> Self {
        MonokaiTheme::theme()
    }

    pub fn dracula() -> Self {
        DraculaTheme::theme()
    }

    pub fn solarized_dark() -> Self {
        SolarizedDarkTheme::theme()
    }

    pub fn one_dark() -> Self {
        OneDarkTheme::theme()
    }

    /// Create a theme from its kind
    pub fn from_kind(kind: &ThemeKind) -> Self {
        match kind {
            ThemeKind::Dracula => Self::dracula(),
            ThemeKind::Monokai => Self::monokai(),
            ThemeKind::TokyoNight => Self::tokyo_night(),
            ThemeKind::Catppuccin => Self::catppuccin(),
            ThemeKind::SolarizedDark => Self::solarized_dark(),
            ThemeKind::OneDark => Self::one_dark(),
            ThemeKind::Custom => Self::dracula(), // fallback
        }
    }

    /// Create a theme from the settings string name
    pub fn from_name(name: &str) -> Self {
        match name {
            "dracula" => Self::dracula(),
            "monokai" => Self::monokai(),
            "tokyo-night" => Self::tokyo_night(),
            "catppuccin" => Self::catppuccin(),
            "solarized-dark" => Self::solarized_dark(),
            "one-dark" => Self::one_dark(),
            _ => Self::dracula(),
        }
    }

    /// Get the settings string name for this theme
    pub fn name(&self) -> &'static str {
        match self.kind {
            ThemeKind::Dracula => "dracula",
            ThemeKind::Monokai => "monokai",
            ThemeKind::TokyoNight => "tokyo-night",
            ThemeKind::Catppuccin => "catppuccin",
            ThemeKind::SolarizedDark => "solarized-dark",
            ThemeKind::OneDark => "one-dark",
            ThemeKind::Custom => "custom",
        }
    }

    /// List all available theme names
    pub fn available_themes() -> &'static [(&'static str, &'static str)] {
        &[
            ("dracula", "Dracula"),
            ("monokai", "Monokai"),
            ("tokyo-night", "Tokyo Night"),
            ("catppuccin", "Catppuccin Mocha"),
            ("solarized-dark", "Solarized Dark"),
            ("one-dark", "One Dark"),
        ]
    }

    pub fn apply_to_egui(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        // UI chrome (sidebar, title bar, status bar, settings window, etc.) always uses light scheme
        // Terminal area draws its own dark background via CentralPanel.frame().fill() and TerminalView
        let mut visuals = Visuals::light();

        visuals.window_fill = self.sidebar_bg;
        visuals.panel_fill = self.sidebar_bg;
        visuals.faint_bg_color = Color32::from_rgb(0xf0, 0xf0, 0xf0);
        visuals.extreme_bg_color = Color32::WHITE;
        visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 2].into(),
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(40),
        };
        visuals.window_corner_radius = egui::CornerRadius::same(6);
        visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);

        // Use accent color to highlight selected state
        visuals.selection.bg_fill = Color32::from_rgba_premultiplied(
            self.accent.r(), self.accent.g(), self.accent.b(), 60,
        );

        style.visuals = visuals;
        ctx.set_style(style);
    }
}
