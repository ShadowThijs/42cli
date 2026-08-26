//! Dashboard: the signed-in student's profile — identity, level, wallet,
//! evaluation points, pace, logtime chart, attendance, achievements,
//! upcoming events and evaluation duties.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme;
use super::widgets;
use crate::api::models::{ScaleTeam, Slot};
use crate::api::slots::parse_slot_time;
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
    // Piscine students (e.g. cursus 65 `c-piscine-antwerp`) have no
    // pace-system profile — the API 404s. Show a friendly placeholder
    // instead of the raw error, mirroring the user view.
    if app.dash.is_piscine() && matches!(app.dash.pace, Loadable::Failed(_)) {
        let block = widgets::titled_block(" pace ", false);
        let inner = block.inner(rows[2]);
        frame.render_widget(block, rows[2]);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No pace — Piscine",
                theme::muted(),
            ))),
            inner,
        );
    } else {
        // With the pace system active there is no blackhole — the
        // milestone deadline replaces it.
        let blackhole = if app.dash.pace.data().is_some_and(|pace| pace.is_activated) {
            None
        } else {
            app.dash
                .main_cursus()
                .and_then(|cursus| cursus.blackholed_at.clone())
        };
        draw_pace_pane(frame, rows[2], app, blackhole);
    }

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

