//! Cluster occupancy: who is logged in where, grouped by cluster.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::theme;
use super::widgets;
use crate::app::App;
use crate::input::Action;
use crate::util;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
        .spacing(1)
        .split(area);

    // Only the user's primary campus — clusters.json spans every campus
    // and their cluster names collide (a1/a2 exist on both).
    let campus = app
        .dash
        .campuses
        .data()
        .and_then(|campuses| campuses.iter().find(|campus| campus.is_primary))
        .map(|campus| (campus.id, campus.name.clone().unwrap_or_default()));
    let (campus_id, campus_name) = match campus {
        Some((id, name)) => (id, name),
        None => (None, "all campuses".to_owned()),
    };
    let rows = app.clusters.rows(campus_id);
    let total: usize = rows.iter().map(|row| row.seats.len()).sum();

    // Cluster list.
    let block = widgets::titled_block(" clusters ", false).title(
        Span::styled(
            format!(" {total} occupied · {campus_name} "),
            theme::muted(),
        )
        .into_right_aligned_line(),
    );
    let inner = block.inner(columns[0]);
    frame.render_widget(block, columns[0]);

    if let Some(message) = app.clusters.seats.failed() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(message, theme::error()))),
            inner,
        );
        return;
    }
    if app.clusters.seats.is_loading() {
        frame.render_widget(
            Paragraph::new(widgets::loading_line("loading…", app.tick)),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<6}", row.name), theme::bright()),
                Span::styled(format!("{:>3}", row.seats.len()), theme::text()),
                Span::styled(" seats", theme::muted()),
            ]))
        })
        .collect();
    if items.is_empty() {
        widgets::hint(frame, inner, "nobody logged in");
        return;
    }
    if let Some(last) = inner.rows().next_back() {
        // Footnote: the API only reports occupied seats, so empty clusters
        // (and empty campuses) never appear.
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "occupied only — empty clusters hidden",
                theme::muted(),
            ))),
            Rect {
                y: last.y,
                height: 1,
                ..inner
            },
        );
    }
    let mut state = ListState::default();
    state.select(Some(
        app.clusters.cluster_sel.min(rows.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme::selected()),
        inner,
        &mut state,
    );

    // Seats of the selected cluster.
    if let Some(row) = rows.get(app.clusters.cluster_sel) {
        let block = widgets::titled_block(&format!(" cluster {} ", row.name), false);
        let seats_area = block.inner(columns[1]);
        frame.render_widget(block, columns[1]);
        let mut lines: Vec<Line> = row
            .seats
            .iter()
            .map(|seat| {
                let since = seat
                    .begin_at
                    .as_deref()
                    .and_then(util::parse_datetime)
                    .map(|at| at.format("%H:%M").to_string())
                    .unwrap_or_else(|| "—".into());
                Line::from(vec![
                    Span::styled(
                        format!("{:<10}", seat.host.clone().unwrap_or_default()),
                        theme::text(),
                    ),
                    Span::styled(
                        format!("{:<12}", seat.login.clone().unwrap_or_default()),
                        theme::bright(),
                    ),
                    Span::styled(format!("since {since}"), theme::muted()),
                ])
            })
            .collect();
        if lines.len() > seats_area.height as usize {
            lines.truncate(seats_area.height as usize);
        }
        frame.render_widget(Paragraph::new(lines), seats_area);
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    let campus_id = app
        .dash
        .campuses
        .data()
        .and_then(|campuses| campuses.iter().find(|campus| campus.is_primary))
        .and_then(|campus| campus.id);
    let count = app.clusters.rows(campus_id).len();
    if count == 0 {
        return Action::Continue;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.clusters.cluster_sel = app.clusters.cluster_sel.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.clusters.cluster_sel = (app.clusters.cluster_sel + 1).min(count.saturating_sub(1))
        }
        _ => {}
    }
    Action::Continue
}
