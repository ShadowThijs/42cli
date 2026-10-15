//! Help overlay: every key binding, grouped per tab and overlay. Wide,
//! word-wrapped and scrollable.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::theme;
use crate::app::App;

/// (section title, [(keys, description)]).
const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "global",
        &[
            (
                "F1 .. F6 / Ctrl+← →",
                "switch tab (Dashboard, Projects, Slots, Search, User, Clusters)",
            ),
            ("r", "refresh the current tab, bypassing the cache"),
            ("n", "notifications overlay"),
            ("?", "this help"),
            ("L", "logout"),
            ("q / Ctrl+C", "quit"),
        ],
    ),
    (
        "help (this overlay)",
        &[
            ("↑ ↓ / j k", "scroll one line"),
            ("PgUp PgDn", "scroll one page"),
            ("g / G · Home End", "top / bottom"),
            ("? / q / Esc / Space", "close"),
        ],
    ),
    (
        "dashboard",
        &[
            ("↑ ↓ / j k", "move the selection in the evaluations to give"),
            (
                "Enter",
                "open the evaluation form of the selected duty (not open yet: press r closer to the start)",
            ),
        ],
    ),
    (
        "evaluation form (opened from the dashboard)",
        &[
            ("↑ ↓ / j k", "move between questions"),
            ("Tab / Shift+Tab", "cycle panes: questions → ratings → comment (Shift+Tab reverses)"),
            ("← → / h l", "adjust a slider · pick the rating flag"),
            ("y / n", "answer a yes/no question"),
            ("Enter", "on the comment: start typing · Esc stops typing"),
            ("g", "clone the evaluated repo (asks where)"),
            ("F", "finish: confirm popup, y submits"),
            ("Esc", "close the evaluation"),
        ],
    ),
    (
        "projects",
        &[
            ("/", "focus the name filter · Esc / Enter leaves it"),
            ("← →", "segment: active / available / done / all"),
            ("↑ ↓ / j k", "move the selection (in details: the document)"),
            ("Tab / Enter", "switch between list and details"),
            ("Esc / ← / Backspace", "details: back to the list"),
            ("d / Enter", "details: download the selected document"),
            ("v", "details: view the subject PDF as markdown"),
            ("g", "clone the project repo (asks where)"),
            ("f", "finish the project (confirm popup, y sends)"),
            ("x", "give up the project (confirm popup, y sends)"),
        ],
    ),
    (
        "subject viewer",
        &[
            ("↑ ↓ / j k", "scroll one line"),
            ("PgUp PgDn / u d", "scroll one page"),
            ("g G / Home End", "top / bottom"),
            ("Esc / q", "close"),
        ],
    ),
    (
        "slots",
        &[
            ("p / b", "project booking mode"),
            ("o", "my opening hours mode"),
            ("s", "sync the slots projects"),
            ("< > / PgUp PgDn", "previous / next week"),
            (
                "← → / h l",
                "projects strip: pick the project · grid: move one day",
            ),
            ("↑ ↓ / j k", "grid: move the cursor by 15 minutes"),
            (
                "Ctrl+d / Ctrl+u",
                "grid: half a screen down / up (also Ctrl+f / Ctrl+b)",
            ),
            ("Tab", "booking mode: projects strip ↔ week grid"),
            (
                "Enter",
                "book at the cursor · cancel yours · start/finish an hours range",
            ),
            ("Esc", "drop the range being dragged out"),
            ("d", "hours mode: close the hour under the cursor"),
            ("c", "toggle campus: bx ↔ anr"),
            ("t", "toggle inter-campus / remote"),
            (
                "colours",
                "campus colour = bookable/open · green = yours · red = booked · grey = too soon",
            ),
        ],
    ),
    (
        "search",
        &[
            ("type", "live user search (from 2 characters)"),
            ("↑ ↓", "move the selection"),
            ("Enter", "open the selected user in the User tab"),
        ],
    ),
    (
        "user",
        &[
            ("Tab", "switch ongoing ↔ marked projects"),
            ("↑ ↓ / j k", "move the project selection"),
            ("Enter", "open the selected project's team popup"),
            ("Esc", "back to Search"),
            ("popup: ↑ ↓ / j k", "scroll"),
            ("popup: PgUp PgDn", "scroll one page"),
            ("popup: Esc / q / Enter", "close"),
        ],
    ),
    ("clusters", &[("↑ ↓ / j k", "move between clusters")]),
    (
        "notifications overlay",
        &[
            ("↑ ↓ / j k", "move the selection"),
            ("Enter", "open the event detail / project"),
            ("s / u", "event detail: subscribe / unsubscribe"),
            ("Esc / n / q", "close"),
        ],
    ),
];

const KEY_COL: usize = 24;

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn build_lines(width: usize) -> Vec<Line<'static>> {
    let desc_width = width.saturating_sub(KEY_COL + 2).max(10);
    let mut lines = Vec::new();
    for (index, (title, bindings)) in SECTIONS.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("▌ {title}"),
            theme::title(),
        )));
        for (keys, action) in *bindings {
            for (row, text) in wrap(action, desc_width).into_iter().enumerate() {
                let key = if row == 0 {
                    (*keys).to_owned()
                } else {
                    String::new()
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {key:<width$}", width = KEY_COL - 2),
                        theme::bright(),
                    ),
                    Span::styled(text, theme::text()),
                ]));
            }
        }
    }
    lines
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width.saturating_sub(4).clamp(40, 110);
    let height = area.height.saturating_sub(6).max(8);
    let popup = centered(area, width, height);
    let lines_for = |inner_width: u16| build_lines(inner_width as usize);

    // Size from a first pass: the inner width is the popup minus borders.
    let inner_width = popup.width.saturating_sub(2);
    let lines = lines_for(inner_width);
    let view = popup.height.saturating_sub(2);
    let max_scroll = (lines.len() as u16).saturating_sub(view);
    app.help_max_scroll.set(max_scroll);
    let scroll = app.help_scroll.min(max_scroll);

    let title = if max_scroll > 0 {
        format!(
            " help · ↑↓ j k scroll · ? q Esc Space close · {}/{} ",
            scroll + 1,
            max_scroll + 1
        )
    } else {
        " help · ? q Esc Space close ".to_owned()
    };
    let block = theme::pane(false).title(Span::styled(title, theme::title()));
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
}

/// Keys while the overlay is open: scroll, or close.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    let max = app.help_max_scroll.get();
    let page = 10;
    match key.code {
        KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char(' ') => {
            app.help_open = false;
        }
        KeyCode::Down | KeyCode::Char('j') => app.help_scroll = (app.help_scroll + 1).min(max),
        KeyCode::Up | KeyCode::Char('k') => app.help_scroll = app.help_scroll.saturating_sub(1),
        KeyCode::PageDown => app.help_scroll = (app.help_scroll + page).min(max),
        KeyCode::PageUp => app.help_scroll = app.help_scroll.saturating_sub(page),
        KeyCode::Home | KeyCode::Char('g') => app.help_scroll = 0,
        KeyCode::End | KeyCode::Char('G') => app.help_scroll = max,
        _ => {}
    }
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
