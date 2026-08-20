//! Week calendar shared by both slots modes — the TUI twin of the
//! FullCalendar week view on slots.42belgium.be: the anchor day first
//! (today, until the week is shifted), the full 24 hours in 15-minute
//! cells (the site's snap granularity), a cursor cell and a drag-out
//! range for opening hours.
//!
//! Slots render as background-coloured blocks — campus colour for
//! bookable/open hours, green for yours, red for booked-by-another, grey
//! before the bookable window — with each block's start time printed on
//! its first row. The cursor carries a tooltip popup anchored on the cell.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::theme;
use super::widgets;
use crate::api::models::Slot;
use crate::api::slots::parse_slot_time;
use crate::state::{SlotsFocus, SlotsState};

/// The grid always spans the whole day, exactly like the site.
pub const START_HOUR: u32 = 0;
pub const END_HOUR: u32 = 24;
/// 15-minute cells across 24 hours — the smallest step the site allows.
pub const ROWS: usize = ((END_HOUR - START_HOUR) * 4) as usize;
/// How far ahead a slot must be before it can be booked or opened.
/// Measured against the live backend by `probe_bookable_window`:
/// opening an hour needs a quarter-aligned start ≥ 30 min ahead
/// (403 "Invalid Time" otherwise), and a booking must be strictly more
/// than 30 min ahead — the quarter landing exactly on `now + 30` is 404.
pub const BOOKING_LEAD_MINUTES: i64 = 30;
/// Width of the `HH:MM ` gutter left of the day columns.
const TIME_GUTTER: usize = 7;
/// Earliest quarter-aligned moment anything can be booked/opened from:
/// the first 15-minute boundary strictly after `now + 30 min`.
pub fn bookable_from() -> DateTime<Local> {
    let earliest = Local::now() + Duration::minutes(BOOKING_LEAD_MINUTES);
    let next = (earliest.timestamp() / 900 + 1) * 900;
    chrono::TimeZone::timestamp_opt(&Local, next, 0)
        .single()
        .unwrap_or(earliest)
}

/// Local time of a calendar cell (day column, 15-minute row), counting
/// from the anchor day. `row == ROWS` is the day's midnight bound and
/// rolls onto the next day, so a range ending on the last cell has a
/// valid end time.
pub fn cell_time(anchor: NaiveDate, day: usize, row: usize) -> Option<DateTime<Local>> {
    let minutes = (row as u32) * 15;
    let date = anchor + Duration::days(day as i64 + (minutes / 1440) as i64);
    let naive = date.and_hms_opt(START_HOUR + (minutes % 1440) / 60, (minutes % 1440) % 60, 0)?;
    Local.from_local_datetime(&naive).single()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellKind {
    #[default]
    Empty,
    /// Bookable (booking mode) or an open hour of mine (hours mode).
    Open,
    /// My own reservation — Enter cancels it.
    Mine,
    /// Taken by another student — the site paints these red.
    Booked,
}

#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub kind: CellKind,
    pub campus: String,
    pub remote: bool,
    /// `10:00 → 12:00` — bounds of the slot covering the cell.
    pub label: String,
    /// `10:00` on the first cell of a block, so blocks carry their start.
    pub starts_at: Option<String>,
    /// Before the bookable window (now + lead): shown greyed out.
    pub past: bool,
}

/// Lay `slots` onto the week grid. A `reserved` slot means `Mine` in
/// booking mode (own reservation) but `Booked` in hours mode (a student
/// took the hour); it wins over plain open slots.
pub fn build_cells(slots: &[Slot], anchor: NaiveDate, hours_mode: bool) -> Vec<Vec<Cell>> {
    let mut cells = vec![vec![Cell::default(); ROWS]; 7];
    let window = bookable_from();
    let parsed: Vec<(&Slot, DateTime<Local>, DateTime<Local>)> = slots
        .iter()
        .filter_map(|slot| {
            let start = slot.start.as_deref().and_then(parse_slot_time)?;
            let end = slot.end.as_deref().and_then(parse_slot_time)?;
            Some((slot, start, end))
        })
        .collect();
    for (day, column) in cells.iter_mut().enumerate() {
        for (row, target) in column.iter_mut().enumerate() {
            let Some(at) = cell_time(anchor, day, row) else {
                continue;
            };
            let best = parsed
                .iter()
                .filter(|(_, start, end)| *start <= at && at < *end)
                .min_by_key(|(slot, _, _)| !slot.reserved);
            if let Some((slot, start, end)) = best {
                *target = Cell {
                    kind: if slot.reserved {
                        if hours_mode {
                            CellKind::Booked
                        } else {
                            CellKind::Mine
                        }
                    } else {
                        CellKind::Open
                    },
                    campus: slot.campus.clone().unwrap_or_default(),
                    remote: slot.remote,
                    label: format!("{} → {}", start.format("%H:%M"), end.format("%H:%M")),
                    starts_at: (*start == at).then(|| start.format("%H:%M").to_string()),
                    past: at < window,
                };
            }
        }
    }
    cells
}

