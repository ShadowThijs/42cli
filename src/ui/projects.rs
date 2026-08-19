//! Projects browser: active / available / done segments over the holy
//! graph data, with per-project detail (description, rules, team, git
//! repository, downloadable attachments).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use tui_input::backend::crossterm::EventHandler;

use super::theme;
use super::widgets;
use crate::api::models::ProjectDataEntry;
use crate::app::App;
use crate::input::Action;
use crate::state::{Loadable, ProjectSegment};
use crate::util;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let top = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);
    widgets::input_field(
        frame,
        top[0],
        " filter (/ to focus)",
        &app.projects.filter,
        app.projects.filter_focused,
        false,
    );

    let columns = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
        .spacing(1)
        .split(top[1]);
    draw_list(frame, app, columns[0]);
    draw_detail(frame, app, columns[1]);
}

/// Entries currently visible: segment + name filter, sorted by name.
fn visible(app: &App) -> Vec<&ProjectDataEntry> {
    let Some(graph) = app.projects.graph.data() else {
        return Vec::new();
    };
    let segment = app.projects.segment.unwrap_or(ProjectSegment::Active);
    let filter = app.projects.filter.value().to_lowercase();
    let mut entries: Vec<&ProjectDataEntry> = graph
        .iter()
        .filter(|entry| match segment {
            ProjectSegment::Active => {
                matches!(entry.state.as_deref(), Some("in_progress" | "subscribed"))
            }
            ProjectSegment::Available => matches!(entry.state.as_deref(), Some("available")),
            ProjectSegment::Done => matches!(entry.state.as_deref(), Some("done")),
            ProjectSegment::All => true,
        })
        .filter(|entry| {
            filter.is_empty()
                || entry
                    .name
                    .as_deref()
                    .is_some_and(|name| name.to_lowercase().contains(&filter))
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let segment = app.projects.segment.unwrap_or(ProjectSegment::Active);
    let tabs: Vec<Span> = ProjectSegment::ALL
        .iter()
        .flat_map(|candidate| {
            let style = if *candidate == segment {
                Style::default()
                    .fg(ratatui::style::Color::Black)
                    .bg(theme::ACCENT)
            } else {
                theme::muted()
            };
            vec![
                Span::styled(format!(" {} ", candidate.label()), style),
                Span::raw(" "),
            ]
        })
        .collect();
    let block = widgets::titled_block(" projects ", !app.projects.focus_details)
        .title(Span::styled("  ", theme::muted()))
        .title(Line::from(tabs).right_aligned());

    let entries = visible(app);
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let state = entry.state.as_deref().unwrap_or("—");
            let mark = entry
                .final_mark
                .map(|mark| format!("{mark:>3}"))
                .unwrap_or_else(|| "   ".into());
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<22}", truncate(entry.name.as_deref().unwrap_or("—"), 22)),
                    theme::text(),
                ),
                Span::styled(mark, Style::default().fg(theme::state_color(state))),
                Span::styled(format!(" {state}"), theme::muted()),
            ]))
        })
        .collect();
    let mut list_state = ListState::default();
    list_state.select(Some(
        app.projects.selection.min(entries.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme::selected()),
        area,
        &mut list_state,
    );
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        format!("{value:<width$}")
    } else {
        let cut: String = value.chars().take(width - 1).collect();
        format!("{cut}…")
    }
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let block = widgets::titled_block(" details ", app.projects.focus_details);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let entries = visible(app);
    let Some(entry) = entries.get(app.projects.selection) else {
        widgets::hint(frame, inner, "select a project");
        return;
    };
    let slug = entry.slug.clone().unwrap_or_default();
    let mine = app.projects.mine.get(&slug);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            entry.name.clone().unwrap_or_default(),
            theme::bright(),
        )),
        widgets::kv(
            "status",
            entry.state.clone().unwrap_or_else(|| "—".into()),
            Style::default().fg(theme::state_color(entry.state.as_deref().unwrap_or(""))),
        ),
    ];
    if let Some(mark) = entry.final_mark {
        lines.push(widgets::kv("final mark", mark.to_string(), theme::bright()));
    }
    lines.push(widgets::kv(
        "duration",
        entry.duration.clone().unwrap_or_else(|| "—".into()),
        theme::text(),
    ));
    if let Some(difficulty) = entry.difficulty {
        lines.push(widgets::kv(
            "difficulty",
            difficulty.to_string(),
            theme::text(),
        ));
    }
    if let Some(rules) = &entry.rules {
        lines.push(Line::from(Span::styled("rules", theme::title())));
        lines.extend(
            util::wrap_lines(rules, inner.width as usize, RULES_LINES)
                .into_iter()
                .map(|line| line.style(theme::muted())),
        );
    }
    if let Some(description) = &entry.description {
        lines.push(Line::from(""));
        lines.extend(
            util::wrap_lines(description, inner.width as usize, DESCRIPTION_LINES)
                .into_iter()
                .map(|line| line.style(theme::muted())),
        );
    }

    // Team + attachments from the scraped `/{slug}/mine` page.
    match mine {
        Some(Loadable::Ready(mine)) => {
            if !mine.members.is_empty() {
                lines.push(Line::from(Span::styled("team", theme::title())));
                for member in &mine.members {
                    lines.push(Line::from(Span::styled(
                        format!("  · {member}"),
                        theme::text(),
                    )));
                }
            }
            if !mine.evaluations.is_empty() {
                lines.push(Line::from(Span::styled("evaluations", theme::title())));
                for (index, evaluation) in mine.evaluations.iter().enumerate() {
                    let result = evaluation.result.as_deref().unwrap_or("—");
                    let style = if result.contains("fail") {
                        theme::error()
                    } else {
                        theme::good()
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {:>2} ", index + 1), theme::muted()),
                        Span::styled(format!("{result:<8}"), style),
                        Span::styled(
                            format!("by {}", evaluation.correctors.join(", ")),
                            theme::text(),
                        ),
                    ]));
                    if let Some(comment) = &evaluation.comment {
                        lines.push(Line::from(Span::styled(
                            format!("      “{}”", truncate(comment, 64)),
                            theme::muted(),
                        )));
                    }
                }
            }
            if let Some(repo) = &mine.git_repo {
                lines.push(Line::from(vec![
                    Span::styled("git  ", theme::muted()),
                    Span::styled(repo.clone(), theme::text()),
                ]));
            }
            if let Some(locked) = &mine.locked_at {
                lines.push(widgets::kv(
                    "locked",
                    util::fmt_datetime(locked),
                    theme::text(),
                ));
            }
            if !mine.attachments.is_empty() {
                lines.push(Line::from(Span::styled(
                    "documents (d = download)",
                    theme::title(),
                )));
                for (index, attachment) in mine.attachments.iter().enumerate() {
                    let focused =
                        app.projects.focus_details && index == app.projects.attachment_sel;
                    let style = if focused {
                        theme::selected()
                    } else {
                        theme::text()
                    };
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  {} {name}",
                            if focused { "▸" } else { " " },
                            name = attachment.name
                        ),
                        style,
                    )));
                }
            }
        }
        Some(Loadable::Loading) => lines.push(widgets::loading_line("loading project…", app.tick)),
        Some(Loadable::Failed(message)) => {
            lines.push(Line::from(Span::styled(message.clone(), theme::error())))
        }
        _ => {}
    }

    let downloading = app.projects.downloading.keys().cloned().collect::<Vec<_>>();
    if !downloading.is_empty() {
        lines.push(widgets::loading_line(
            &format!("downloading {}…", downloading.join(", ")),
            app.tick,
        ));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Wrapped-line budget so team + attachments stay visible.
const RULES_LINES: usize = 3;
const DESCRIPTION_LINES: usize = 6;

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    // Filter capture mode.
    if app.projects.filter_focused {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.projects.filter_focused = false,
            _ => {
                let _ = app
                    .projects
                    .filter
                    .handle_event(&crossterm::event::Event::Key(key));
            }
        }
        app.projects.selection = 0;
        return Action::Continue;
    }

    // Tab toggles between the list pane and the details pane.
    if key.code == KeyCode::Tab {
        app.projects.focus_details = !app.projects.focus_details;
        app.projects.attachment_sel = 0;
        return Action::Continue;
    }

    let entries = visible(app);
    let max = entries.len().saturating_sub(1);

    if app.projects.focus_details {
        match key.code {
            KeyCode::Esc | KeyCode::Left | KeyCode::Backspace => {
                app.projects.focus_details = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.projects.attachment_sel = app.projects.attachment_sel.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.projects.attachment_sel += 1;
            }
            KeyCode::Char('d') | KeyCode::Enter => {
                if let Some(entry) = entries.get(app.projects.selection)
                    && let Some(slug) = entry.slug.clone()
                    && let Some(Loadable::Ready(mine)) = app.projects.mine.get(&slug)
                    && let Some(attachment) = mine.attachments.get(app.projects.attachment_sel)
                {
                    let name = attachment.name.clone();
                    app.projects.downloading.insert(name.clone(), true);
                    app.send(crate::bus::Command::DownloadAttachment {
                        name,
                        url: attachment.url.clone(),
                    });
                }
            }
            _ => {}
        }
        clamp_attachment_sel(app);
        return Action::Continue;
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.projects.selection = app.projects.selection.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.projects.selection = (app.projects.selection + 1).min(max);
        }
        KeyCode::Left => app.projects.segment = Some(prev_segment(app)),
        KeyCode::Right => app.projects.segment = Some(next_segment(app)),
        KeyCode::Char('/') => app.projects.filter_focused = true,
        KeyCode::Enter => {
            app.projects.focus_details = true;
            app.projects.attachment_sel = 0;
        }
        _ => {}
    }
    lazy_load_mine(app);
    clamp_attachment_sel(app);
    Action::Continue
}

