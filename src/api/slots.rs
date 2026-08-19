//! `slots.42belgium.be` — open-hour slots and project slot booking.

use chrono::{DateTime, Datelike, Local, TimeZone};
use serde_json::Value;

use super::{Api, ApiError, ApiResult, Slot};

/// Campus identifiers used by the slots service.
pub const CAMPUS_BX: &str = "bx";
pub const CAMPUS_ANR: &str = "anr";

/// Slot feed names accepted by the API.
pub const RESERVED_FEEDS: &[&str] = &["reserved-bx", "reserved-anr"];

impl Api {
    fn slots_headers(&self) -> ApiResult<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        let csrf = self.slots_csrf().ok_or(ApiError::MissingCsrf)?;
        headers.insert(
            "X-CSRFToken",
            csrf.parse().map_err(|_| ApiError::MissingCsrf)?,
        );
        headers.insert("X-Requested-With", "XMLHttpRequest".parse().unwrap());
        Ok(headers)
    }

    /// Make sure the Django session is live; re-run the OAuth chain once if
    /// the slots site bounces us to the login page.
    async fn ensure_slots_session(&self) -> ApiResult<()> {
        if self.has_slots_session() {
            let resp = self
                .noredirect
                .get(format!("{}/slots", super::SLOTS_BASE))
                .send()
                .await?;
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if super::auth::bootstrap_slots_session(self).await {
            Ok(())
        } else {
            Err(ApiError::Other(
                "cannot reach slots.42belgium.be — log in again".into(),
            ))
        }
    }

    /// The API answers `{}` (object) when a feed is empty and a list
    /// otherwise; normalize both to `Vec<Slot>`.
    async fn slots_feed(&self, path: &str, params: &[(&str, String)]) -> ApiResult<Vec<Slot>> {
        self.ensure_slots_session().await?;
        let url = format!("{}/{path}", super::SLOTS_BASE);
        let resp = self
            .http
            .get(&url)
            .query(params)
            .headers(self.slots_headers()?)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ApiError::from_response("slots", resp).await);
        }
        let value: Value = resp.json().await.map_err(|error| ApiError::Parse {
            endpoint: "slots",
            detail: error.to_string(),
        })?;
        match value {
            Value::Array(items) => Ok(items
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect()),
            _ => Ok(Vec::new()),
        }
    }

    async fn slots_write(
        &self,
        method: reqwest::Method,
        path: &str,
        form: &[(&str, String)],
    ) -> ApiResult<()> {
        self.ensure_slots_session().await?;
        let url = format!("{}/{path}", super::SLOTS_BASE);
        let resp = self
            .http
            .request(method, &url)
            .headers(self.slots_headers()?)
            .header(reqwest::header::REFERER, super::SLOTS_BASE)
            .form(form)
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(ApiError::from_response("slots", resp).await)
        }
    }

    /// Raw feed JSON — live-test helper to inspect undocumented fields.
    #[cfg(test)]
    pub async fn slots_feed_raw(&self, path: &str, params: &[(&str, String)]) -> ApiResult<String> {
        self.ensure_slots_session().await?;
        let url = format!("{}/{path}", super::SLOTS_BASE);
        let resp = self
            .http
            .get(&url)
            .query(params)
            .headers(self.slots_headers()?)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ApiError::from_response("slots", resp).await);
        }
        Ok(resp.text().await.unwrap_or_default())
    }

    /// Projects the user may book slots for.
    pub async fn slots_projects(&self) -> ApiResult<Vec<super::SlotsProject>> {
        self.ensure_slots_session().await?;
        let resp = self
            .http
            .get(format!("{}/api/projects/", super::SLOTS_BASE))
            .headers(self.slots_headers()?)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ApiError::from_response("slots projects", resp).await);
        }
        Ok(resp
            .json::<Vec<super::SlotsProject>>()
            .await
            .unwrap_or_default())
    }

    /// Ask the backend to resync projects from the intranet.
    pub async fn slots_sync_projects(&self) -> ApiResult<()> {
        self.slots_write(reqwest::Method::POST, "api/sync/", &[])
            .await
    }

    /// My open-hour slots at both campuses for the `days` days starting at
    /// `anchor`'s Monday (the calendar's fetch window).
    pub async fn open_slots(&self, anchor: chrono::NaiveDate, days: i64) -> ApiResult<Vec<Slot>> {
        let (start, end) = range_for(anchor, days);
        let mut all = Vec::new();
        for status in [CAMPUS_BX, CAMPUS_ANR] {
            let mut slots = self
                .slots_feed(
                    "api/slot",
                    &[
                        ("status", status.to_string()),
                        ("start", start.clone()),
                        ("end", end.clone()),
                    ],
                )
                .await?;
            for slot in &mut slots {
                slot.feed = status.to_string();
                if slot.campus.is_none() {
                    slot.campus = Some(status.to_string());
                }
            }
            all.extend(slots);
        }
        all.sort_by(|a, b| a.start.cmp(&b.start));
        Ok(all)
    }

    /// My project-slot reservations for the `days` days starting at
    /// `anchor`'s Monday.
    pub async fn reserved_slots(
        &self,
        anchor: chrono::NaiveDate,
        days: i64,
    ) -> ApiResult<Vec<Slot>> {
        let (start, end) = range_for(anchor, days);
        let mut all = Vec::new();
        for status in RESERVED_FEEDS {
            let mut slots = self
                .slots_feed(
                    "api/slot",
                    &[
                        ("status", status.to_string()),
                        ("start", start.clone()),
                        ("end", end.clone()),
                    ],
                )
                .await?;
            for slot in &mut slots {
                slot.feed = status.to_string();
                slot.reserved = true;
                if slot.campus.is_none() {
                    slot.campus = Some(status.trim_start_matches("reserved-").to_string());
                }
            }
            all.extend(slots);
        }
        all.sort_by(|a, b| a.start.cmp(&b.start));
        Ok(all)
    }

    /// Open a new availability slot.
    pub async fn create_open_slot(
        &self,
        begin_at: DateTime<Local>,
        end_at: DateTime<Local>,
        campus: &str,
        remote: bool,
    ) -> ApiResult<()> {
        self.slots_write(
            reqwest::Method::POST,
            "api/slot",
            &[
                ("begin_at", rfc3339_local(begin_at)),
                ("end_at", rfc3339_local(end_at)),
                ("campus", campus.to_string()),
                ("remote", remote.to_string()),
            ],
        )
        .await
    }

    /// Close an open availability slot.
    pub async fn delete_open_slot(
        &self,
        start: DateTime<Local>,
        end: DateTime<Local>,
    ) -> ApiResult<()> {
        self.slots_write(
            reqwest::Method::DELETE,
            "api/slot",
            &[("start", rfc3339_local(start)), ("end", rfc3339_local(end))],
        )
        .await
    }

    /// Slots for a project session, filtered exactly like the site's
    /// booking calendar: your campus's local feed, the other campus's
    /// remote feed when inter-campus is on, plus your own reservations.
    pub async fn project_slots(
        &self,
        ps_id: u32,
        anchor: chrono::NaiveDate,
        days: i64,
        campus: &str,
        remote: bool,
    ) -> ApiResult<Vec<Slot>> {
        let (start, end) = range_for(anchor, days);
        let local = format!("{}-local", campus);
        let other_remote = format!(
            "remote-{}",
            if campus == CAMPUS_BX {
                CAMPUS_ANR
            } else {
                CAMPUS_BX
            }
        );
        let mut feeds: Vec<String> = vec![local];
        if remote {
            feeds.push(other_remote);
        }
        feeds.extend(RESERVED_FEEDS.iter().map(|feed| feed.to_string()));
        let mut all = Vec::new();
        for status in feeds {
            if let Ok(mut slots) = self
                .slots_feed(
                    &format!("api/project_slots/{ps_id}"),
                    &[
                        ("status", status.to_string()),
                        ("start", start.clone()),
                        ("end", end.clone()),
                    ],
                )
                .await
            {
                let reserved = status.starts_with("reserved-");
                for slot in &mut slots {
                    slot.feed = status.to_string();
                    slot.reserved = reserved;
                }
                all.extend(slots);
            }
        }
        all.sort_by(|a, b| a.start.cmp(&b.start));
        all.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.campus == b.campus);
        Ok(all)
    }

    /// Book a project slot. `time` is the slot's exact `start` string.
    pub async fn book_project_slot(&self, ps_id: u32, time: &str, campus: &str) -> ApiResult<()> {
        self.slots_write(
            reqwest::Method::POST,
            &format!("api/project_slots/{ps_id}"),
            &[("time", time.to_string()), ("campus", campus.to_string())],
        )
        .await
    }
}