/// Render the calendar into `area`. `tooltip` (already composed by the
/// caller) is drawn as a popup anchored next to the cursor cell.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    slots: &SlotsState,
    cells: &[Vec<Cell>],
    title: &str,
    tooltip: &[Line<'static>],
) {
    let block = widgets::titled_block(title, slots.focus == SlotsFocus::Grid);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // One space of separation between the day columns.
    let col_width = (inner.width as usize).saturating_sub(TIME_GUTTER + 6) / 7;
    if col_width < 3 {
        widgets::hint(frame, inner, "terminal too narrow for the week grid");
        return;
    }

    // Header + hour rows; report the visible row count back to key handling.
    let visible = inner.height.saturating_sub(1) as usize;
    slots.grid_view.set(visible as u16);
    let scroll = clamp_scroll(slots, visible);

    let today = Local::now().date_naive();
    let first = slots.week_anchor;
    let mut header = Line::from(Span::raw(" ".repeat(TIME_GUTTER)));
    for day in 0..7 {
        let date = first + Duration::days(day as i64);
        let label = format!(
            "{:<width$}",
            format!("{} {:02}", short_weekday(date), date.day()),
            width = col_width
        );
        header.spans.push(Span::styled(
            label,
            if date == today {
                Style::default()
                    .fg(ratatui::style::Color::Black)
                    .bg(theme::ACCENT)
            } else {
                theme::bright()
            },
        ));
        header.spans.push(Span::raw(" "));
    }
    let mut lines = vec![header];
    for row in scroll..(scroll + visible).min(ROWS) {
        let mut line = Line::from(Span::styled(
            if row % 4 == 0 {
                format!(
                    "{:02}:{:02} ",
                    START_HOUR + row as u32 / 4,
                    15 * (row as u32 % 4)
                )
            } else {
                " ".repeat(TIME_GUTTER - 1)
            },
            theme::muted(),
        ));
        for (day, column) in cells.iter().enumerate() {
            let cursor_here = slots.focus == SlotsFocus::Grid && slots.cursor == (day, row);
            let highlighted = cursor_here || range_covers(slots, day, row);
            if let Some(cell) = column.get(row) {
                line.spans.push(Span::styled(
                    cell_text(cell, col_width),
                    cell_style(cell, highlighted),
                ));
            } else {
                line.spans
                    .push(Span::styled(" ".repeat(col_width), theme::muted()));
            }
            line.spans.push(Span::raw(" "));
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), inner);

    if slots.focus == SlotsFocus::Grid && !tooltip.is_empty() {
        draw_tooltip(frame, inner, col_width, scroll, tooltip, slots.cursor);
    }
}

/// What a cell shows: the block's start time on its first row, otherwise
/// blank — the background colour is the content.
fn cell_text(cell: &Cell, width: usize) -> String {
    match (&cell.starts_at, width) {
        (Some(time), w) if w >= 6 => format!("{time:^w$}"),
        _ => " ".repeat(width),
    }
}

/// A cell's look: filled backgrounds per state, inverted for the cursor
/// and dragged ranges. Colours are backgrounds deliberately — much louder
/// than a one-character glyph in the corner.
fn cell_style(cell: &Cell, highlighted: bool) -> Style {
    if highlighted {
        return Style::default().fg(Color::Black).bg(theme::ACCENT);
    }
    let background = match cell.kind {
        CellKind::Empty => None,
        CellKind::Mine => Some(theme::GOOD),
        CellKind::Booked => Some(theme::ERR),
        CellKind::Open => {
            if cell.past {
                Some(theme::DIM)
            } else {
                Some(theme::campus_color(&cell.campus))
            }
        }
    };
    match background {
        // White text on the strong backgrounds keeps start times legible.
        Some(color) => Style::default().fg(Color::Black).bg(color),
        None => theme::muted(),
    }
}