/// Keep the selection inside the (possibly shrunken) list of the segment.
fn clamp_selection(app: &mut App) {
    let max = visible(app).len().saturating_sub(1);
    app.projects.selection = app.projects.selection.min(max);
}

fn prev_segment(app: &App) -> ProjectSegment {
    let current = app.projects.segment.unwrap_or(ProjectSegment::Active);
    let index = ProjectSegment::ALL
        .iter()
        .position(|s| *s == current)
        .unwrap_or(0);
    ProjectSegment::ALL[(index + ProjectSegment::ALL.len() - 1) % ProjectSegment::ALL.len()]
}

fn next_segment(app: &App) -> ProjectSegment {
    let current = app.projects.segment.unwrap_or(ProjectSegment::Active);
    let index = ProjectSegment::ALL
        .iter()
        .position(|s| *s == current)
        .unwrap_or(0);
    ProjectSegment::ALL[(index + 1) % ProjectSegment::ALL.len()]
}

/// Fetch the `/{slug}/mine` page for the selected active project once.
fn lazy_load_mine(app: &mut App) {
    let entries = visible(app);
    let Some(entry) = entries.get(app.projects.selection) else {
        return;
    };
    let active = matches!(entry.state.as_deref(), Some("in_progress" | "subscribed"));
    let Some(slug) = entry.slug.clone().filter(|_| active) else {
        return;
    };
    if !app.projects.mine.contains_key(&slug) {
        app.projects.mine.insert(slug.clone(), Loadable::Loading);
        app.send(crate::bus::Command::LoadMine { slug, fresh: false });
    }
}

