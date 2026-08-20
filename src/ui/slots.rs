//! Slots: a week calendar for both modes, mirroring slots.42belgium.be —
//! project booking (pick a project, book/cancel on the grid) and my open
//! hours (drag out a range to open, `d` to close).

use chrono::{DateTime, Duration, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

use super::theme;
use super::week_grid::{self, Cell, CellKind};
use super::widgets;
use crate::api::models::Slot;
use crate::api::slots::{campus_label, parse_slot_time, slot_label};
use crate::app::App;
use crate::bus::Command;
use crate::input::Action;
use crate::state::{SlotsFocus, SlotsMode};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let mode = app.slots.mode.unwrap_or(SlotsMode::Overview);
    let campus = if app.slots.campus_bx { "bx" } else { "anr" };
    let header = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Line::from(vec![
            mode_span(" [p] project booking ", mode == SlotsMode::Overview),
            mode_span(" [o] my hours ", mode == SlotsMode::Hours),
            Span::styled("   ", theme::muted()),
            Span::styled(
                if app.slots.remote {
                    "✈ inter-campus "
                } else {
                    "  campus only  "
                },
                if app.slots.remote {
                    theme::good()
                } else {
                    theme::muted()
                },
            ),
            Span::styled(
                campus_label(campus).to_owned(),
                Style::default().fg(theme::campus_color(campus)),
            ),
            Span::styled(
                "   campus=bookable · green=yours · red=booked · grey=too soon",
                theme::muted(),
            ),
        ]),
        header[0],
    );
    match mode {
        SlotsMode::Overview => draw_booking(frame, app, header[1]),
        SlotsMode::Hours => draw_hours(frame, app, header[1]),
    }
}

fn mode_span(label: &str, active: bool) -> Span<'static> {
    if active {
        Span::styled(
            label.to_owned(),
            Style::default()
                .fg(ratatui::style::Color::Black)
                .bg(theme::ACCENT),
        )
    } else {
        Span::styled(label.to_owned(), theme::muted())
    }
}

// ----------------------------------------------------------- booking ----

fn draw_booking(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(0)])
        .spacing(1)
        .split(area);

    widgets::loadable(
        frame,
        rows[0],
        " bookable projects ",
        &app.slots.projects,
        app.tick,
        |frame, area, projects| {
            let spans: Vec<Span> = projects
                .iter()
                .enumerate()
                .flat_map(|(index, project)| {
                    let selected = index == app.slots.project_sel;
                    let style = if selected && app.slots.focus == SlotsFocus::Strip {
                        Style::default()
                            .fg(ratatui::style::Color::Black)
                            .bg(theme::ACCENT)
                    } else if selected {
                        Style::default().fg(theme::ACCENT)
                    } else {
                        theme::text()
                    };
                    vec![
                        Span::styled(
                            format!(" {} ", project.name.clone().unwrap_or_default()),
                            style,
                        ),
                        Span::raw("│"),
                    ]
                })
                .collect();
            frame.render_widget(Paragraph::new(Line::from(spans)), area);
        },
    );

    let columns = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        .spacing(1)
        .split(rows[1]);

    // Available slots + own reservations laid onto the week grid.
    let empty = Vec::new();
    let slots = app.slots.project_slots.data().unwrap_or(&empty);
    let cells = week_grid::build_cells(slots, app.slots.week_anchor, false);
    let tooltip = cursor_tooltip(app, &cells, false);
    week_grid::draw(
        frame,
        columns[0],
        &app.slots,
        &cells,
        &week_title(app, " project slots "),
        &tooltip,
    );

    widgets::loadable(
        frame,
        columns[1],
        " my reservations ",
        &app.slots.reserved,
        app.tick,
        |frame, area, slots| {
            if slots.is_empty() {
                widgets::hint(frame, area, "none");
                return;
            }
            frame.render_widget(List::new(slots.iter().map(slot_item)), area);
        },
    );
}

// ------------------------------------------------------------- hours ----

