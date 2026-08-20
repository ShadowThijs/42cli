//! Profile view for any student (opened from search).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::theme;
use super::widgets;
use crate::app::App;
use crate::input::Action;
use crate::state::{Loadable, UserProjectFocus};
use crate::util;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    // Requested layout:
    // | user profile | cursus |
    // | pace         | open   |
    // | logtime      | completed |
    // |              | achievements |
    let columns = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
        .spacing(1)
        .split(area);

    let rows_left = Layout::vertical([
        Constraint::Min(10),
        Constraint::Length(6),
        Constraint::Min(7),
    ])
    .spacing(1)
    .split(columns[0]);

    draw_profile(frame, app, rows_left[0]);
    draw_pace(frame, app, rows_left[1]);
    draw_logtime(frame, app, rows_left[2]);

    let rows_right = Layout::vertical([
        Constraint::Min(4),
        Constraint::Length(6),
        Constraint::Length(10),
        Constraint::Min(3),
    ])
    .spacing(1)
    .split(columns[1]);

    draw_cursus(frame, app, rows_right[0]);
    draw_ongoing(frame, app, rows_right[1]);
    draw_marked(frame, app, rows_right[2]);
    draw_achievements(frame, app, rows_right[3]);

    if app.user.popup.is_some() {
        draw_popup(frame, app, area);
    }
}

