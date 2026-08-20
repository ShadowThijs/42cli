//! User search over `profile.intra.42.fr/searches/search.json`.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use tui_input::backend::crossterm::EventHandler;

use super::theme;
use super::widgets;
use crate::app::App;
use crate::input::Action;

/// Minimum characters before we hit the API (it is a prefix search).
const MIN_QUERY: usize = 2;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);
    widgets::input_field(
        frame,
        rows[0],
        " search users (type a login prefix)",
        &app.search.input,
        true,
        false,
    );
    // Search is the most latency-sensitive pane — keep its loading state
    // deliberately minimal: single spinner line, no skeleton, no pane fill.
    let block = widgets::titled_block(" results ", false);
    let inner = block.inner(rows[1]);
    frame.render_widget(block, rows[1]);
    match &app.search.results {
        crate::state::Loadable::Idle => {
            let hint = if app.search.input.value().len() < MIN_QUERY {
                "type at least 2 characters to search"
            } else {
                "press enter to search"
            };
            widgets::hint(frame, inner, hint);
        }
        crate::state::Loadable::Loading => {
            frame.render_widget(
                ratatui::widgets::Paragraph::new(widgets::loading_line("searching…", app.tick)),
                inner,
            );
        }
        crate::state::Loadable::Failed(msg) => {
            frame.render_widget(
                ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                    msg.clone(),
                    theme::error(),
                ))),
                inner,
            );
        }
        crate::state::Loadable::Ready(results) => {
            if results.is_empty() {
                widgets::hint(frame, inner, "no matches");
                return;
            }
            let items: Vec<ListItem> = results
                .iter()
                .map(|result| {
                    ListItem::new(Line::from(Span::styled(
                        result.login.clone().unwrap_or_default(),
                        theme::text(),
                    )))
                })
                .collect();
            let mut state = ListState::default();
            state.select(Some(
                app.search.selection.min(results.len().saturating_sub(1)),
            ));
            frame.render_stateful_widget(
                List::new(items).highlight_style(theme::selected()),
                inner,
                &mut state,
            );
        }
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Up => {
            app.search.selection = app.search.selection.saturating_sub(1);
        }
        KeyCode::Down => {
            let max = app
                .search
                .results
                .data()
                .map(|results| results.len().saturating_sub(1))
                .unwrap_or(0);
            app.search.selection = (app.search.selection + 1).min(max);
        }
        KeyCode::Enter => {
            if let Some(login) = app
                .search
                .results
                .data()
                .and_then(|results| results.get(app.search.selection))
                .and_then(|result| result.login.clone())
            {
                app.open_user(&login);
            }
        }
        _ => {
            app.search
                .input
                .handle_event(&crossterm::event::Event::Key(key));
            maybe_search(app);
        }
    }
    Action::Continue
}

/// Debounce-free live search: only fire when the query actually changed.
fn maybe_search(app: &mut App) {
    let query = app.search.input.value().to_owned();
    if query.len() < MIN_QUERY || query == app.search.last_query {
        return;
    }
    app.search.last_query = query.clone();
    app.search.selection = 0;
    app.search.results.start();
    app.send(crate::bus::Command::Search { query });
}