/// Popup describing the cursor cell, drawn right next to it.
fn draw_tooltip(
    frame: &mut Frame,
    inner: Rect,
    col_width: usize,
    scroll: usize,
    tooltip: &[Line<'static>],
    cursor: (usize, usize),
) {
    let width = 38u16.min(inner.width.saturating_sub(4));
    let height = (tooltip.len() as u16 + 2).min(inner.height);
    if width < 12 || height < 3 {
        return; // No room for a popup — the legend still explains the colours.
    }
    let cell_x = inner.x + TIME_GUTTER as u16 + cursor.0 as u16 * (col_width as u16 + 1);
    let cell_y = inner.y + 1 + (cursor.1.saturating_sub(scroll)) as u16;
    // Prefer the right of the cell, fall back to the left, then clamp.
    let mut x = cell_x + col_width as u16 + 2;
    if x + width > inner.right() {
        x = cell_x.saturating_sub(width + 1);
    }
    x = x.clamp(inner.x, inner.right().saturating_sub(width));
    let y = (cell_y + 1).clamp(inner.y, inner.bottom().saturating_sub(height));

    let popup = Rect {
        x,
        y,
        width,
        height,
    };
    let block = widgets::titled_block(" here ", false);
    let text_area = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    frame.render_widget(Paragraph::new(tooltip.to_vec()), text_area);
}

/// True while the cell is part of the range being dragged out.
fn range_covers(slots: &SlotsState, day: usize, row: usize) -> bool {
    let Some((from_day, from_row)) = slots.range_start else {
        return false;
    };
    day == from_day
        && day == slots.cursor.0
        && row >= from_row.min(slots.cursor.1)
        && row <= from_row.max(slots.cursor.1)
}

/// Keep the scroll window pinned around the cursor and inside the grid.
fn clamp_scroll(slots: &SlotsState, visible: usize) -> usize {
    let max_scroll = ROWS.saturating_sub(visible);
    let cursor_row = slots.cursor.1;
    let mut scroll = (slots.grid_scroll.get() as usize).min(max_scroll);
    if cursor_row < scroll {
        scroll = cursor_row;
    }
    if visible > 0 && cursor_row >= scroll + visible {
        scroll = cursor_row + 1 - visible;
    }
    slots.grid_scroll.set(scroll as u16);
    scroll
}

fn short_weekday(date: NaiveDate) -> &'static str {
    match date.weekday() {
        chrono::Weekday::Mon => "Mon",
        chrono::Weekday::Tue => "Tue",
        chrono::Weekday::Wed => "Wed",
        chrono::Weekday::Thu => "Thu",
        chrono::Weekday::Fri => "Fri",
        chrono::Weekday::Sat => "Sat",
        chrono::Weekday::Sun => "Sun",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Slot;

    fn slot(start: &str, end: &str, campus: &str, reserved: bool) -> Slot {
        Slot {
            start: Some(start.to_owned()),
            end: Some(end.to_owned()),
            campus: Some(campus.to_owned()),
            remote: false,
            reserved,
            feed: campus.to_owned(),
            ..Default::default()
        }
    }

    /// Every cell is a 15-minute step, through all 24 hours, counting from
    /// the anchor day itself.
    #[test]
    fn cells_step_quarter_hours_all_day() {
        let anchor = NaiveDate::from_ymd_opt(2026, 8, 17).unwrap();
        assert_eq!(ROWS, 96);
        let first = cell_time(anchor, 0, 0).unwrap();
        assert_eq!(first.format("%H:%M").to_string(), "00:00");
        let quarter = cell_time(anchor, 2, 1).unwrap();
        assert_eq!(quarter.format("%H:%M").to_string(), "00:15");
        let last = cell_time(anchor, 2, ROWS - 1).unwrap();
        assert_eq!(last.format("%H:%M").to_string(), "23:45");
        // One row past the grid is the midnight bound — a valid range end.
        let midnight = cell_time(anchor, 2, ROWS).unwrap();
        assert_eq!(midnight.format("%H:%M").to_string(), "00:00");
        assert_eq!(
            midnight.date_naive(),
            NaiveDate::from_ymd_opt(2026, 8, 20).unwrap()
        );
    }

    #[test]
    fn blocks_carry_their_start_time() {
        // Day 0 is the anchor day itself: Wednesday the 19th.
        let anchor = NaiveDate::from_ymd_opt(2026, 8, 19).unwrap();
        // Wednesday 10:00–11:30 local (CEST = +02:00, so 08:00–09:30 UTC).
        let slots = vec![
            slot("2026-08-19T08:00:00Z", "2026-08-19T09:30:00Z", "bx", false),
            slot(
                "2026-08-19T08:30:00Z",
                "2026-08-19T09:00:00Z",
                "reserved-bx",
                true,
            ),
        ];
        let cells = build_cells(&slots, anchor, true);
        // 10:00 local = row 40; the block announces itself on the first row.
        assert_eq!(cells[0][40].starts_at.as_deref(), Some("10:00"));
        assert_eq!(cells[0][41].starts_at, None);
        // Reserved wins where it overlaps; both announce their own starts.
        assert_eq!(cells[0][42].kind, CellKind::Booked);
        assert_eq!(cells[0][42].starts_at.as_deref(), Some("10:30"));
        // In booking mode the same reserved slot is *mine* instead.
        let cells = build_cells(&slots, anchor, false);
        assert_eq!(cells[0][42].kind, CellKind::Mine);
        // Nothing on Thursday.
        assert_eq!(cells[1][10].kind, CellKind::Empty);
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use chrono::TimeZone;

    /// The boundary exactly on `now + 30` is not bookable — the next
    /// quarter strictly after it is.
    #[test]
    fn bookable_from_is_the_next_strict_quarter() {
        let now = Local::now();
        let from = bookable_from();
        assert!(from > now + Duration::minutes(BOOKING_LEAD_MINUTES));
        assert_eq!(from.timestamp() % 900, 0, "quarter aligned");
        assert!(from - (now + Duration::minutes(BOOKING_LEAD_MINUTES)) <= Duration::minutes(15));
        // A time exactly on the lead boundary is refused, one quarter later passes.
        let exact = Local
            .timestamp_opt((now.timestamp() / 900 + 3) * 900, 0)
            .single()
            .unwrap();
        assert!(exact >= from);
    }
}