fn draw_hours(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .spacing(1)
    .split(area);

    // My open hours (░) with student bookings (✕) on top.
    let mut merged: Vec<Slot> = Vec::new();
    if let Some(open) = app.slots.open.data() {
        merged.extend(open.iter().cloned());
    }
    if let Some(reserved) = app.slots.reserved.data() {
        merged.extend(reserved.iter().cloned());
    }
    let cells = week_grid::build_cells(&merged, app.slots.week_anchor, true);
    let tooltip = cursor_tooltip(app, &cells, true);
    week_grid::draw(
        frame,
        rows[0],
        &app.slots,
        &cells,
        &week_title(app, " my week "),
        &tooltip,
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("██", Style::default().fg(theme::campus_color("bx"))),
            Span::styled("/", theme::muted()),
            Span::styled("██", Style::default().fg(theme::campus_color("anr"))),
            Span::styled(" open · ", theme::muted()),
            Span::styled("██", Style::default().fg(theme::GOOD)),
            Span::styled(" yours · ", theme::muted()),
            Span::styled("██", Style::default().fg(theme::ERR)),
            Span::styled(" booked by a student · ", theme::muted()),
            Span::styled("██", Style::default().fg(theme::DIM)),
            Span::styled(" too soon", theme::muted()),
        ])),
        rows[1],
    );
    if app.slots.open.is_loading() || app.slots.reserved.is_loading() {
        frame.render_widget(widgets::loading_line("loading hours…", app.tick), rows[2]);
    }
}

// ------------------------------------------------------------ shared ----

fn week_title(app: &App, prefix: &str) -> String {
    let first = app.slots.week_anchor;
    let last = first + Duration::days(6);
    format!(
        "{prefix}{} – {} ",
        first.format("%d %b"),
        last.format("%d %b")
    )
}

/// The popup describing the cursor cell — anchored right next to it by
/// `week_grid::draw`, so the info sits where you are looking.
fn cursor_tooltip(app: &App, cells: &[Vec<Cell>], hours_mode: bool) -> Vec<Line<'static>> {
    let Some(at) = week_grid::cell_time(
        app.slots.week_anchor,
        app.slots.cursor.0,
        app.slots.cursor.1,
    ) else {
        return Vec::new();
    };
    let cell = cells
        .get(app.slots.cursor.0)
        .and_then(|column| column.get(app.slots.cursor.1));
    let (mark, detail) = match cell.map(|cell| cell.kind) {
        Some(CellKind::Mine) => (
            "★",
            if hours_mode {
                "your reservation"
            } else {
                "your reservation — Enter cancels"
            },
        ),
        Some(CellKind::Booked) => ("✕", "booked by another student"),
        Some(CellKind::Open) => (
            "░",
            if hours_mode {
                "your open hour"
            } else {
                "available — Enter books"
            },
        ),
        _ => (
            "·",
            if hours_mode {
                "free — Enter opens a range"
            } else {
                "no slot here"
            },
        ),
    };
    let campus = cell
        .map(|cell| {
            let name = campus_label(&cell.campus).to_owned();
            if cell.remote {
                format!("{name}, inter-campus")
            } else {
                name
            }
        })
        .unwrap_or_default();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{mark} "), theme::bright()),
            Span::styled(at.format("%a %d %b %H:%M").to_string(), theme::bright()),
        ]),
        Line::from(Span::styled(detail.to_owned(), theme::text())),
    ];
    if let Some(cell) = cell.filter(|cell| cell.kind != CellKind::Empty) {
        lines.push(Line::from(Span::styled(
            format!("{} · {campus}", cell.label),
            theme::muted(),
        )));
    }
    if !hours_mode && cell.is_some_and(|cell| cell.past && cell.kind == CellKind::Open) {
        lines.push(Line::from(Span::styled(
            "past the bookable window (30 min ahead)",
            theme::muted(),
        )));
    }
    lines
}

fn slot_item(slot: &Slot) -> ListItem<'static> {
    let campus = slot.campus.as_deref().unwrap_or("—");
    let label = if slot.remote {
        format!("{} ✈", campus_label(campus))
    } else {
        campus_label(campus).to_owned()
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{:<22}", slot_label(slot)), theme::text()),
        Span::styled(label, Style::default().fg(theme::campus_color(campus))),
    ]))
}