fn draw_profile(frame: &mut Frame, app: &App, area: Rect) {
    widgets::loadable(
        frame,
        area,
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
            let groups: Vec<String> = profile
                .groups
                .iter()
                .filter_map(|g| g.name.clone())
                .collect();
            if !groups.is_empty() {
                lines.push(widgets::kv("groups", groups.join(", "), theme::text()));
            }
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
            if let Some(campus) = app
                .user
                .campuses
                .data()
                .and_then(|cs| cs.iter().find(|c| c.is_primary))
            {
                lines.push(widgets::kv(
                    "campus",
                    campus.name.clone().unwrap_or_default(),
                    theme::text(),
                ));
            }
            // Keep all lines if possible, truncate only if truly overflow.
            // Height 9 fits 7-8 lines without huge whitespace (was 10).
            let max = area.height as usize;
            if lines.len() > max && max > 0 {
                lines.truncate(max);
            }
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
}

fn draw_cursus(frame: &mut Frame, app: &App, area: Rect) {
    widgets::loadable(
        frame,
        area,
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
                            format!("{:<18}", entry.name.clone().unwrap_or_default()),
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
}

fn draw_pace(frame: &mut Frame, app: &App, area: Rect) {
    let is_piscine = app
        .user
        .cursus
        .data()
        .map(|list| {
            list.iter().any(|c| {
                c.slug
                    .as_deref()
                    .map(|s| s.contains("piscine"))
                    .unwrap_or(false)
                    || c.name
                        .as_deref()
                        .map(|n| n.to_lowercase().contains("piscine"))
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    // Piscine students (e.g. mtorfs, cursus 65) have no pace-system
    // profile — the API returns 404 `No row was found…`. Show a friendly
    // placeholder instead of the raw error line. This was observed in
    // caido for mtorfs: GET /api/v1/users/272535/profile → 404, while
    // /api/v1/users/mtorfs/projects/marked?cursus_id=65 and
    // /ongoing?cursus_id=65 succeed with Piscine projects.
    if is_piscine && matches!(app.user.pace, Loadable::Failed(_)) {
        let block = theme::pane(false).title(Span::styled(" pace ", theme::title()));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No pace — Piscine",
                theme::muted(),
            ))),
            inner,
        );
        return;
    }
    let blackhole = if app.user.pace.data().is_some_and(|p| p.is_activated) {
        None
    } else {
        app.user
            .cursus
            .data()
            .and_then(|list| {
                list.iter()
                    .find(|c| c.slug.as_deref() == Some("42cursus"))
                    .or_else(|| list.first())
            })
            .and_then(|c| c.blackholed_at.clone())
    };
    widgets::loadable(
        frame,
        area,
        " pace ",
        &app.user.pace,
        app.tick,
        |frame, area, pace| {
            let mut lines = vec![];
            if pace.is_activated {
                lines.push(widgets::kv(
                    "pace",
                    format!("{} days / level", pace.pace.unwrap_or_default()),
                    theme::text(),
                ));
            }
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
            if let Some(bh) = &blackhole {
                let days = util::days_until(bh).unwrap_or_default();
                lines.push(widgets::kv(
                    "blackhole",
                    format!("{} · {} days", util::fmt_date(bh), days),
                    if days < 0 {
                        theme::error()
                    } else if days < 14 {
                        theme::warn()
                    } else {
                        theme::text()
                    },
                ));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::styled("—", theme::muted())));
            }
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
}

fn draw_logtime(frame: &mut Frame, app: &App, area: Rect) {
    widgets::loadable(
        frame,
        area,
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
}

fn draw_ongoing(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.user.project_focus == UserProjectFocus::Ongoing;
    widgets::loadable(
        frame,
        area,
        &ongoing_title(app, area),
        &app.user.ongoing,
        app.tick,
        |frame, area, ongoing| {
            if ongoing.is_empty() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("—", theme::muted()))),
                    area,
                );
                return;
            }
            // `area` is already `block.inner()` — don't subtract borders again.
            // Use the full inner height so no blank lines remain at the bottom.
            let visible = (area.height as usize).min(ongoing.len()).max(1);
            let sel = app.user.project_sel.min(ongoing.len().saturating_sub(1));
            let start = if !focused || ongoing.len() <= visible {
                0
            } else {
                (sel + 1).saturating_sub(visible)
            };
            let end = (start + visible).min(ongoing.len());
            let lines: Vec<Line> = (start..end)
                .map(|abs_idx| {
                    let proj = &ongoing[abs_idx];
                    let name = proj.project_name.clone().unwrap_or_else(|| "—".into());
                    let selected = focused && abs_idx == sel;
                    let style = if selected {
                        theme::selected()
                    } else {
                        theme::text()
                    };
                    Line::from(vec![
                        Span::styled(if selected { "▸ " } else { "  " }, style),
                        Span::styled(name, style),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
}

fn ongoing_title(app: &App, area: Rect) -> String {
    let Some(ongoing) = app.user.ongoing.data() else {
        return " open projects ".to_string();
    };
    let visible = (area.height.saturating_sub(2) as usize)
        .max(1)
        .min(ongoing.len());
    // `area` here is the outer block area; inner height is area-2. Use it so
    // the title count matches what will actually be visible after `block.inner`.
    if ongoing.len() > visible {
        format!(" open projects ({visible}/{}) ", ongoing.len())
    } else {
        " open projects ".to_string()
    }
}

fn sorted_marked(marked: &[crate::api::MarkedProject]) -> Vec<&crate::api::MarkedProject> {
    let mut v: Vec<&crate::api::MarkedProject> = marked.iter().collect();
    v.sort_by(|a, b| {
        let da = a
            .extra
            .get("last_event_date")
            .and_then(|v| v.as_str())
            .or(a.marked_at.as_deref())
            .unwrap_or("");
        let db = b
            .extra
            .get("last_event_date")
            .and_then(|v| v.as_str())
            .or(b.marked_at.as_deref())
            .unwrap_or("");
        db.cmp(da)
    });
    v
}

fn marked_title(app: &App, area: Rect) -> String {
    let Some(marked) = app.user.marked.data() else {
        return " completed ".to_string();
    };
    let visible = (area.height.saturating_sub(2) as usize)
        .max(1)
        .min(marked.len());
    if marked.len() > visible {
        format!(" completed ({visible}/{}) ", marked.len())
    } else {
        " completed ".to_string()
    }
}

fn draw_marked(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.user.project_focus == UserProjectFocus::Marked;
    widgets::loadable(
        frame,
        area,
        &marked_title(app, area),
        &app.user.marked,
        app.tick,
        |frame, area, marked| {
            if marked.is_empty() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("—", theme::muted()))),
                    area,
                );
                return;
            }
            let sorted = sorted_marked(marked);
            let visible = (area.height as usize).min(sorted.len()).max(1);
            let sel = app.user.project_sel.min(sorted.len().saturating_sub(1));
            let start = if !focused || sorted.len() <= visible {
                0
            } else {
                (sel + 1).saturating_sub(visible)
            };
            let end = (start + visible).min(sorted.len());
            let lines: Vec<Line> = (start..end)
                .map(|abs_idx| {
                    let proj = sorted[abs_idx];
                    let is_sel = focused && abs_idx == sel;
                    let name = proj.display_name().unwrap_or("—");
                    let mark = proj
                        .final_mark
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "—".into());
                    let validated = proj.validated.unwrap_or(false);
                    let style = if is_sel {
                        theme::selected()
                    } else if validated {
                        theme::good()
                    } else {
                        theme::text()
                    };
                    let when = proj
                        .extra
                        .get("last_event_date")
                        .and_then(|v| v.as_str())
                        .or(proj.marked_at.as_deref())
                        .map(util::fmt_date)
                        .unwrap_or_default();
                    let prefix = if is_sel { "▸ " } else { "  " };
                    Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(format!("{:<20} ", &name[..name.len().min(20)]), style),
                        Span::styled(
                            format!("{mark:>3}"),
                            if validated {
                                theme::good()
                            } else {
                                theme::warn()
                            },
                        ),
                        Span::styled(format!("  {when}"), theme::muted()),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        },
    );
}

fn draw_achievements(frame: &mut Frame, app: &App, area: Rect) {
    widgets::loadable(
        frame,
        area,
        " achievements ",
        &app.user.achievements,
        app.tick,
        |frame, area, achievements| {
            let lines: Vec<Line> = achievements
                .iter()
                .take(area.height as usize)
                .map(|ach| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:<10}", ach.tier.clone().unwrap_or_default()),
                            Style::default().fg(theme::MAGENTA),
                        ),
                        Span::styled(ach.name.clone().unwrap_or_default(), theme::text()),
                    ])
                })
                .collect();
            if lines.is_empty() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("—", theme::muted()))),
                    area,
                );
            } else {
                frame.render_widget(Paragraph::new(lines), area);
            }
        },
    );
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

