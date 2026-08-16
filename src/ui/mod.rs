//! Root drawing: header with tabs, per-tab content, status bar, overlays.

pub mod clusters;
pub mod dashboard;
pub mod help;
pub mod login;
pub mod projects;
pub mod search;
pub mod slots;
pub mod theme;
pub mod user;
pub mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, Paragraph};

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
    let level = app.dash.main_cursus().and_then(|cursus| cursus.level);
    let identity = match level {
        Some(level) => format!("{login} · {:.2}   ", level),
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
        .map(|notifications| notifications.len())
        .unwrap_or(0);
    if unread > 0 {
        spans.push(Span::styled(format!(" ✉ {unread} "), theme::warn()));
    }
    let hints = match app.tab {
        Tab::Dashboard => "1-6 tabs · n notifications · ? help",
        Tab::Projects => "/ filter · ←→ segment · Enter details · d download",
        Tab::Slots => "o/h mode · ←→ project · Enter book · f form · s sync",
        Tab::Search => "type to search · Enter open profile",
        Tab::User => "Esc back to search",
        Tab::Clusters => "↑↓ cluster",
    };
    let mut line = Line::from(spans);
    line.spans.push(Span::styled(
        format!(
            "{hints:>width$}",
            hints = hints,
            width = (area.width as usize).min(hints.len() + 2)
        ),
        theme::muted(),
    ));
    frame.render_widget(line, area);
}

fn draw_notifications(frame: &mut Frame, app: &App, area: Rect) {
    let popup = centered(area, 64, 20);
    frame.render_widget(Clear, popup);
    let block = theme::pane(false).title(Span::styled(
        " notifications (any key closes) ",
        theme::title(),
    ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    match &app.dash.notifications {
        Loadable::Ready(notifications) => {
            let items: Vec<ListItem> = notifications
                .iter()
                .take(inner.height as usize)
                .map(|notification| {
                    let text = notification_text(notification);
                    ListItem::new(Line::from(Span::styled(text, theme::text())))
                })
                .collect();
            if items.is_empty() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("none", theme::muted()))),
                    inner,
                );
            } else {
                frame.render_widget(List::new(items), inner);
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

/// Flatten a notification payload into one readable line.
fn notification_text(notification: &crate::api::models::Notification) -> String {
    let raw = &notification.raw;
    let author = raw["author"]["login"]
        .as_str()
        .or_else(|| raw["from"]["login"].as_str())
        .unwrap_or("intra");
    let verb = raw["verb"].as_str().unwrap_or("notification");
    let object = raw["data"]["name"]
        .as_str()
        .or_else(|| raw["data"]["project"]["name"].as_str())
        .or_else(|| raw["data"]["title"].as_str())
        .unwrap_or("");
    let when = notification
        .created_at
        .as_deref()
        .and_then(util::parse_datetime)
        .map(|at| at.format("%d %b %H:%M").to_string())
        .unwrap_or_default();
    format!("{when:<14}{author:<12} {verb} {object}")
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
