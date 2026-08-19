//! Help overlay listing every key binding.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::theme;

const BINDINGS: &[(&str, &str)] = &[
    ("global", ""),
    ("F1 .. F6 / Ctrl+← →", "switch tab"),
    ("Tab", "move between panes"),
    ("r", "refresh current tab (bypass cache)"),
    ("n", "notifications overlay"),
    ("?", "this help"),
    ("q / Ctrl+C", "quit"),
    ("L", "logout"),
    ("", ""),
    ("projects", ""),
    ("/", "focus filter · Esc leaves"),
    ("← →", "segment: active / available / done / all"),
    ("↑ ↓ / j k", "move selection"),
    ("Tab / Enter", "focus details"),
    ("d", "download selected document"),
    ("g", "clone the project repo (asks where)"),
    ("", ""),
    ("notifications", ""),
    ("n", "open the overlay"),
    ("↑ ↓ / j k", "move selection"),
    ("Enter", "open event detail / project"),
    ("s / u", "subscribe / unsubscribe event"),
    ("Esc / n", "close overlay"),
    ("", ""),
    ("slots", ""),
    ("p / o", "project booking / my hours"),
    ("Tab", "projects strip ↔ week grid"),
    ("↑ ↓ ← →", "move the calendar cursor (15 min)"),
    ("< > / PgUp PgDn", "previous / next week"),
    ("Enter", "book at the cursor · cancel yours · open a range"),
    ("d", "close the hour under the cursor"),
    ("c / t", "campus / inter-campus toggle"),
    ("s", "sync slots projects"),
    (
        "blocks (24 h grid)",
        "campus colour = bookable/open · green = yours · red = booked · grey = too soon",
    ),
];

pub fn draw(frame: &mut Frame, area: Rect) {
    let popup = centered(area, 48, 26);
    let block = theme::pane(false).title(Span::styled(" help ", theme::title()));
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    let lines: Vec<Line> = BINDINGS
        .iter()
        .map(|(keys, action)| {
            if action.is_empty() {
                Line::from(Span::styled(*keys, theme::title()))
            } else {
                Line::from(vec![
                    Span::styled(format!("{keys:<18}"), theme::bright()),
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