fn popup_display_name(app: &App, slug: &str) -> String {
    if let Some(ongoing) = app.user.ongoing.data() {
        for proj in ongoing {
            if proj.project_slug.as_deref() == Some(slug) {
                return proj.project_name.clone().unwrap_or_else(|| slug.to_owned());
            }
        }
    }
    if let Some(marked) = app.user.marked.data() {
        for proj in sorted_marked(marked) {
            if proj.display_slug() == Some(slug) {
                return proj.display_name().unwrap_or(slug).to_owned();
            }
        }
    }
    slug.to_owned()
}

fn draw_popup(frame: &mut Frame, app: &App, area: Rect) {
    let Some(popup) = &app.user.popup else {
        return;
    };
    let slug = popup.slug.clone();
    let display = popup_display_name(app, &slug);
    let login = app.user.login.clone();
    // Bigger than before (was 70×24) — covers more of the screen while
    // still leaving a visible backdrop. Percentage-based so it scales with
    // the terminal but is clamped and never overflows.
    let w = ((area.width as u16 * 88) / 100)
        .clamp(60, 92)
        .min(area.width.saturating_sub(2));
    let h = ((area.height as u16 * 82) / 100)
        .clamp(20, 34)
        .min(area.height.saturating_sub(2));
    let popup_area = centered(area, w, h);
    frame.render_widget(Clear, popup_area);
    let title = format!(" {display} — {login} (Esc/q close · ↑↓ scroll) ");
    let block = theme::pane(true).title(Span::styled(title, theme::title()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let Some(entry) = app.user.user_mines.get(&slug) else {
        frame.render_widget(
            Paragraph::new(widgets::loading_line("loading…", app.tick)),
            inner,
        );
        return;
    };
    match entry {
        Loadable::Loading => {
            frame.render_widget(
                Paragraph::new(widgets::loading_line("loading project…", app.tick)),
                inner,
            );
        }
        Loadable::Failed(msg) => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(msg.clone(), theme::error()))),
                inner,
            );
        }
        Loadable::Idle => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled("press r to load", theme::muted()))),
                inner,
            );
        }
        Loadable::Ready(mine) => {
            // Build lines mirroring projects.intra.42.fr/{slug}/{login}/ as seen in caido
            // captures for asylla: header, status, team, git repo, evaluations, attachments.
            let mut lines: Vec<Line> = Vec::new();
            // Find marked metadata for final_mark / validated if this is a completed project.
            let marked_meta = app.user.marked.data().and_then(|marked| {
                sorted_marked(marked)
                    .into_iter()
                    .find(|p| p.display_slug() == Some(&slug))
                    .map(|p| {
                        (
                            p.final_mark,
                            p.validated.unwrap_or(false),
                            p.extra
                                .get("last_event_date")
                                .and_then(|v| v.as_str())
                                .or(p.marked_at.as_deref())
                                .map(|v| v.to_owned()),
                        )
                    })
            });

            if let Some((mark, validated, when)) = marked_meta {
                let style = if validated {
                    theme::good()
                } else {
                    theme::warn()
                };
                let mark_s = mark.map(|m| m.to_string()).unwrap_or_else(|| "—".into());
                let when_s = when.as_deref().map(util::fmt_date).unwrap_or_default();
                lines.push(widgets::kv(
                    "final mark",
                    format!("{mark_s} {}", if validated { "✓" } else { "✗" }),
                    style,
                ));
                if !when_s.is_empty() {
                    lines.push(widgets::kv("graded", when_s, theme::muted()));
                }
            }

            if let Some(status) = &mine.status {
                lines.push(widgets::kv(
                    "status",
                    status.clone(),
                    Style::default().fg(theme::state_color(status)),
                ));
            }
            if let Some(team_name) = &mine.team_name {
                lines.push(widgets::kv("team", team_name.clone(), theme::text()));
            }
            if !mine.members.is_empty() {
                lines.push(Line::from(Span::styled("members", theme::title())));
                for member in &mine.members {
                    let style = if member == &login {
                        theme::bright()
                    } else {
                        theme::text()
                    };
                    lines.push(Line::from(Span::styled(format!("  · {member}"), style)));
                }
            } else {
                // Solo project still shows the owner.
                lines.push(widgets::kv("owner", login.clone(), theme::text()));
            }
            if let Some(locked) = &mine.locked_at {
                lines.push(widgets::kv(
                    "locked",
                    util::fmt_datetime(locked),
                    theme::muted(),
                ));
            }
            if let Some(deadline) = &mine.deadline {
                lines.push(widgets::kv(
                    "deadline",
                    util::fmt_datetime(deadline),
                    theme::warn(),
                ));
            }
            if let Some(repo) = &mine.git_repo {
                lines.push(Line::from(Span::styled("git repository", theme::title())));
                // Keep the full vogsphere URL as seen in caido HTML captures
                // e.g. git@vogsphere-v2.42belgium.be:vogsphere/intra-uuid-...-asylla
                // Truncate with `…` so long URLs don't silently disappear off-screen.
                let w = inner.width as usize;
                lines.push(Line::from(Span::styled(
                    util::truncate_str(repo, w),
                    Style::default().fg(theme::ACCENT),
                )));
            } else {
                lines.push(widgets::kv("git repository", "—", theme::muted()));
            }

            if !mine.evaluations.is_empty() {
                lines.push(Line::from(Span::styled("evaluations", theme::title())));
                for (idx, ev) in mine.evaluations.iter().enumerate() {
                    let result = ev.result.as_deref().unwrap_or("—");
                    let flagged = ev.flag_reason.is_some();
                    let style = if flagged || result.contains("fail") {
                        theme::error()
                    } else {
                        theme::good()
                    };
                    let when = ev
                        .evaluated_at
                        .as_deref()
                        .and_then(util::parse_datetime)
                        .map(|at| at.format("%d %b %Y %H:%M").to_string())
                        .unwrap_or_default();
                    // Build the `by … on …` part and truncate so the whole
                    // row fits `inner.width` and ends with `…` instead of
                    // vanishing off-screen on narrow popups.
                    let by_raw = format!(
                        "by {}{}",
                        ev.correctors.join(", "),
                        if when.is_empty() {
                            String::new()
                        } else {
                            format!(" on {when}")
                        }
                    );
                    let w = inner.width as usize;
                    // 4 chars for `  1 ` + 8 for result = 12 prefix
                    let by = util::truncate_str(&by_raw, w.saturating_sub(12));
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {:>2} ", idx + 1), theme::muted()),
                        Span::styled(format!("{result:<8}"), style),
                        Span::styled(by, theme::text()),
                    ]));
                    if let Some(reason) = &ev.flag_reason {
                        let raw = format!("      flagged: {reason}");
                        lines.push(Line::from(Span::styled(
                            util::truncate_str(&raw, inner.width as usize),
                            theme::error(),
                        )));
                    }
                    if let Some(comment) = &ev.comment {
                        // Feedback notes can be very long — truncate with `…`
                        // instead of letting them disappear off the right edge.
                        // Keep the quoting but ensure the line fits `inner.width`.
                        let raw = format!("      “{}”", comment);
                        let w = inner.width as usize;
                        // If the comment still doesn't fit, `truncate_str` will
                        // end it with `…` rather than hiding overflow.
                        lines.push(Line::from(Span::styled(
                            util::truncate_str(&raw, w),
                            theme::muted(),
                        )));
                    }
                }
            } else {
                lines.push(widgets::kv("evaluations", "none yet", theme::muted()));
            }

            if !mine.attachments.is_empty() {
                lines.push(Line::from(Span::styled("attachments", theme::title())));
                let w = inner.width as usize;
                for att in &mine.attachments {
                    lines.push(Line::from(vec![
                        Span::styled("  · ", theme::muted()),
                        Span::styled(util::truncate_str(&att.name, w.saturating_sub(4)), theme::text()),
                    ]));
                    // Show CDN URL as secondary muted line so it mirrors browser's
                    // link target without being interactive.
                    lines.push(Line::from(Span::styled(
                        util::truncate_str(&format!("    {}", att.url), w),
                        theme::muted(),
                    )));
                }
            }

            // Scroll handling: `inner` is already the bordered content area.
            let total = lines.len() as u16;
            let view_h = inner.height;
            popup.view_height.set(view_h);
            popup.total_height.set(total);
            let max_scroll = total.saturating_sub(view_h);
            let start = popup.scroll.min(max_scroll) as usize;
            let end = (start + view_h as usize).min(lines.len());
            let visible = &lines[start..end];

            // Scrollbar hint in title area if overflow.
            let mut display_lines = visible.to_vec();
            if total > view_h {
                display_lines.push(Line::from(Span::styled(
                    format!("  ↕ {}/{}  (↑↓ scroll)", start + 1, total),
                    theme::muted(),
                )));
                // Keep last line visible by trimming one content line if needed
                if display_lines.len() as u16 > view_h {
                    display_lines.truncate(view_h as usize);
                }
            }
            frame.render_widget(Paragraph::new(display_lines), inner);
        }
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    // Popup overlay has priority: Esc/q closes, ↑↓ scroll.
    if let Some(popup) = &app.user.popup {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.user.popup = None;
                return Action::Continue;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let max = popup
                    .total_height
                    .get()
                    .saturating_sub(popup.view_height.get());
                let next = popup.scroll.saturating_sub(1).min(max);
                if let Some(p) = &mut app.user.popup {
                    p.scroll = next;
                }
                return Action::Continue;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = popup
                    .total_height
                    .get()
                    .saturating_sub(popup.view_height.get());
                let next = (popup.scroll + 1).min(max);
                if let Some(p) = &mut app.user.popup {
                    p.scroll = next;
                }
                return Action::Continue;
            }
            KeyCode::PageUp => {
                let step = popup.view_height.get().max(1);
                let next = popup.scroll.saturating_sub(step);
                if let Some(p) = &mut app.user.popup {
                    p.scroll = next;
                }
                return Action::Continue;
            }
            KeyCode::PageDown => {
                let step = popup.view_height.get().max(1);
                let max = popup
                    .total_height
                    .get()
                    .saturating_sub(popup.view_height.get());
                let next = (popup.scroll + step).min(max);
                if let Some(p) = &mut app.user.popup {
                    p.scroll = next;
                }
                return Action::Continue;
            }
            _ => return Action::Continue,
        }
    }

    if key.code == KeyCode::Esc {
        app.enter_tab(crate::app::Tab::Search);
        return Action::Continue;
    }
    match key.code {
        KeyCode::Tab => {
            app.user.project_focus = match app.user.project_focus {
                UserProjectFocus::Ongoing => UserProjectFocus::Marked,
                UserProjectFocus::Marked => UserProjectFocus::Ongoing,
            };
            app.user.project_sel = 0;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.user.project_sel = app.user.project_sel.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = match app.user.project_focus {
                UserProjectFocus::Ongoing => app
                    .user
                    .ongoing
                    .data()
                    .map(|v| v.len())
                    .unwrap_or(0)
                    .saturating_sub(1),
                UserProjectFocus::Marked => app
                    .user
                    .marked
                    .data()
                    .map(|v| v.len())
                    .unwrap_or(0)
                    .saturating_sub(1),
            };
            app.user.project_sel = (app.user.project_sel + 1).min(max);
        }
        KeyCode::Enter => {
            let slug = match app.user.project_focus {
                UserProjectFocus::Ongoing => app
                    .user
                    .ongoing
                    .data()
                    .and_then(|v| v.get(app.user.project_sel))
                    .and_then(|p| p.project_slug.clone()),
                UserProjectFocus::Marked => app.user.marked.data().and_then(|v| {
                    let sorted = sorted_marked(v);
                    sorted
                        .get(app.user.project_sel)
                        .and_then(|p| p.display_slug().map(|s| s.to_owned()))
                }),
            };
            if let Some(slug) = slug {
                let login = app.user.login.clone();
                // Open popup immediately — mirrors browser navigation to
                // https://projects.intra.42.fr/{slug}/{login}/ (caido ids
                // 51627 rag-against-the-machine/asylla and 51523 pac-man/asylla
                // show git repo, team members, locked date etc).
                app.user.popup = Some(crate::state::UserPopup {
                    slug: slug.clone(),
                    scroll: 0,
                    view_height: std::cell::Cell::new(0),
                    total_height: std::cell::Cell::new(0),
                });
                if !app.user.user_mines.contains_key(&slug) {
                    app.user.user_mines.insert(slug.clone(), Loadable::Loading);
                    app.send(crate::bus::Command::LoadUserMine {
                        login,
                        slug,
                        fresh: false,
                    });
                }
            }
        }
        _ => {}
    }
    Action::Continue
}