/// The pace pane for cursus students: pace, milestone deadline and the
/// blackhole fallback.
fn draw_pace_pane(frame: &mut Frame, area: Rect, app: &App, blackhole: Option<String>) {
    widgets::loadable(
        frame,
        area,
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
            let now = chrono::Local::now();
            let empty = Vec::new();
            let slots = app.dash.eval_slots.data().unwrap_or(&empty);
            let duties = eval_duties(teams, slots, now);
            if duties.is_empty() {
                widgets::hint(frame, area, "none upcoming");
                return;
            }
            let lines: Vec<Line> = duties
                .iter()
                .enumerate()
                .take(area.height as usize)
                .map(|(index, duty)| duty_line(duty, now, index == app.dash.duty_sel))
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
}

// ---------------------------------------------------------- evaluations ----

/// How far ahead the panel reveals who will be evaluated. A booked open
/// hour only materializes as an intra scale_team (carrying the project
/// and the corrected) shortly before it begins.
const REVEAL_MINUTES: i64 = 15;

/// One upcoming evaluation to give, from either source.
struct EvalDuty {
    at: chrono::DateTime<chrono::Local>,
    /// Intra scale-team id — the evaluation form's address. `None` for a
    /// booked open hour whose team has not materialized yet.
    scale_team_id: Option<u64>,
    /// `None` for a booked open hour whose intra team has not
    /// materialized yet — the slot feed has no project.
    project: Option<String>,
    /// Logins of the students to evaluate.
    logins: Vec<String>,
}

/// Merge both duty sources into one time-sorted list: intra scale_teams
/// (upcoming or still running) and booked open hours whose scale_team has
/// not appeared yet. Finished duties are dropped.
fn eval_duties(
    teams: &[ScaleTeam],
    slots: &[Slot],
    now: chrono::DateTime<chrono::Local>,
) -> Vec<EvalDuty> {
    let mut duties: Vec<EvalDuty> = teams
        .iter()
        .filter_map(|team| {
            let at = team.begin_at.as_deref().and_then(util::parse_datetime)?;
            // `duration` is hours in the intra payload; default one hour.
            let minutes = (team.duration.unwrap_or(1.0) * 60.0) as i64;
            let ends = at + chrono::Duration::minutes(minutes);
            (ends > now).then(|| EvalDuty {
                at,
                scale_team_id: team.id,
                project: team.project_name().map(str::to_owned),
                logins: team
                    .correcteds
                    .iter()
                    .filter_map(|corrected| corrected.login.clone())
                    .collect(),
            })
        })
        .collect();
    for slot in slots {
        let Some(at) = slot.start.as_deref().and_then(parse_slot_time) else {
            continue;
        };
        let Some(end) = slot.end.as_deref().and_then(parse_slot_time) else {
            continue;
        };
        if end <= now {
            continue;
        }
        // Drop the bare slot once the intra team for the same evaluation
        // exists — it carries the project and the corrected.
        let absorbed = duties
            .iter()
            .any(|duty| (duty.at - at).num_minutes().abs() <= 1);
        if !absorbed {
            duties.push(EvalDuty {
                at,
                scale_team_id: None,
                project: None,
                logins: Vec::new(),
            });
        }
    }
    duties.sort_by_key(|duty| duty.at);
    duties
}

/// One row: `Tue 26 Aug 15:00  codexion  in 2h 15m  → student` — the
/// corrected only appears inside the reveal window.
fn duty_line(
    duty: &EvalDuty,
    now: chrono::DateTime<chrono::Local>,
    selected: bool,
) -> Line<'static> {
    let minutes = (duty.at - now).num_minutes();
    let marker = if selected { "▸ " } else { "  " };
    let mut spans = vec![Span::raw(marker)];
    let style = |base: Style| {
        if selected {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        }
    };
    spans.push(Span::styled(
        format!("{}  ", duty.at.format("%a %d %b %H:%M")),
        style(Style::default().fg(theme::WARN)),
    ));
    match &duty.project {
        Some(project) => spans.push(Span::styled(project.clone(), style(theme::text()))),
        None => spans.push(Span::styled("open hour", style(theme::muted()))),
    }
    let countdown_style = if minutes < 0 {
        theme::good()
    } else if minutes <= REVEAL_MINUTES {
        theme::warn()
    } else {
        theme::muted()
    };
    spans.push(Span::styled(
        format!("  {}", util::fmt_until(duty.at, now)),
        style(countdown_style),
    ));
    if minutes <= REVEAL_MINUTES && !duty.logins.is_empty() {
        spans.push(Span::styled(
            format!("  → {}", duty.logins.join(", ")),
            style(theme::bright()),
        ));
    }
    Line::from(spans)
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    let duties = || {
        let now = chrono::Local::now();
        let no_teams: Vec<ScaleTeam> = Vec::new();
        let no_slots: Vec<Slot> = Vec::new();
        let teams = app.dash.scale_teams.data().unwrap_or(&no_teams);
        let slots = app.dash.eval_slots.data().unwrap_or(&no_slots);
        eval_duties(teams, slots, now)
    };
    let count = duties().len();
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if count > 0 {
                app.dash.duty_sel = (app.dash.duty_sel + 1).min(count - 1);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.dash.duty_sel = app.dash.duty_sel.saturating_sub(1);
        }
        KeyCode::Enter => {
            let duties = duties();
            if let Some(duty) = duties.get(app.dash.duty_sel.min(duties.len().saturating_sub(1))) {
                match duty.scale_team_id {
                    Some(id) => {
                        app.eval_view = Some(crate::state::EvalView::open(id));
                        app.send(crate::bus::Command::LoadEvalForm { id });
                    }
                    // The slot exists but the intra team does not — it
                    // appears once the evaluation is close (press r).
                    None => app.set_status("not open yet — press r closer to the start"),
                }
            }
        }
        _ => {}
    }
    Action::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Corrected;
    use chrono::TimeZone as _;

    /// `now` pinned to a fixed local instant; every fixture time is
    /// expressed relative to it so the tests hold in any timezone.
    fn clock() -> chrono::DateTime<chrono::Local> {
        chrono::Local
            .with_ymd_and_hms(2026, 8, 26, 12, 0, 0)
            .single()
            .expect("valid instant")
    }

    /// Fixture timestamp `minutes` away from `now`.
    fn at(now: chrono::DateTime<chrono::Local>, minutes: i64) -> String {
        (now + chrono::Duration::minutes(minutes)).to_rfc3339()
    }

    fn team(begin: &str, project: Option<&str>, login: Option<&str>) -> ScaleTeam {
        ScaleTeam {
            begin_at: Some(begin.into()),
            duration: Some(1.0),
            correcteds: vec![Corrected {
                login: login.map(str::to_owned),
            }],
            extra: serde_json::json!({ "team": { "project": { "name": project } } }),
            ..Default::default()
        }
    }

    fn slot(start: &str, end: &str) -> Slot {
        Slot {
            start: Some(start.into()),
            end: Some(end.into()),
            ..Default::default()
        }
    }

    #[test]
    fn finished_duties_are_dropped() {
        let now = clock();
        // Began 3h ago with a 1h duration — over.
        let teams = vec![team(&at(now, -180), Some("codexion"), Some("student"))];
        assert!(eval_duties(&teams, &[], now).is_empty());
        // Began 20min ago, still running — kept.
        let teams = vec![team(&at(now, 0), Some("codexion"), None)];
        let kept = eval_duties(&teams, &[], now - chrono::Duration::minutes(20));
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn duties_merge_sort_and_absorb() {
        let now = clock();
        let teams = vec![
            team(&at(now, 1560), Some("push_swap"), Some("later")),
            team(&at(now, 120), Some("codexion"), Some("student")),
        ];
        // Slot at the same time as the codexion team — absorbed; slot 5h
        // out stays as a bare open hour.
        let slots = vec![
            slot(&at(now, 120), &at(now, 3)),
            slot(&at(now, 300), &at(now, 360)),
            slot(&at(now, -120), &at(now, -60)), // finished — dropped
        ];
        let duties = eval_duties(&teams, &slots, now);
        assert_eq!(duties.len(), 3);
        assert_eq!(duties[0].project.as_deref(), Some("codexion"));
        assert_eq!(duties[1].project, None);
        assert_eq!(duties[2].project.as_deref(), Some("push_swap"));
    }

    #[test]
    fn logins_only_reveal_inside_the_window() {
        let now = clock();
        let far = eval_duties(
            &[team(&at(now, 120), Some("codexion"), Some("student"))],
            &[],
            now,
        );
        let near = eval_duties(
            &[team(&at(now, 10), Some("codexion"), Some("student"))],
            &[],
            now,
        );
        let text = |duty: &EvalDuty| {
            duty_line(duty, now, false)
                .spans
                .iter()
                .map(|span| span.content.to_string())
                .collect::<String>()
        };
        assert!(!text(&far[0]).contains("student"));
        assert!(text(&near[0]).contains("→ student"));
        // Already started — still revealed.
        let mut running_team = team(&at(now, -60), Some("codexion"), Some("student"));
        running_team.duration = Some(2.0); // began 1h ago, ends in 1h
        let running = eval_duties(&[running_team], &[], now);
        assert!(text(&running[0]).contains("→ student"));
    }
}
