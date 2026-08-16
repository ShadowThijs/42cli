//! Help overlay listing every key binding.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::theme;

const BINDINGS: &[(&str, &str)] = &[
    ("1..6 / Tab", "switch tab"),
    ("r", "refresh current tab (bypass cache)"),
    ("n", "notifications overlay"),
    ("?", "this help"),
    ("q / Ctrl+C", "quit"),
    ("L", "logout"),
    ("", ""),
    ("— projects —", ""),
    ("/", "focus filter"),
    ("← →", "segment: active / available / done / all"),
    ("↑ ↓ / j k", "move selection"),
    ("Enter", "open details"),
    ("↑ ↓ + d", "download selected document"),
    ("Esc", "back to the list"),
    ("", ""),
    ("— slots —", ""),
    ("o / h", "project booking / open hours"),
    ("← →", "previous / next project"),
    ("↑ ↓ + Enter", "select slot, book (★ = cancel)"),
    ("f", "focus the open-hour form"),
    ("c / t", "campus / inter-campus toggle"),
    ("d", "close selected open hour"),
    ("s", "sync slots projects"),
];

pub fn draw(frame: &mut Frame, area: Rect) {
    let popup = centered(area, 46, 30);
    let block = theme::pane(false).title(Span::styled(" help ", theme::title()));
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    let lines: Vec<Line> = BINDINGS
        .iter()
        .map(|(keys, action)| {
            if action.is_empty() && !keys.is_empty() {
                Line::from(Span::styled(*keys, theme::title()))
            } else {
                Line::from(vec![
                    Span::styled(format!("{keys:<16}"), super::theme::bright()),
                    Span::styled(*action, theme::text()),
                ])
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .split(vertical[1]);
    horizontal[1]
}