// ------------------------------------------------------------- input ----

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    let mode = app.slots.mode.unwrap_or(SlotsMode::Overview);
    match key.code {
        KeyCode::Char('o') => return enter(app, SlotsMode::Hours),
        KeyCode::Char('p') | KeyCode::Char('b') => return enter(app, SlotsMode::Overview),
        KeyCode::Char('s') => {
            app.send(Command::SyncSlotsProjects);
            return Action::Continue;
        }
        KeyCode::Char('<') | KeyCode::PageUp => return shift_week(app, -7),
        KeyCode::Char('>') | KeyCode::PageDown => return shift_week(app, 7),
        _ => {}
    }

    if mode == SlotsMode::Overview && app.slots.focus == SlotsFocus::Strip {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => move_project(app, -1),
            KeyCode::Right | KeyCode::Char('l') => move_project(app, 1),
            KeyCode::Down
            | KeyCode::Tab
            | KeyCode::Enter
            | KeyCode::Char('f')
            | KeyCode::Char('j')
            | KeyCode::Char('k') => {
                app.slots.focus = SlotsFocus::Grid;
            }
            _ => {}
        }
        return Action::Continue;
    }

    match key.code {
        // Vim-style half-page jumps — 15-minute rows make single steps
        // far too slow to cross a day. (Plain d/u stay mode keys; these
        // arms sit above them so the modifier wins.)
        KeyCode::Char('d') | KeyCode::Char('f')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            let half = app.slots.grid_view.get().max(1) / 2;
            move_cursor(app, 0, half as i32);
        }
        KeyCode::Char('u') | KeyCode::Char('b')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            let half = app.slots.grid_view.get().max(1) / 2;
            move_cursor(app, 0, -(half as i32));
        }
        KeyCode::Up | KeyCode::Char('k') => move_cursor(app, 0, -1),
        KeyCode::Down | KeyCode::Char('j') => move_cursor(app, 0, 1),
        KeyCode::Left | KeyCode::Char('h') => move_cursor(app, -1, 0),
        KeyCode::Right | KeyCode::Char('l') => move_cursor(app, 1, 0),
        KeyCode::Tab if mode == SlotsMode::Overview => app.slots.focus = SlotsFocus::Strip,
        KeyCode::Esc => app.slots.range_start = None,
        KeyCode::Enter => grid_enter(app, mode),
        KeyCode::Char('d') if mode == SlotsMode::Hours => delete_under_cursor(app),
        // Campus / inter-campus matter in both modes: booking filters its
        // feeds by them, opening hours use them for new ranges.
        KeyCode::Char('c') => {
            app.slots.campus_bx = !app.slots.campus_bx;
            if mode == SlotsMode::Overview {
                app.reload_project_slots();
            }
        }
        KeyCode::Char('t') => {
            app.slots.remote = !app.slots.remote;
            if mode == SlotsMode::Overview {
                app.reload_project_slots();
            }
        }
        _ => {}
    }
    Action::Continue
}

fn enter(app: &mut App, mode: SlotsMode) -> Action {
    if app.slots.mode != Some(mode) {
        app.slots.mode = Some(mode);
        app.slots.range_start = None;
        app.slots.focus = if mode == SlotsMode::Overview {
            SlotsFocus::Strip
        } else {
            SlotsFocus::Grid
        };
        app.slots_reload();
    }
    Action::Continue
}

fn shift_week(app: &mut App, days: i64) -> Action {
    app.slots.week_anchor += Duration::try_days(days).unwrap_or_default();
    app.slots.range_start = None;
    app.slots_reload();
    Action::Continue
}

fn move_cursor(app: &mut App, day: i32, row: i32) {
    let (mut cursor_day, mut cursor_row) = app.slots.cursor;
    cursor_day = (cursor_day as i32 + day).clamp(0, 6) as usize;
    cursor_row = (cursor_row as i32 + row).clamp(0, week_grid::ROWS as i32 - 1) as usize;
    // Leaving the dragged-out range's day cancels it, like a broken drag.
    if app
        .slots
        .range_start
        .is_some_and(|(day, _)| day != cursor_day)
    {
        app.slots.range_start = None;
    }
    app.slots.cursor = (cursor_day, cursor_row);
}

fn move_project(app: &mut App, delta: i32) {
    let count = app
        .slots
        .projects
        .data()
        .map_or(0, |projects| projects.len());
    if count == 0 {
        return;
    }
    app.slots.project_sel =
        ((app.slots.project_sel as i32 + delta).rem_euclid(count as i32)) as usize;
    app.reload_project_slots();
}

/// Enter on the grid: book / cancel in booking mode, open a range or
/// confirm it in hours mode.
fn grid_enter(app: &mut App, mode: SlotsMode) {
    match mode {
        SlotsMode::Overview => book_under_cursor(app),
        SlotsMode::Hours => hours_enter(app),
    }
}

