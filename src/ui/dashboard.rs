//! Dashboard: the signed-in student's profile — identity, level, wallet,
//! evaluation points, pace, logtime chart, attendance, achievements,
//! upcoming events and evaluation duties.

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme;
use super::widgets;
use crate::app::App;
use crate::input::Action;
use crate::state::Loadable;
use crate::util;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
        .spacing(1)
        .split(area);

    draw_identity(frame, app, columns[0]);
    draw_right(frame, app, columns[1]);
}

fn draw_identity(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(12),
        Constraint::Length(3),
        Constraint::Length(6),
        Constraint::Min(6),
    ])
    .spacing(1)
    .split(area);

    // -- identity ------------------------------------------------------
    let block = widgets::titled_block(" profile ", false);
    let inner = block.inner(rows[0]);
    frame.render_widget(block, rows[0]);
    match (&app.dash.profile, &app.dash.campuses) {
        (Loadable::Ready(profile), _) => {
            let cursus = app.dash.main_cursus();
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
            if let Some(cursus) = cursus {
                lines.push(widgets::kv(
                    "grade",
                    cursus.grade.clone().unwrap_or_else(|| "—".into()),
                    theme::text(),
                ));
            }
            if let Loadable::Ready(campuses) = &app.dash.campuses
                && let Some(primary) = campuses.iter().find(|campus| campus.is_primary)
            {
                lines.push(widgets::kv(
                    "campus",
                    primary.name.clone().unwrap_or_default(),
                    theme::text(),
                ));
            }
            let groups: Vec<String> = profile
                .groups
                .iter()
                .filter_map(|group| group.name.clone())
                .collect();
            if !groups.is_empty() {
                lines.push(widgets::kv("groups", groups.join(", "), theme::text()));
            }
            frame.render_widget(Paragraph::new(lines), inner);
        }
        (Loadable::Loading, _) => frame.render_widget(
            Paragraph::new(widgets::loading_line("loading profile…", app.tick)),
            inner,
        ),
        (Loadable::Failed(message), _) => frame.render_widget(
            Paragraph::new(Line::from(Span::styled(message.clone(), theme::error()))),
            inner,
        ),
        _ => widgets::hint(frame, inner, "waiting…"),
    }

    // -- level gauge ----------------------------------------------------
    let gauge_block = widgets::titled_block(" level ", false);
    let gauge_area = gauge_block.inner(rows[1]);
    frame.render_widget(gauge_block, rows[1]);
    if let Some(cursus) = app.dash.main_cursus() {
        let level = cursus.level.unwrap_or_default() as u32;
        let percent = cursus.progress.unwrap_or_default().min(100);
        widgets::mini_gauge(
            frame,
            gauge_area,
            &format!("level {level}"),
            percent as f64 / 100.0,
            theme::ACCENT,
        );
    } else {
        widgets::hint(frame, gauge_area, "…");
    }

    // -- pace / blackhole ------------------------------------------------
    // With the pace system active there is no blackhole — the milestone
    // deadline replaces it.
    let blackhole = if app.dash.pace.data().is_some_and(|pace| pace.is_activated) {
        None
    } else {
        app.dash
            .main_cursus()
            .and_then(|cursus| cursus.blackholed_at.clone())
    };
    widgets::loadable(
        frame,
        rows[2],
        " pace ",
        &app.dash.pace,
        app.tick,
        |frame, area, pace| {
            let mut lines = vec![widgets::kv(
                "pace",
                format!("{} days / level", pace.pace.unwrap_or_default()),
                theme::text(),
            )];
            if let Some(deadline) = &pace.deadline {
                let deadline_date = util::parse_datetime(deadline).map(|at| at.date_naive());
                let left = util::days_until(deadline).unwrap_or_default();
                lines.push(widgets::kv(
                    "milestone",
                    format!(
                        "L{} · {} ({} days left)",
                        pace.milestone.unwrap_or_default(),
                        util::fmt_date(deadline),
                        left
                    ),
                    if left < 0 {
                        theme::error()
                    } else if left < 14 {
                        theme::warn()
                    } else {
                        theme::good()
                    },
                ));
                if let Some(deadline_date) = deadline_date {
                    let start = util::pace_milestone_start(pace);
                    if let Some(start) = start {
                        let total = (deadline_date - start).num_days().max(1);
                        let elapsed = (chrono::Local::now().date_naive() - start).num_days();
                        lines.push(widgets::kv(
                            "progress",
                            format!("{elapsed} / {total} days in milestone"),
                            theme::text(),
                        ));
                    }
                }
            }
            if let Some(blackhole) = &blackhole {
                let days = util::days_until(blackhole).unwrap_or_default();
                lines.push(widgets::kv(
                    "blackhole",
                    format!("{} · {} days", util::fmt_date(blackhole), days),
                    if days < 0 {
                        theme::error()
                    } else if days < 14 {
                        theme::warn()
                    } else {
                        theme::text()
                    },
                ));
            }
            frame.render_widget(Paragraph::new(lines), area);
        },
    );

    // -- achievements -----------------------------------------------------
    widgets::loadable(
        frame,
        rows[3],
        " achievements ",
        &app.dash.achievements,
        app.tick,
        |frame, area, achievements| {
            let lines: Vec<Line> = achievements
                .iter()
                .take(area.height as usize)
                .map(|achievement| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:<10}", achievement.tier.clone().unwrap_or_default()),
                            Style::default().fg(theme::MAGENTA),
                        ),
                        Span::styled(achievement.name.clone().unwrap_or_default(), theme::text()),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
}