/// RFC 3339 with local offset, e.g. `2026-08-16T23:00:00+02:00` — the exact
/// shape FullCalendar posts.
fn rfc3339_local(at: DateTime<Local>) -> String {
    at.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// `[start, end]` range strings covering the Monday of `anchor`'s week ..
/// that + `days` — the window the displayed calendar needs.
fn range_for(anchor: chrono::NaiveDate, days: i64) -> (String, String) {
    let monday = anchor - chrono::Duration::days(anchor.weekday().num_days_from_monday() as i64);
    let to_local_midnight = |date: chrono::NaiveDate| {
        date.and_hms_opt(0, 0, 0)
            .and_then(|naive| Local.from_local_datetime(&naive).single())
            .unwrap_or_else(Local::now)
    };
    let begin = to_local_midnight(monday);
    let end = to_local_midnight(monday + chrono::Duration::try_days(days).unwrap_or_default());
    (rfc3339_local(begin), rfc3339_local(end))
}

/// Parse a slot timestamp (`Z` or offset form) for display.
pub fn parse_slot_time(value: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

/// Human label for a slot campus code.
pub fn campus_label(code: &str) -> &'static str {
    match code {
        CAMPUS_BX => "Brussels",
        CAMPUS_ANR => "Antwerp",
        other if other.ends_with(CAMPUS_BX) => "Brussels (remote)",
        other if other.ends_with(CAMPUS_ANR) => "Antwerp (remote)",
        _ => "Unknown",
    }
}

/// Weekday + date + time range for a slot, e.g. `Mon 18 Aug  15:00 → 17:00`.
pub fn slot_label(slot: &Slot) -> String {
    let Some(start) = slot.start.as_deref().and_then(parse_slot_time) else {
        return String::new();
    };
    let end = slot.end.as_deref().and_then(parse_slot_time);
    let date = start.format("%a %d %b");
    match end {
        Some(end) => format!(
            "{date}  {} → {}",
            start.format("%H:%M"),
            end.format("%H:%M")
        ),
        None => format!("{date}  {}", start.format("%H:%M")),
    }
}