fn book_under_cursor(app: &mut App) {
    let Some(ps_id) = app.slots.selected_project().and_then(|project| project.id) else {
        return;
    };
    let Some(at) = week_grid::cell_time(
        app.slots.week_anchor,
        app.slots.cursor.0,
        app.slots.cursor.1,
    ) else {
        return;
    };
    let Some(slot) = slot_under(app.slots.project_slots.data(), at) else {
        app.set_status("no slot under the cursor");
        return;
    };
    let campus = if slot.feed.is_empty() {
        slot.campus.clone().unwrap_or_default()
    } else {
        slot.feed.clone()
    };
    if slot.reserved {
        // No working cancel route exists: DELETE on api/project_slots is
        // 405 and reserved-* campuses are 400 (see probe_booking_rules).
        app.set_status(
            "your reservation — the slots API exposes no cancel; use the website to cancel",
        );
        return;
    }
    // Bookings go at the cursor cell's own 15-minute time — the site offers
    // every quarter hour inside an open block, not just block starts.
    if at < week_grid::bookable_from() {
        app.set_status(format!(
            "bookable from {} — {} minutes ahead only",
            week_grid::bookable_from().format("%H:%M"),
            week_grid::BOOKING_LEAD_MINUTES
        ));
        return;
    }
    let time = at
        .with_timezone(&chrono::Utc)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    app.send(Command::BookSlot {
        ps_id,
        time,
        campus,
    });
    app.set_status("booking slot…");
}

fn hours_enter(app: &mut App) {
    let Some(at) = week_grid::cell_time(
        app.slots.week_anchor,
        app.slots.cursor.0,
        app.slots.cursor.1,
    ) else {
        return;
    };
    let bookable_from = week_grid::bookable_from();
    if app.slots.range_start.is_none() {
        if at < bookable_from {
            app.set_status(format!(
                "hours open from {} onward ({} min ahead, today only)",
                bookable_from.format("%a %H:%M"),
                week_grid::BOOKING_LEAD_MINUTES
            ));
            return;
        }
        // Starting a range: only from an empty (or own) cell.
        if let Some(_slot) = slot_under(app.slots.open.data(), at) {
            app.set_status("this hour is already open — d closes it");
        } else if slot_under(app.slots.reserved.data(), at).is_some() {
            app.set_status("booked by a student — cannot overlap");
        } else {
            app.slots.range_start = Some(app.slots.cursor);
            app.set_status("move to the end and press Enter");
        }
        return;
    }
    // Confirming: same-day range from the start cell to the cursor cell.
    let (from_day, from_row) = app.slots.range_start.take().unwrap_or(app.slots.cursor);
    if from_day != app.slots.cursor.0 {
        app.set_status("a range must stay within one day");
        return;
    }
    let (lo, hi) = if from_row <= app.slots.cursor.1 {
        (from_row, app.slots.cursor.1)
    } else {
        (app.slots.cursor.1, from_row)
    };
    let Some(begin) = week_grid::cell_time(app.slots.week_anchor, from_day, lo) else {
        return;
    };
    // The last selected cell spans 15 minutes, so the range ends a quarter
    // hour after its row begins (one row past the last is a valid time).
    let Some(end) = week_grid::cell_time(app.slots.week_anchor, from_day, hi + 1) else {
        return;
    };
    if begin < bookable_from {
        app.set_status(format!(
            "hours open from {} onward ({} min ahead)",
            bookable_from.format("%a %H:%M"),
            week_grid::BOOKING_LEAD_MINUTES
        ));
        return;
    }
    app.send(Command::CreateSlot {
        begin,
        end,
        campus: if app.slots.campus_bx {
            "bx".into()
        } else {
            "anr".into()
        },
        remote: app.slots.remote,
    });
    app.set_status("opening hours…");
}

fn delete_under_cursor(app: &mut App) {
    let Some(at) = week_grid::cell_time(
        app.slots.week_anchor,
        app.slots.cursor.0,
        app.slots.cursor.1,
    ) else {
        return;
    };
    if slot_under(app.slots.reserved.data(), at).is_some() {
        app.set_status("booked by a student — cannot close");
        return;
    }
    let Some(slot) = slot_under(app.slots.open.data(), at) else {
        app.set_status("no open hour under the cursor");
        return;
    };
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| dt.with_timezone(&Local))
    };
    let (Some(start), Some(end)) = (
        slot.start.as_deref().and_then(parse),
        slot.end.as_deref().and_then(parse),
    ) else {
        app.set_status("cannot parse slot time");
        return;
    };
    app.send(Command::DeleteSlot { start, end });
    app.set_status("closing hour…");
}

/// The slot covering `at`, own reservations winning over bookable ones.
fn slot_under(slots: Option<&Vec<Slot>>, at: DateTime<Local>) -> Option<&Slot> {
    slots?
        .iter()
        .filter(|slot| {
            let Some(start) = slot.start.as_deref().and_then(parse_slot_time) else {
                return false;
            };
            let Some(end) = slot.end.as_deref().and_then(parse_slot_time) else {
                return false;
            };
            start <= at && at < end
        })
        .max_by_key(|slot| slot.reserved)
}
