//! Shared widgets: loadable panes, spinners, input fields, key-value rows,
//! gauges and bar charts.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{BarChart, Block, Gauge, Paragraph};
use tui_input::Input;

use super::theme;
use crate::state::Loadable;

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn spinner(tick: u64) -> char {
    SPINNER[(tick as usize) % SPINNER.len()]
}

/// Render a pane whose content depends on a `Loadable`: spinner while
/// loading, error line on failure, `render` on success.
pub fn loadable<T>(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    state: &Loadable<T>,
    tick: u64,
    render: impl FnOnce(&mut Frame, Rect, &T),
) {
    let block = theme::pane(false).title(Span::styled(title, theme::title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match state {
        Loadable::Idle => hint(frame, inner, "press r to load"),
        Loadable::Loading => {
            frame.render_widget(Paragraph::new(loading_line("loading…", tick)), inner)
        }
        Loadable::Failed(message) => {
            let line = Line::from(Span::styled(format!("error: {message}"), theme::error()));
            frame.render_widget(Paragraph::new(line), inner)
        }
        Loadable::Ready(value) => render(frame, inner, value),
    }
}

/// Flicker-free spinner text with animated frame.
pub fn loading_line(label: &str, tick: u64) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{} ", spinner(tick)),
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(label.to_owned(), theme::muted()),
    ])
}

pub fn hint(frame: &mut Frame, area: Rect, message: &str) {
    let line = Line::from(Span::styled(message, theme::muted()));
    frame.render_widget(Paragraph::new(line), area);
}

/// One-line `key: value` row with colored key.
pub fn kv(key: &str, value: impl Into<String>, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<16}"), Style::default().fg(theme::DIM)),
        Span::styled(value.into(), value_style),
    ])
}

/// Simple masked/unmasked input field with optional focus ring.
pub fn input_field(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    input: &Input,
    focused: bool,
    masked: bool,
) {
    let display = if masked {
        "*".repeat(input.value().len())
    } else {
        input.value().to_owned()
    };
    let block = theme::pane(focused).title(Span::styled(label, theme::title()));
    let visible = display.chars().collect::<Vec<_>>();
    let inner = block.inner(area);
    let scroll = visible.len().saturating_sub(inner.width as usize);
    let shown: String = visible
        .iter()
        .skip(scroll)
        .take(inner.width as usize)
        .collect();
    let line = if focused {
        Line::from(vec![
            Span::styled(shown, Style::default().fg(theme::BRIGHT)),
            Span::styled("│", Style::default().fg(theme::ACCENT)),
        ])
    } else if shown.is_empty() {
        Line::from(Span::styled("…", theme::muted()))
    } else {
        Line::from(Span::styled(shown, theme::text()))
    };
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(line), inner);
}

/// Horizontal percentage gauge with label, e.g. `level 4 ▓▓▓░ 15%`.
pub fn mini_gauge(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    ratio: f64,
    color: ratatui::style::Color,
) {
    let gauge = Gauge::default()
        .ratio(ratio.clamp(0.0, 1.0))
        .label(Span::styled(
            format!("{label} {}%", (ratio * 100.0).round() as u32),
            Style::default()
                .fg(ratatui::style::Color::Black)
                .add_modifier(Modifier::BOLD),
        ))
        .gauge_style(Style::default().fg(color).bg(theme::DIM));
    frame.render_widget(gauge, area);
}

/// Bar chart of daily hours.
pub fn hours_chart(bars: &[(String, f64)]) -> BarChart<'_> {
    let data: Vec<(&str, u64)> = bars
        .iter()
        .map(|(label, hours)| (label.as_str(), hours.round() as u64))
        .collect();
    BarChart::default()
        .data(data.as_slice())
        .bar_style(Style::default().fg(theme::ACCENT))
        .value_style(Style::default().fg(theme::BRIGHT))
        .bar_width(1)
        .bar_gap(0)
        .max(10)
}

/// Standard pane block with title.
pub fn titled_block(title: &str, focused: bool) -> Block<'static> {
    theme::pane(focused).title(Span::styled(title.to_owned(), theme::title()))
}