/// Select a project by slug (notification jump). Falls back to the `all`
/// segment when the project is not part of the current one.
pub fn focus_project(app: &mut App, slug: &str) {
    if app.projects.graph.data().is_none() {
        // Graph not loaded yet (first Projects entry): retry once it lands.
        app.projects.pending_focus = Some(slug.to_owned());
        app.set_status("loading projects…");
        return;
    }
    if !visible(app)
        .iter()
        .any(|entry| entry.slug.as_deref() == Some(slug))
    {
        app.projects.segment = Some(ProjectSegment::All);
    }
    let position = visible(app)
        .iter()
        .position(|entry| entry.slug.as_deref() == Some(slug));
    match position {
        Some(index) => {
            app.projects.selection = index;
            app.projects.attachment_sel = 0;
            app.projects.focus_details = false;
            app.set_status(format!("opened {slug}"));
            lazy_load_mine(app);
        }
        None => app.set_status(format!("project {slug} not in your graph")),
    }
}

fn clamp_attachment_sel(app: &mut App) {
    let slug = visible(app)
        .get(app.projects.selection)
        .and_then(|entry| entry.slug.clone());
    let count = slug
        .and_then(|slug| app.projects.mine.get(&slug))
        .and_then(|slot| slot.data())
        .map(|mine| mine.attachments.len())
        .unwrap_or(0);
    app.projects.attachment_sel = app.projects.attachment_sel.min(count.saturating_sub(1));
}
