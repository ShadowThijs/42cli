//! Slots: availability hours (open / close) at both campuses and project
//! slot booking through slots.42belgium.be.

use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::slot_form;
use super::theme;
use super::widgets;
use crate::api::models::Slot;
use crate::api::slots::{campus_label, slot_label};
use crate::app::App;
use crate::bus::Command;
use crate::input::Action;
use crate::state::{Loadable, SlotsFocus, SlotsMode};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let mode = app.slots.mode.unwrap_or(SlotsMode::Overview);
    let header = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Line::from(vec![
            mode_span(" [o] project booking ", mode == SlotsMode::Overview),
            mode_span(" [h] open hours ", mode == SlotsMode::Hours),
            Span::styled("   s=sync  r=reload", theme::muted()),
        ]),
        header[0],
    );
    match mode {
        SlotsMode::Overview => draw_overview(frame, app, header[1]),
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

// ------------------------------------------------------------- hours ----

fn draw_hours(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        .spacing(1)
        .split(area);

    widgets::loadable(
        frame,
        columns[0],
        " my open hours (d=close) ",
        &app.slots.open,
        app.tick,
        |frame, area, slots| {
            let items: Vec<ListItem> = slots.iter().map(slot_item).collect();
            if items.is_empty() {
                widgets::hint(frame, area, "nothing open — use the form");
                return;
            }
            let mut state = ListState::default();
            state.select(Some(app.slots.open_sel.min(slots.len().saturating_sub(1))));
            frame.render_stateful_widget(
                List::new(items).highlight_style(theme::selected()),
                area,
                &mut state,
            );
        },
    );

    slot_form::draw_form(frame, app, columns[1]);
}

// ---------------------------------------------------------- overview ----

fn draw_overview(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);

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
                    let style = if index == app.slots.project_sel {
                        Style::default()
                            .fg(ratatui::style::Color::Black)
                            .bg(theme::ACCENT)
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

    widgets::loadable(
        frame,
        columns[0],
        " slots (Enter=book / cancel) ",
        &app.slots.project_slots,
        app.tick,
        |frame, area, slots| {
            // Available slots first, then the user's own reservations (★),
            // kept in the same order as `slots` so the selection index lines
            // up for booking / cancelling.
            let mut items: Vec<ListItem> = slots
                .iter()
                .filter(|slot| !slot.reserved)
                .map(slot_item)
                .collect();
            items.extend(slots.iter().filter(|slot| slot.reserved).map(|slot| {
                let mut line = slot_line(slot);
                line.spans
                    .insert(0, Span::styled("★ ", Style::default().fg(theme::WARN)));
                ListItem::new(line)
            }));
            if items.is_empty() {
                widgets::hint(frame, area, "no slots — pick a project above");
                return;
            }
            let mut state = ListState::default();
            state.select(Some(app.slots.slot_sel.min(items.len().saturating_sub(1))));
            frame.render_stateful_widget(
                List::new(items).highlight_style(theme::selected()),
                area,
                &mut state,
            );
        },
    );

    widgets::loadable(
        frame,
        columns[1],
        " all my reservations ",
        &app.slots.reserved,
        app.tick,
        |frame, area, slots| {
            let items: Vec<ListItem> = slots.iter().map(slot_item).collect();
            if items.is_empty() {
                widgets::hint(frame, area, "none");
                return;
            }
            frame.render_widget(List::new(items), area);
        },
    );
}

fn slot_item(slot: &Slot) -> ListItem<'static> {
    ListItem::new(slot_line(slot))
}

fn slot_line(slot: &Slot) -> Line<'static> {
    let campus = slot.campus.as_deref().unwrap_or("—");
    let label = if slot.remote {
        format!("{} ✈", campus_label(campus))
    } else {
        campus_label(campus).to_owned()
    };
    Line::from(vec![
        Span::styled(format!("{:<22}", slot_label(slot)), theme::text()),
        Span::styled(label, Style::default().fg(theme::campus_color(campus))),
    ])
}

