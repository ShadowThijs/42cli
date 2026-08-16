//! Profile view for any student (opened from search).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme;
use super::widgets;
use crate::app::App;
use crate::input::Action;
use crate::util;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
        .spacing(1)
        .split(area);
    let rows_left = Layout::vertical([Constraint::Min(10), Constraint::Min(6)])
        .spacing(1)
        .split(columns[0]);

    widgets::loadable(
        frame,
        rows_left[0],
        &format!(" {} ", app.user.login),
        &app.user.profile,
        app.tick,
        |frame, area, profile| {
            let mut lines = vec![
                widgets::kv(
                    "login",
                    profile.login.clone().unwrap_or_default(),
                    theme::bright(),
                ),
                widgets::kv(
                    "name",
                    format!(
                        "{} {}",
                        profile.first_name.clone().unwrap_or_default(),
                        profile.last_name.clone().unwrap_or_default()
                    ),
                    theme::text(),
                ),
                widgets::kv(
                    "location",
                    profile.location.clone().unwrap_or_else(|| "—".into()),
                    if profile.location.is_some() {
                        theme::good()
                    } else {
                        theme::muted()
                    },
                ),
                widgets::kv(
                    "wallet",
                    format!("{} ₳", profile.wallet.unwrap_or_default()),
                    theme::warn(),
                ),
                widgets::kv(
                    "eval points",
                    profile.evaluation_points.unwrap_or_default().to_string(),
                    theme::bright(),
                ),
            ];
            if profile.is_active == Some(false) {
                lines.push(widgets::kv("status", "inactive", theme::error()));
            }
            if let Some(alumnized) = &profile.alumnized_at {
                lines.push(widgets::kv(
                    "alumnized",
                    util::fmt_date(alumnized),
                    theme::muted(),
                ));
            }
            frame.render_widget(Paragraph::new(lines), area);
        },
    );

    widgets::loadable(
        frame,
        rows_left[1],
        " logtime (30d) ",
        &app.user.logtime,
        app.tick,
        |frame, area, stats| {
            let month = util::logtime_last_days(stats, 30);
            frame.render_widget(widgets::logtime_sparkline(stats, 30), area);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("month {}", util::fmt_seconds(month)),
                    theme::muted(),
                ))),
                Rect {
                    y: area.bottom().saturating_sub(1),
                    height: 1,
                    ..area
                },
            );
        },
    );

    let rows_right = Layout::vertical([Constraint::Length(8), Constraint::Min(0)])
        .spacing(1)
        .split(columns[1]);
    widgets::loadable(
        frame,
        rows_right[0],
        " cursus ",
        &app.user.cursus,
        app.tick,
        |frame, area, cursus| {
            let lines: Vec<Line> = cursus
                .iter()
                .map(|entry| {
                    let level = entry.level.unwrap_or_default() as u32;
                    let percent = entry.progress.unwrap_or_default();
                    Line::from(vec![
                        Span::styled(
                            format!("{:<22}", entry.name.clone().unwrap_or_default()),
                            theme::text(),
                        ),
                        Span::styled(
                            format!(
                                "L{level} · {percent}% · {}",
                                entry.grade.clone().unwrap_or_default()
                            ),
                            theme::muted(),
                        ),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
    widgets::loadable(
        frame,
        rows_right[1],
        " achievements ",
        &app.user.achievements,
        app.tick,
        |frame, area, achievements| {
            let lines: Vec<Line> = achievements
                .iter()
                .take(area.height as usize)
                .map(|achievement| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:<10}", achievement.tier.clone().unwrap_or_default()),
                            ratatui::style::Style::default().fg(theme::MAGENTA),
                        ),
                        Span::styled(achievement.name.clone().unwrap_or_default(), theme::text()),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        },
    );

    // Patrons footer.
    let patrons: Vec<Line> = [
        (
            "tutors",
            app.user.patroning.data().map(|list| {
                list.iter()
                    .filter_map(|user| user.login.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
        ),
        (
            "tutor of",
            app.user.patroned.data().map(|list| {
                list.iter()
                    .filter_map(|user| user.login.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
        ),
    ]
    .iter()
    .filter_map(|(label, names)| {
        names
            .clone()
            .map(|names| widgets::kv(label, names, theme::text()))
    })
    .collect();
    if !patrons.is_empty() {
        let block = widgets::titled_block(" tutoring ", false);
        let inner = block.inner(rows_right[1]);
        let _ = inner;
        let area = Rect {
            height: 3,
            y: rows_right[1].bottom().saturating_sub(3),
            ..rows_right[1]
        };
        let block_area = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(patrons), block_area);
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    if key.code == KeyCode::Esc {
        app.enter_tab(crate::app::Tab::Search)
    }
    Action::Continue
}
