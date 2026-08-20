//! Root drawing: header with tabs, per-tab content, status bar, overlays.

pub mod clusters;
pub mod dashboard;
pub mod markdown;
pub mod help;
pub mod login;
pub mod projects;
pub mod search;
pub mod slots;
pub mod subject;
pub mod theme;
pub mod user;
pub mod week_grid;
pub mod widgets;
pub mod wrap;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph};

use crate::app::{App, Screen, Tab};
use crate::state::Loadable;
use crate::util;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if app.screen == Screen::Login {
        login::draw(frame, app, area);
        draw_status(frame, app, bottom_bar(area));
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .spacing(0)
    .split(area);
    draw_header(frame, app, rows[0]);

    match app.tab {
        Tab::Dashboard => dashboard::draw(frame, app, rows[1]),
        Tab::Projects => projects::draw(frame, app, rows[1]),
        Tab::Slots => slots::draw(frame, app, rows[1]),
        Tab::Search => search::draw(frame, app, rows[1]),
        Tab::User => user::draw(frame, app, rows[1]),
        Tab::Clusters => clusters::draw(frame, app, rows[1]),
    }

    draw_status(frame, app, rows[2]);

    if app.subject_view.is_some() {
        subject::draw(frame, app, area);
        return;
    }
    if app.notifications_open {
        draw_notifications(frame, app, area);
    }
    if app.help_open {
        help::draw(frame, area);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::styled(
        " 42cli ",
        ratatui::style::Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(theme::ACCENT),
    )];
    for tab in Tab::ORDER {
        let style = if tab == app.tab {
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Black)
                .bg(theme::BRIGHT)
        } else {
            theme::muted()
        };
        spans.push(Span::styled(format!(" {} ", tab.title()), style));
        spans.push(Span::raw(" "));
    }
    // Right-aligned identity summary.
    let login = app
        .dash
        .summary
        .data()
        .and_then(|me| me.login.clone())
        .unwrap_or_default();
    let identity = match app.dash.main_cursus() {
        Some(cursus) => format!(
            "{login} · L{} {}%   ",
            cursus.level.unwrap_or_default() as u32,
            cursus.progress.unwrap_or_default()
        ),
        None => format!("{login}   "),
    };
    spans.push(Span::styled(identity, theme::text()));
    spans.push(Span::styled(util::now_brussels(), theme::muted()));
    frame.render_widget(Line::from(spans), area);
}

fn bottom_bar(area: Rect) -> Rect {
    Rect {
        y: area.bottom().saturating_sub(1),
        height: 1,
        ..area
    }
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    if let Some(message) = app.status_message().filter(|message| !message.is_empty()) {
        spans.push(Span::styled(format!(" {message} "), theme::good()));
    }
    let unread = app
        .dash
        .notifications
        .data()
        .map(|notifications| notifications.unread)
        .unwrap_or(0);
    if unread > 0 {
        spans.push(Span::styled(format!(" ✉ {unread} "), theme::warn()));
    }
    let hints = match app.tab {
        Tab::Dashboard => "F1-F6 tabs · n notifications · ? help",
        Tab::Projects => "/ filter · ←→ segment · Tab pane · d download · v view subject",
        Tab::Slots => "p/o mode · ←→ project · Enter book · Tab form · s sync",
        Tab::Search => "type to search · Enter open profile",
        Tab::User => "Esc back to search",
        Tab::Clusters => "↑↓ cluster",
    };
    let mut line = Line::from(spans);
    line.spans
        .push(Span::styled(format!("  {hints}"), theme::muted()));
    frame.render_widget(line, area);
}