fn draw_right(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Percentage(42),
        Constraint::Percentage(28),
        Constraint::Percentage(30),
    ])
    .spacing(1)
    .split(area);

    // -- logtime ---------------------------------------------------------
    widgets::loadable(
        frame,
        rows[0],
        " logtime ",
        &app.dash.logtime,
        app.tick,
        |frame, area, stats| {
            let chart_area = Rect {
                height: area.height.saturating_sub(2),
                ..area
            };
            frame.render_widget(widgets::logtime_sparkline(stats, 30), chart_area);
            let week = util::logtime_last_days(stats, 7);
            let month = util::logtime_last_days(stats, 30);
            let today = stats
                .get(&chrono::Local::now().format("%Y-%m-%d").to_string())
                .map(|value| util::hms_to_seconds(value))
                .unwrap_or(0);
            let footer = Line::from(vec![
                Span::styled("today ", theme::muted()),
                Span::styled(util::fmt_seconds(today), theme::bright()),
                Span::styled("   week ", theme::muted()),
                Span::styled(util::fmt_seconds(week), theme::text()),
                Span::styled("   month ", theme::muted()),
                Span::styled(util::fmt_seconds(month), theme::text()),
            ]);
            frame.render_widget(
                Paragraph::new(footer),
                Rect {
                    y: area.bottom().saturating_sub(1),
                    height: 1,
                    ..area
                },
            );
        },
    );

    // -- attendance + events ---------------------------------------------
    let bottom =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(rows[1]);
    widgets::loadable(
        frame,
        bottom[0],
        " attendance ",
        &app.dash.attendance,
        app.tick,
        |frame, area, summary| {
            let mut lines: Vec<Line> = summary
                .weeks
                .iter()
                .take(area.height as usize)
                .map(|week| {
                    let seconds = week
                        .total
                        .as_deref()
                        .map(util::iso_duration_to_seconds)
                        .unwrap_or(0);
                    Line::from(vec![
                        Span::styled(
                            format!("{}  ", week.at.clone().unwrap_or_default()),
                            theme::muted(),
                        ),
                        Span::styled(util::fmt_seconds(seconds), theme::text()),
                    ])
                })
                .collect();
            if lines.is_empty() {
                lines.push(Line::from(Span::styled("no data", theme::muted())));
            }
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
    widgets::loadable(
        frame,
        bottom[1],
        " events ",
        &app.dash.events,
        app.tick,
        |frame, area, events| {
            let lines: Vec<Line> = events
                .iter()
                .take(area.height as usize)
                .map(|event| {
                    let subscribed = if event.is_subscribed { " ✓" } else { "" };
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "{}  ",
                                util::fmt_datetime(event.begin_at.as_deref().unwrap_or("—"))
                            ),
                            Style::default().fg(theme::ACCENT),
                        ),
                        Span::styled(
                            format!("{}{}", event.name.clone().unwrap_or_default(), subscribed),
                            theme::text(),
                        ),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        },
    );

    // -- evaluation duties --------------------------------------------------
    widgets::loadable(
        frame,
        rows[2],
        " evaluations to give ",
        &app.dash.scale_teams,
        app.tick,
        |frame, area, teams| {
            let future: Vec<Line> = teams
                .iter()
                .filter(|team| {
                    team.begin_at
                        .as_deref()
                        .and_then(util::parse_datetime)
                        .is_some_and(|at| at > chrono::Local::now())
                })
                .take(area.height as usize)
                .map(|team| {
                    let correcteds: Vec<String> = team
                        .correcteds
                        .iter()
                        .filter_map(|corrected| corrected.login.clone())
                        .collect();
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "{}  ",
                                util::fmt_datetime(team.begin_at.as_deref().unwrap_or("—"))
                            ),
                            Style::default().fg(theme::WARN),
                        ),
                        Span::styled(correcteds.join(", "), theme::text()),
                    ])
                })
                .collect();
            if future.is_empty() {
                widgets::hint(frame, area, "none upcoming");
            } else {
                frame.render_widget(Paragraph::new(future), area);
            }
        },
    );
}

pub fn handle_key(_app: &mut App, _key: KeyEvent) -> Action {
    Action::Continue
}