// ------------------------------------------------------------- input ----

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    if app.slots.focus == SlotsFocus::Form {
        return slot_form::handle_form_key(app, key);
    }
    let mode = app.slots.mode.unwrap_or(SlotsMode::Overview);
    match (mode, key.code) {
        (_, KeyCode::Char('h')) => enter(app, SlotsMode::Hours),
        (_, KeyCode::Char('o')) | (_, KeyCode::Char('b')) => enter(app, SlotsMode::Overview),
        (_, KeyCode::Char('s')) => app.send(Command::SyncSlotsProjects),

        (SlotsMode::Hours, KeyCode::Up | KeyCode::Char('k')) => {
            app.slots.open_sel = app.slots.open_sel.saturating_sub(1)
        }
        (SlotsMode::Hours, KeyCode::Down | KeyCode::Char('j')) => app.slots.open_sel += 1,
        (SlotsMode::Hours, KeyCode::Char('f') | KeyCode::Enter) => {
            app.slots.focus = SlotsFocus::Form
        }
        (SlotsMode::Hours, KeyCode::Delete | KeyCode::Char('d')) => delete_open(app),

        (SlotsMode::Overview, KeyCode::Up | KeyCode::Char('k')) => move_slot(app, -1),
        (SlotsMode::Overview, KeyCode::Down | KeyCode::Char('j')) => move_slot(app, 1),
        (SlotsMode::Overview, KeyCode::Left) => move_project(app, -1),
        (SlotsMode::Overview, KeyCode::Right) => move_project(app, 1),
        (SlotsMode::Overview, KeyCode::Enter) => act_on_slot(app),
        _ => {}
    }
    Action::Continue
}

fn enter(app: &mut App, mode: SlotsMode) {
    if app.slots.mode != Some(mode) {
        app.slots.mode = Some(mode);
        if mode == SlotsMode::Hours && app.slots.form.date.value().is_empty() {
            app.slots.form.date =
                tui_input::Input::new(Local::now().format("%Y-%m-%d").to_string());
        }
        app.slots_reload();
    }
}

fn move_slot(app: &mut App, delta: i32) {
    let count = app
        .slots
        .project_slots
        .data()
        .map_or(0, |slots| slots.len());
    if count == 0 {
        return;
    }
    app.slots.slot_sel = ((app.slots.slot_sel as i32 + delta).rem_euclid(count as i32)) as usize;
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
    app.slots.slot_sel = 0;
    if let Some(ps_id) = app.slots.selected_project().and_then(|project| project.id) {
        app.slots.project_slots = Loadable::Loading;
        app.send(Command::LoadProjectSlots { ps_id });
    }
}

fn act_on_slot(app: &mut App) {
    let Some(ps_id) = app.slots.selected_project().and_then(|project| project.id) else {
        return;
    };
    // Same order as drawn: available slots first, reservations after.
    let ordered: Vec<Slot> = app
        .slots
        .project_slots
        .data()
        .map(|slots| {
            slots
                .iter()
                .filter(|slot| !slot.reserved)
                .chain(slots.iter().filter(|slot| slot.reserved))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let Some(slot) = ordered.get(app.slots.slot_sel) else {
        return;
    };
    let time = slot.start.clone().unwrap_or_default();
    let campus = if slot.feed.is_empty() {
        slot.campus.clone().unwrap_or_default()
    } else {
        slot.feed.clone()
    };
    if slot.reserved {
        app.send(Command::CancelSlot {
            ps_id,
            time,
            campus,
        });
        app.set_status("cancelling reservation…");
    } else {
        app.send(Command::BookSlot {
            ps_id,
            time,
            campus,
        });
        app.set_status("booking slot…");
    }
}

fn delete_open(app: &mut App) {
    let Some(slot) = app
        .slots
        .open
        .data()
        .and_then(|slots| slots.get(app.slots.open_sel))
        .cloned()
    else {
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
    app.set_status("closing slot…");
}