fn draw_notifications(frame: &mut Frame, app: &App, area: Rect) {
    // The event detail popup replaces the notification list.
    if let Some(popup) = &app.event_popup {
        draw_event_popup(frame, app, popup, area);
        return;
    }
    let popup = centered(area, 64, 20);
    frame.render_widget(Clear, popup);
    let block = theme::pane(false).title(Span::styled(
        " notifications (j/k select · Enter open · Esc close) ",
        theme::title(),
    ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    match &app.dash.notifications {
        Loadable::Ready(payload) => {
            let unread_seen = payload.unread;
            let width = inner.width as usize;
            let items: Vec<ListItem> = payload
                .items
                .iter()
                .enumerate()
                .map(|(index, notification)| {
                    let text = notification_text(notification, index < unread_seen);
                    let style = if index < unread_seen {
                        theme::bright()
                    } else {
                        theme::text()
                    };
                    ListItem::new(Line::from(Span::styled(
                        util::truncate_str(&text, width),
                        style,
                    )))
                })
                .collect();
            if items.is_empty() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("none", theme::muted()))),
                    inner,
                );
            } else {
                let mut state = ListState::default();
                state.select(Some(
                    app.notifications_sel.min(items.len().saturating_sub(1)),
                ));
                frame.render_stateful_widget(
                    List::new(items).highlight_style(theme::selected()),
                    inner,
                    &mut state,
                );
            }
        }
        Loadable::Loading => frame.render_widget(
            Paragraph::new(widgets::loading_line("loading…", app.tick)),
            inner,
        ),
        Loadable::Failed(message) => frame.render_widget(
            Paragraph::new(Line::from(Span::styled(message.clone(), theme::error()))),
            inner,
        ),
        Loadable::Idle => {}
    }
}

fn draw_event_popup(
    frame: &mut Frame,
    app: &App,
    popup_state: &crate::state::EventPopup,
    area: Rect,
) {
    let popup = centered(area, 64, 22);
    frame.render_widget(Clear, popup);
    // The footer action decides which key hint applies.
    let title = match &popup_state.event {
        Loadable::Ready(event) if event.subscribe_url.is_some() => {
            " event (s subscribe · Esc close) "
        }
        Loadable::Ready(event) if event.unsubscribe_url.is_some() => {
            " event (u unsubscribe · Esc close) "
        }
        _ => " event (Esc close) ",
    };
    let block = theme::pane(false).title(Span::styled(title, theme::title()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    match &popup_state.event {
        Loadable::Loading => frame.render_widget(
            Paragraph::new(widgets::loading_line("loading event…", app.tick)),
            inner,
        ),
        Loadable::Failed(message) => frame.render_widget(
            Paragraph::new(Line::from(Span::styled(message.clone(), theme::error()))),
            inner,
        ),
        Loadable::Idle => {}
        Loadable::Ready(event) => {
            let mut lines: Vec<Line> = vec![Line::from(Span::styled(
                event.name.clone().unwrap_or_default(),
                theme::bright(),
            ))];
            if let Some(kind) = &event.kind {
                lines.push(widgets::kv("kind", kind.clone(), theme::text()));
            }
            if let Some(begin) = &event.begin_at {
                lines.push(widgets::kv(
                    "begins",
                    util::fmt_datetime(begin),
                    theme::text(),
                ));
            }
            if let Some(end) = &event.end_at {
                lines.push(widgets::kv("ends", util::fmt_datetime(end), theme::text()));
            }
            if let Some(duration) = &event.duration {
                lines.push(widgets::kv("duration", duration.clone(), theme::text()));
            }
            if let Some(location) = &event.location {
                lines.push(widgets::kv("location", location.clone(), theme::text()));
            }
            match (event.current_subscribers, event.max_subscribers) {
                (Some(current), Some(max)) => lines.push(widgets::kv(
                    "sign-ups",
                    format!("{current} / {max}"),
                    theme::text(),
                )),
                (Some(current), None) => {
                    lines.push(widgets::kv("sign-ups", current.to_string(), theme::text()))
                }
                _ => {}
            }
            if event.is_subscribed {
                lines.push(widgets::kv("status", "subscribed ✓", theme::good()));
            }
            if let Some(description) = &event.description {
                let remaining = (inner.height as usize).saturating_sub(lines.len() + 2);
                if remaining > 0 {
                    lines.push(Line::from(""));
                    lines.extend(util::wrap_lines(
                        description,
                        inner.width as usize,
                        remaining,
                    ));
                }
            }
            frame.render_widget(Paragraph::new(lines), inner);
        }
    }
}

/// One readable line per notification: date, then title — text.
fn notification_text(notification: &crate::api::models::Notification, unread: bool) -> String {
    let when = notification
        .created_at
        .as_deref()
        .and_then(util::parse_datetime)
        .map(|at| at.format("%d %b %H:%M").to_string())
        .unwrap_or_default();
    let marker = if unread { "●" } else { " " };
    let title = notification.title.clone().unwrap_or_default();
    let text = notification.text.clone().unwrap_or_default();
    format!("{marker} {when:<13}{title} — {text}")
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
