//! The "new open hour" form: date, start/end time, campus and inter-campus
//! toggles, submitting a `CreateSlot` command.

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tui_input::backend::crossterm::EventHandler;

use super::theme;
use super::widgets;
use crate::app::App;
use crate::bus::Command;
use crate::input::Action;
use crate::state::SlotsFocus;

pub fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let form = &app.slots.form;
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .spacing(1)
    .split(area);

    let focused = app.slots.focus == SlotsFocus::Form;
    widgets::input_field(
        frame,
        rows[0],
        " date (YYYY-MM-DD)",
        &form.date,
        focused && form.focus == 0,
        false,
    );
    widgets::input_field(
        frame,
        rows[1],
        " start (HH:MM)",
        &form.start,
        focused && form.focus == 1,
        false,
    );
    widgets::input_field(
        frame,
        rows[2],
        " end (HH:MM)",
        &form.end,
        focused && form.focus == 2,
        false,
    );

    let campus = if form.campus_bx {
        "Brussels"
    } else {
        "Antwerp"
    };
    frame.render_widget(
        widgets::titled_block(" campus ", false).title(
            Span::styled(
                format!(" {campus} "),
                Style::default().fg(theme::campus_color(if form.campus_bx {
                    "bx"
                } else {
                    "anr"
                })),
            )
            .into_right_aligned_line(),
        ),
        rows[3],
    );
    frame.render_widget(
        widgets::titled_block(" inter-campus ", false).title(
            Span::styled(
                if form.remote { " on " } else { " off " },
                if form.remote {
                    theme::good()
                } else {
                    theme::muted()
                },
            )
            .into_right_aligned_line(),
        ),
        rows[4],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "f=focus · c=campus · t=remote · Enter=save",
            theme::muted(),
        ))),
        rows[5],
    );
}

pub fn handle_form_key(app: &mut App, key: KeyEvent) -> Action {
    let form = &mut app.slots.form;
    match key.code {
        KeyCode::Esc => app.slots.focus = SlotsFocus::List,
        KeyCode::Tab | KeyCode::Down => form.focus = (form.focus + 1) % 3,
        KeyCode::Up => form.focus = (form.focus + 2) % 3,
        KeyCode::Char('c') => form.campus_bx = !form.campus_bx,
        KeyCode::Char('t') => form.remote = !form.remote,
        KeyCode::Enter => submit_form(app),
        _ => {
            let input = match form.focus {
                0 => &mut form.date,
                1 => &mut form.start,
                _ => &mut form.end,
            };
            input.handle_event(&crossterm::event::Event::Key(key));
        }
    }
    Action::Continue
}

fn submit_form(app: &mut App) {
    let form = &app.slots.form;
    let Some(date) = NaiveDate::parse_from_str(form.date.value().trim(), "%Y-%m-%d").ok() else {
        app.set_status("date must be YYYY-MM-DD");
        return;
    };
    let Some(begin) = parse_hm(form.start.value(), date) else {
        app.set_status("start must be HH:MM");
        return;
    };
    let Some(end) = parse_hm(form.end.value(), date) else {
        app.set_status("end must be HH:MM");
        return;
    };
    if end <= begin {
        app.set_status("end must be after start");
        return;
    }
    app.send(Command::CreateSlot {
        begin,
        end,
        campus: if form.campus_bx {
            "bx".into()
        } else {
            "anr".into()
        },
        remote: form.remote,
    });
    app.set_status("opening slot…");
}

/// `HH:MM` on a given date -> local `DateTime` (hours ≥ 24 roll over).
fn parse_hm(value: &str, date: NaiveDate) -> Option<DateTime<Local>> {
    let (hours, minutes) = value.trim().split_once(':')?;
    let hours: u32 = hours.trim().parse().ok()?;
    let minutes: u32 = minutes.trim().parse().ok()?;
    let day = if hours >= 24 {
        date + Duration::try_days((hours / 24) as i64).unwrap_or_default()
    } else {
        date
    };
    day.and_hms_opt(hours % 24, minutes.min(59), 0)
        .and_then(|naive| Local.from_local_datetime(&naive).single())
}
