//! Central palette + style helpers (dark, low-noise, cyan accent).

use ratatui::style::{Color, Modifier, Style};

pub const ACCENT: Color = Color::LightCyan;
pub const DIM: Color = Color::DarkGray;
pub const TEXT: Color = Color::Gray;
pub const BRIGHT: Color = Color::White;
pub const GOOD: Color = Color::LightGreen;
pub const WARN: Color = Color::LightYellow;
pub const ERR: Color = Color::LightRed;
pub const MAGENTA: Color = Color::LightMagenta;
pub const CAMPUS_ANR: Color = Color::LightYellow;
pub const CAMPUS_BX: Color = Color::LightBlue;

pub fn title() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn pane(focused: bool) -> ratatui::widgets::Block<'static> {
    let border = if focused { ACCENT } else { DIM };
    ratatui::widgets::Block::default()
        .border_style(Style::default().fg(border))
        .borders(ratatui::widgets::Borders::ALL)
}

pub fn selected() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::default().fg(DIM)
}

pub fn text() -> Style {
    Style::default().fg(TEXT)
}

pub fn bright() -> Style {
    Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD)
}

pub fn error() -> Style {
    Style::default().fg(ERR)
}

pub fn good() -> Style {
    Style::default().fg(GOOD)
}

pub fn warn() -> Style {
    Style::default().fg(WARN)
}

/// Color for a project state chip.
pub fn state_color(state: &str) -> Color {
    match state {
        "done" | "finished" => GOOD,
        "ongoing" | "in_progress" | "subscribed" => ACCENT,
        "available" | "registered" => WARN,
        "locked" | "unavailable" => DIM,
        "failed" => ERR,
        _ => TEXT,
    }
}

/// Campus code -> color.
pub fn campus_color(code: &str) -> Color {
    if code.ends_with("anr") {
        CAMPUS_ANR
    } else {
        CAMPUS_BX
    }
}
