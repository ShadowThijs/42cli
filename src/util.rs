//! Small date/duration helpers shared across screens.

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use chrono_tz::Europe::Brussels;

/// Parse the various timestamp shapes the 42 APIs emit.
pub fn parse_datetime(value: &str) -> Option<DateTime<Local>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Local));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .and_then(|naive| Local.from_local_datetime(&naive).single());
    }
    if let Ok(naive) = value.parse::<chrono::NaiveDateTime>() {
        return Local.from_local_datetime(&naive).single();
    }
    None
}

/// Days from today until `date` (negative if past).
pub fn days_until(value: &str) -> Option<i64> {
    let target = parse_datetime(value)?;
    let today = Local::now().date_naive();
    Some((target.date_naive() - today).num_days())
}

/// `2026-11-06` -> `06 Nov 2026`.
pub fn fmt_date(value: &str) -> String {
    parse_datetime(value)
        .map(|dt| dt.format("%d %b %Y").to_string())
        .unwrap_or_else(|| value.to_owned())
}

/// `2026-08-18T13:30:00` -> `Tue 18 Aug 13:30`.
pub fn fmt_datetime(value: &str) -> String {
    parse_datetime(value)
        .map(|dt| dt.format("%a %d %b %H:%M").to_string())
        .unwrap_or_else(|| value.to_owned())
}

/// "HH:MM:SS" logtime -> seconds.
pub fn hms_to_seconds(value: &str) -> i64 {
    let parts: Vec<i64> = value.split(':').filter_map(|p| p.parse().ok()).collect();
    match parts.len() {
        3 => parts[0] * 3600 + parts[1] * 60 + parts[2],
        2 => parts[0] * 3600 + parts[1] * 60,
        _ => 0,
    }
}

/// Seconds -> "12h 30m".
pub fn fmt_seconds(total_seconds: i64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    format!("{hours}h {minutes:02}m")
}

/// Sum logtime between `from` and `to` (inclusive), keyed by date string.
pub fn sum_logtime_between(
    stats: &std::collections::HashMap<String, String>,
    from: NaiveDate,
    to: NaiveDate,
) -> i64 {
    stats
        .iter()
        .filter_map(|(date, value)| {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .ok()
                .filter(|day| (from..=to).contains(day))
                .map(|_| hms_to_seconds(value))
        })
        .sum()
}

/// Total logtime for the last `days` days including today.
pub fn logtime_last_days(stats: &std::collections::HashMap<String, String>, days: u32) -> i64 {
    let today = Local::now().date_naive();
    let from = today - Duration::try_days(days as i64 - 1).unwrap_or_default();
    sum_logtime_between(stats, from, today)
}

/// The last `days` days as (label, seconds) bars, oldest first.
pub fn logtime_bars(
    stats: &std::collections::HashMap<String, String>,
    days: u32,
) -> Vec<(String, f64)> {
    let today = Local::now().date_naive();
    let mut bars = Vec::new();
    for offset in (0..days as i64).rev() {
        let day = today - Duration::try_days(offset).unwrap_or_default();
        let key = day.format("%Y-%m-%d").to_string();
        let seconds = stats.get(&key).map_or(0, |value| hms_to_seconds(value));
        bars.push((day.format("%d/%m").to_string(), seconds as f64 / 3600.0));
    }
    bars
}

/// ISO-8601 duration (`PT26H30M`, `PT0S`) -> seconds.
pub fn iso_duration_to_seconds(value: &str) -> i64 {
    let mut total = 0i64;
    let mut number = String::new();
    for ch in value.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }
        let amount: i64 = number.parse().unwrap_or(0);
        match ch {
            'H' => total += amount * 3600,
            'M' => total += amount * 60,
            'S' => total += amount,
            _ => {}
        }
        number.clear();
    }
    total
}

/// Wall-clock in Brussels for the header.
pub fn now_brussels() -> String {
    let now = chrono::Utc::now().with_timezone(&Brussels);
    now.format("%a %d %b %H:%M:%S").to_string()
}

/// Level as `4.15` with its percentage toward the next level.
pub fn level_percent(level: f64) -> (u32, u32) {
    let floor = level.floor() as u32;
    let percent = ((level - level.floor()) * 100.0).round() as u32;
    (floor, percent.min(100))
}
