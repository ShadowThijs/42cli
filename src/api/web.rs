//! Cookie-authenticated intra.42.fr endpoints: the holy-graph project data,
//! user search, cluster occupancy and the `/{slug}/mine` project page
//! (attachments, team, git repository).

use std::time::Duration;

use serde::de::DeserializeOwned;

use super::{Api, ApiError, ApiResult};

const TTL_GRAPH: Duration = Duration::from_secs(30 * 60);
const TTL_CLUSTERS: Duration = Duration::from_secs(60);
const TTL_MINE: Duration = Duration::from_secs(10 * 60);
const TTL_MINE_DONE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const TTL_SCHEDULE_DONE: Duration = Duration::from_secs(24 * 60 * 60);

impl Api {
    /// GET an intra web endpoint with the Rails session cookie, transparently
    /// re-bootstrapping the session once when it has expired. The intranet
    /// answers 401 JSON or a login-page redirect for dead sessions.
    async fn web_get_json<T: DeserializeOwned>(&self, url: &str) -> ApiResult<T> {
        let fetch = || async {
            let resp = self.http.get(url).send().await?;
            let status = resp.status();
            if status.as_u16() == 401 {
                return Err(ApiError::SessionExpired);
            }
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            if !status.is_success() {
                return Err(ApiError::from_response("intra web", resp).await);
            }
            if !content_type.contains("json") {
                // We were bounced to an HTML login page: session is dead.
                return Err(ApiError::SessionExpired);
            }
            resp.json::<T>().await.map_err(|error| ApiError::Parse {
                endpoint: "intra web",
                detail: error.to_string(),
            })
        };
        match fetch().await {
            Ok(value) => Ok(value),
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    fetch().await
                } else {
                    Err(ApiError::SessionExpired)
                }
            }
            Err(error) => Err(error),
        }
    }

    /// All cursus projects with per-user state (holy graph data).
    pub async fn project_data(
        &self,
        cursus_id: u32,
        campus_id: u32,
        fresh: bool,
    ) -> ApiResult<Vec<super::ProjectDataEntry>> {
        let key = format!("project_data/{cursus_id}/{campus_id}");
        if !fresh {
            if let Some(cached) = self.cache.get(&key, TTL_GRAPH) {
                return Ok(cached);
            }
            // Documents, difficulty etc rarely change: allow stale up to 24h optimistically,
            // background refresh will correct passively when fresh=true is requested.
            if let Some((cached, age)) = self
                .cache
                .get_with_age::<Vec<super::ProjectDataEntry>>(&key)
                && age < Duration::from_secs(24 * 60 * 60)
                && !cached.is_empty()
            {
                return Ok(cached);
            }
        }
        let entries: Vec<super::ProjectDataEntry> = self
            .web_get_json(&format!(
                "{}/project_data.json?cursus_id={cursus_id}&campus_id={campus_id}",
                super::PROJECTS_BASE
            ))
            .await?;
        self.cache.put(&key, &entries);
        Ok(entries)
    }

    /// Login-prefix user search (max ~5 hits, same as the web UI).
    pub async fn search_users(&self, query: &str) -> ApiResult<Vec<super::SearchResult>> {
        let url = format!(
            "{}/searches/search.json?query={}",
            super::PROFILE_BASE,
            urlencoding_lite(query)
        );
        self.web_get_json(&url).await
    }

    /// Currently occupied cluster seats across campuses.
    pub async fn cluster_seats(&self, fresh: bool) -> ApiResult<Vec<super::ClusterSeat>> {
        if !fresh && let Some(cached) = self.cache.get("clusters", TTL_CLUSTERS) {
            return Ok(cached);
        }
        let seats: Vec<super::ClusterSeat> = self
            .web_get_json(&format!("{}/clusters.json", super::META_BASE))
            .await?;
        self.cache.put("clusters", &seats);
        Ok(seats)
    }

    /// Raw `/{slug}/mine` page HTML — used by the live tests to develop the
    /// `parse_project_mine` selectors against the real DOM.
    #[cfg(test)]
    pub async fn project_mine_html(&self, slug: &str) -> ApiResult<String> {
        let resp = self
            .http
            .get(format!("{}/{slug}/mine", super::PROJECTS_BASE))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ApiError::from_response("project mine", resp).await);
        }
        Ok(resp.text().await.unwrap_or_default())
    }

    /// Scrape `projects.intra.42.fr/{slug}/mine` for team + attachments.
    /// `mine2` — added evaluation attempts to the cached shape.
    pub async fn project_mine(&self, slug: &str, fresh: bool) -> ApiResult<super::ProjectMine> {
        let key = format!("mine2/{slug}");
        if !fresh {
            if let Some(cached) = self.cache.get::<super::ProjectMine>(&key, TTL_MINE) {
                return Ok(cached);
            }
            // Completed projects rarely change: serve stale up to 7 days optimistically
            if let Some((cached, age)) = self.cache.get_with_age::<super::ProjectMine>(&key) {
                let is_done = cached.status.as_deref() == Some("finished")
                    || cached.status.as_deref() == Some("done")
                    || !cached.evaluations.is_empty();
                if is_done && age < TTL_MINE_DONE {
                    // Return stale but spawn background refresh passively via next call
                    return Ok(cached);
                }
                if age < Duration::from_secs(30 * 60) {
                    // Generic stale fallback for active projects (30m)
                    return Ok(cached);
                }
            }
            if let Some(cached) = self.cache.get_stale::<super::ProjectMine>(&key) {
                // Last resort stale: still better than empty, will be validated in background
                // Only use if we have at least some data
                if !cached.attachments.is_empty() || !cached.evaluations.is_empty() {
                    return Ok(cached);
                }
            }
        }
        let fetch = || async {
            let resp = self
                .http
                .get(format!("{}/{slug}/mine", super::PROJECTS_BASE))
                .send()
                .await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(ApiError::from_response("project mine", resp).await);
            }
            let html = resp.text().await.unwrap_or_default();
            parse_project_mine(&html).ok_or_else(|| ApiError::Parse {
                endpoint: "project mine",
                detail: format!("could not parse `{slug}`"),
            })
        };
        let mine = match fetch().await {
            Ok(mine) => mine,
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    fetch().await?
                } else {
                    return Err(ApiError::SessionExpired);
                }
            }
            Err(error) => return Err(error),
        };
        self.cache.put(&key, &mine);
        Ok(mine)
    }

    /// Scrape `projects.intra.42.fr/{slug}/scale_teams` — the project's
    /// evaluation schedule: who corrects whom and when, plus each attempt's
    /// result and the corrected's feedback once it happened. Page 1 holds
    /// the ~25 most recent bookings.
    pub async fn project_schedule(
        &self,
        slug: &str,
        fresh: bool,
    ) -> ApiResult<Vec<super::ProjectScheduleEntry>> {
        let key = format!("schedule/{slug}");
        if !fresh {
            if let Some(cached) = self.cache.get(&key, TTL_MINE) {
                return Ok(cached);
            }
            if let Some((cached, age)) = self
                .cache
                .get_with_age::<Vec<super::ProjectScheduleEntry>>(&key)
            {
                // If schedule has any graded entries (result Some), treat as done and keep longer
                let has_results = cached.iter().any(|e| e.result.is_some());
                let ttl = if has_results {
                    TTL_SCHEDULE_DONE
                } else {
                    Duration::from_secs(30 * 60)
                };
                if age < ttl {
                    return Ok(cached);
                }
            }
        }
        let fetch = || async {
            let resp = self
                .http
                .get(format!("{}/{slug}/scale_teams", super::PROJECTS_BASE))
                .send()
                .await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(ApiError::from_response("project schedule", resp).await);
            }
            // A dead Rails session bounces to the signin page (HTTP 200).
            if resp
                .url()
                .host_str()
                .is_some_and(|host| host.contains("signin"))
            {
                return Err(ApiError::SessionExpired);
            }
            let html = resp.text().await.unwrap_or_default();
            parse_scale_team_schedule(&html).ok_or_else(|| ApiError::Parse {
                endpoint: "project schedule",
                detail: format!("could not parse `{slug}`"),
            })
        };
        let schedule = match fetch().await {
            Ok(schedule) => schedule,
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    fetch().await?
                } else {
                    return Err(ApiError::SessionExpired);
                }
            }
            Err(error) => return Err(error),
        };
        self.cache.put(&key, &schedule);
        Ok(schedule)
    }

    /// Download a CDN document (subjects, archives) into the downloads dir
    /// and return the destination path.
    pub async fn download_attachment(&self, url: &str, name: &str) -> ApiResult<String> {
        let resp = self
            .http
            .get(url)
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(ApiError::from_response("cdn", resp).await);
        }
        let bytes = resp.bytes().await?;
        let dir = crate::config::downloads_dir();
        let safe_name = name.replace(['/', '\\'], "_");
        let path = dir.join(safe_name);
        std::fs::write(&path, &bytes)
            .map_err(|error| ApiError::Other(format!("write {}: {error}", path.display())))?;
        Ok(path.display().to_string())
    }

    /// Scrape `profile.intra.42.fr/events/{id}` for the full event record.
    pub async fn event_detail(&self, id: u32) -> ApiResult<super::EventDetail> {
        let fetch = || async {
            let resp = self
                .http
                .get(format!("{}/events/{id}", super::PROFILE_BASE))
                .send()
                .await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(ApiError::from_response("event", resp).await);
            }
            // A dead Rails session bounces to the signin page (HTTP 200).
            if resp
                .url()
                .host_str()
                .is_some_and(|host| host.contains("signin"))
            {
                return Err(ApiError::SessionExpired);
            }
            let html = resp.text().await.unwrap_or_default();
            parse_event_detail(&html, id).ok_or_else(|| ApiError::Parse {
                endpoint: "event",
                detail: format!("could not parse event {id}"),
            })
        };
        match fetch().await {
            Ok(event) => Ok(event),
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    fetch().await
                } else {
                    Err(ApiError::SessionExpired)
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Subscribe (`subscribe = true`) or unsubscribe to an event by
    /// replaying the rails-ujs form behind the page footer link: POST with
    /// the page CSRF token, and `_method=delete` in the body to override
    /// the verb for unsubscription.
    pub async fn set_event_subscription(
        &self,
        url: &str,
        csrf_token: &str,
        subscribe: bool,
    ) -> ApiResult<()> {
        let send = || async {
            let mut request = self.http.post(url).header("X-CSRF-Token", csrf_token);
            if !subscribe {
                request = request.form(&[("_method", "delete")]);
            }
            let resp = request.send().await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(ApiError::from_response("event subscribe", resp).await);
            }
            // A dead Rails session bounces to the signin page (HTTP 200).
            if resp
                .url()
                .host_str()
                .is_some_and(|host| host.contains("signin"))
            {
                return Err(ApiError::SessionExpired);
            }
            Ok(())
        };
        match send().await {
            Ok(()) => Ok(()),
            Err(ApiError::SessionExpired) => {
                if super::auth::bootstrap_intra_session(self).await {
                    send().await
                } else {
                    Err(ApiError::SessionExpired)
                }
            }
            Err(error) => Err(error),
        }
    }
}

fn parse_project_mine(html: &str) -> Option<super::ProjectMine> {
    use scraper::{Html, Selector};
    let document = Html::parse_document(html);

    let mut mine = super::ProjectMine {
        status: select_attr(&document, ".project-status-box", "data-status"),
        ..Default::default()
    };

    if let Ok(selector) = Selector::parse(".team-name") {
        mine.team_name = document
            .select(&selector)
            .next()
            .map(|element| element.text().collect::<String>().trim().to_owned());
    }

    // Teammates: only the `.team-users-list` block. The wider
    // `.team-content` also contains the evaluation history, whose
    // "Evaluated by" links used to leak into the team list.
    if let (Ok(selector), Ok(user_selector)) = (
        Selector::parse(".team-users-list"),
        Selector::parse(r#"a[href*="profile.intra.42.fr/users/"]"#),
    ) {
        for list in document.select(&selector) {
            for link in list.select(&user_selector) {
                if let Some(href) = link.attr("href")
                    && let Some(login) = href.rsplit('/').next()
                    && !login.is_empty()
                    && !mine.members.iter().any(|m| m == login)
                {
                    mine.members.push(login.to_owned());
                }
            }
        }
    }

    // Evaluations: one `.correction-item` per attempt, each carrying the
    // corrector login, a result badge, a closing comment, the defense date,
    // a flag reason when flagged, and the feedback link while feedback on
    // the correction is still pending.
    if let (
        Ok(item_selector),
        Ok(corrector_selector),
        Ok(header_selector),
        Ok(badge_selector),
        Ok(comment_selector),
        Ok(date_selector),
        Ok(flag_selector),
        Ok(feedback_selector),
    ) = (
        Selector::parse(".correction-item"),
        Selector::parse(r#"a[data-tooltip-login]"#),
        Selector::parse(".corrected-header"),
        Selector::parse("b.pull-right"),
        Selector::parse(".correction-comment-item > span"),
        Selector::parse("span[data-long-date]"),
        Selector::parse(r#"span.iconf-folder-1[title]"#),
        Selector::parse(r#"a[href*="/feedbacks/new"]"#),
    ) {
        for item in document.select(&item_selector) {
            let mut evaluation = super::ProjectEvaluation::default();
            for link in item.select(&corrector_selector) {
                if let Some(login) = link.attr("data-tooltip-login")
                    && !login.is_empty()
                    && !evaluation.correctors.iter().any(|c| c == login)
                {
                    evaluation.correctors.push(login.to_owned());
                }
            }
            if let Some(header) = item.select(&header_selector).next() {
                evaluation.result = header
                    .select(&badge_selector)
                    .next()
                    .map(|badge| {
                        badge
                            .text()
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .filter(|text| !text.is_empty());
            }
            evaluation.comment = item
                .select(&comment_selector)
                .next()
                .map(|span| {
                    span.text()
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .filter(|text| !text.is_empty());
            evaluation.evaluated_at = item
                .select(&date_selector)
                .next()
                .and_then(|span| span.attr("data-long-date"))
                .and_then(long_date_to_rfc3339);
            // A flagged attempt carries its reason on the danger folder icon
            // (`title='empty_work'`); unflagged attempts have no such icon.
            evaluation.flag_reason = item
                .select(&flag_selector)
                .next()
                .and_then(|icon| icon.attr("title"))
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .map(str::to_owned);
            if let Some(link) = item.select(&feedback_selector).next()
                && let Some(href) = link.attr("href")
            {
                evaluation.feedback_url = Some(format!(
                    "{}{href}",
                    if href.starts_with("http") {
                        ""
                    } else {
                        super::PROJECTS_BASE
                    }
                ));
                evaluation.scale_team_id = href.split('/').find_map(|part| part.parse().ok());
            }
            if !evaluation.correctors.is_empty() {
                mine.evaluations.push(evaluation);
            }
        }
    }

    if let Ok(selector) = Selector::parse(".team-repo input.form-control") {
        mine.git_repo = document
            .select(&selector)
            .next()
            .and_then(|element| element.attr("value"))
            .map(str::to_owned);
    }

    if let Ok(selector) = Selector::parse("span[data-long-date]") {
        mine.locked_at = document
            .select(&selector)
            .next()
            .and_then(|element| element.attr("data-long-date"))
            .map(str::to_owned);
    }

    if let (Ok(selector), name_selector) = (
        Selector::parse(".project-attachment-item"),
        Selector::parse(".attachment-name a").expect("static selector"),
    ) {
        for item in document.select(&selector) {
            if let Some(link) = item.select(&name_selector).next()
                && let Some(href) = link.attr("href")
            {
                let name = link.text().collect::<String>().trim().to_owned();
                if !name.is_empty() {
                    mine.attachments.push(super::Attachment {
                        name,
                        url: href.to_owned(),
                    });
                }
            }
        }
    }

    if let Ok(selector) = Selector::parse(r#"a[data-method="delete"][href*="projects_users"]"#) {
        mine.unsub_url = document
            .select(&selector)
            .next()
            .and_then(|element| element.attr("href"))
            .map(str::to_owned);
    }

    Some(mine)
}

/// Parse `/{slug}/scale_teams`: one `.scaleteam-list-item` per booking —
/// header "A will correct B's group scheduled on <date>", then optional
/// final mark, comment and corrected-feedback blocks once graded.
fn parse_scale_team_schedule(html: &str) -> Option<Vec<super::ProjectScheduleEntry>> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let item_selector = Selector::parse(".scaleteam-list-item").ok()?;
    let login_selector = Selector::parse(r#"a[data-tooltip-login]"#).ok()?;
    let mark_selector = Selector::parse(".final-mark .rating").ok()?;
    let comment_selector = Selector::parse(".final-mark .comment").ok()?;
    let feedback_selector = Selector::parse(".feedback [data-toggle='tooltip']").ok()?;

    let mut entries = Vec::new();
    for item in document.select(&item_selector) {
        let mut entry = super::ProjectScheduleEntry::default();
        // Header pattern: <b>corrector</b> will correct <b>corrected</b>
        // scheduled on <b>date</b> — the date is the only <b> without a
        // user link inside.
        for header in item.select(&Selector::parse(".header").ok()?) {
            let mut logins = header.select(&login_selector).filter_map(|link| {
                let login = link.attr("data-tooltip-login").unwrap_or_default();
                (!login.is_empty()).then_some(login.to_owned())
            });
            entry.corrector = logins.next();
            entry.corrected = logins.next();
            if let Ok(b_selector) = Selector::parse("b") {
                for bold in header.select(&b_selector) {
                    if bold.select(&login_selector).next().is_none() {
                        let text = collapse_ws(&bold.text().collect::<String>());
                        if !text.is_empty() {
                            entry.scheduled_at = Some(text);
                            break;
                        }
                    }
                }
            }
        }
        let text_of = |selector: &Selector| {
            item.select(selector)
                .next()
                .map(|element| collapse_ws(&element.text().collect::<String>()))
                .filter(|text| !text.is_empty())
        };
        // Upcoming rows render an empty rating shell ("%" with no mark).
        entry.result = text_of(&mark_selector).filter(|mark| mark != "%");
        entry.comment = text_of(&comment_selector);
        entry.feedback = item
            .select(&feedback_selector)
            .next()
            .and_then(|element| element.attr("title"))
            .map(collapse_ws)
            .filter(|title| !title.is_empty());
        if entry.corrector.is_some() || entry.corrected.is_some() {
            entries.push(entry);
        }
    }
    // Zero rows is a valid schedule (no evaluations yet): recognize the
    // page by its "Evaluations made for …" heading instead of failing.
    if entries.is_empty()
        && !document
            .select(&Selector::parse("h2.main-title").ok()?)
            .any(|heading| heading.text().collect::<String>().contains("Evaluations"))
    {
        return None;
    }
    Some(entries)
}

fn select_attr(document: &scraper::Html, selector: &str, attr: &str) -> Option<String> {
    use scraper::Selector;
    Selector::parse(selector)
        .ok()
        .and_then(|selector| {
            document
                .select(&selector)
                .next()
                .and_then(|element| element.attr(attr))
        })
        .map(str::to_owned)
}

/// Parse the server-rendered event modal on `profile.intra.42.fr/events/{id}`.
fn parse_event_detail(html: &str, id: u32) -> Option<super::EventDetail> {
    use scraper::{ElementRef, Html, Selector};

    let document = Html::parse_document(html);
    let text_of = |selector: &str| {
        Selector::parse(selector).ok().and_then(|selector| {
            document
                .select(&selector)
                .next()
                .map(|element| collapse_ws(&element.text().collect::<String>()))
        })
    };

    let name = text_of(".event-modal header .head h4").filter(|name| !name.is_empty())?;
    let kind = text_of(".event-modal header .head .kind");
    let location = text_of(".event-modal .event-location");

    // `<span data-long-date='2026-09-29 16:00:00 +0200'>`: first is the
    // start, a second one (when present) is the end.
    let mut dates = Vec::new();
    if let Ok(selector) = Selector::parse(".event-modal span[data-long-date]") {
        for element in document.select(&selector) {
            if let Some(raw) = element.attr("data-long-date") {
                dates.push(long_date_to_rfc3339(raw).unwrap_or_else(|| raw.to_owned()));
            }
        }
    }
    let (begin_at, end_at) = match dates.as_slice() {
        [first] => (Some(first.clone()), None),
        [first, second, ..] => (Some(first.clone()), Some(second.clone())),
        [] => (None, None),
    };

    // Duration lives next to the clock icon, printed as "for 6 days".
    let duration = Selector::parse(".event-modal .icon-clock")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .and_then(|icon| ElementRef::wrap(icon.parent()?))
        .map(|span| collapse_ws(&span.text().collect::<String>()))
        .map(|text| {
            text.strip_prefix("for ")
                .unwrap_or(text.as_str())
                .to_owned()
        });

    // Sign-ups render as a single "15 / 50" text.
    let (current_subscribers, max_subscribers) = text_of(".event-modal .event-suscriptions")
        .map(|text| {
            let mut parts = text.split('/');
            (
                parts.next().and_then(|v| v.trim().parse().ok()),
                parts.next().and_then(|v| v.trim().parse().ok()),
            )
        })
        .unwrap_or((None, None));

    // The subscribe button switches to data-method="delete" once signed up;
    // the footer links are the rails-ujs forms we replay for s/u actions.
    let subscribe_url = select_attr(
        &document,
        r#"a[data-method="post"][href*="events_users"]"#,
        "href",
    )
    .map(|href| absolutize(&href));
    let unsubscribe_url = select_attr(
        &document,
        r#"a[data-method="delete"][href*="events_users"]"#,
        "href",
    )
    .map(|href| absolutize(&href));
    let is_subscribed = unsubscribe_url.is_some();

    // Full markdown description rides along in data-markdownable.
    let description = select_attr(
        &document,
        ".event-modal .notification-text",
        "data-markdownable",
    )
    .or_else(|| text_of(".event-modal .notification-text"));

    let csrf_token = select_attr(&document, r#"meta[name="csrf-token"]"#, "content");

    Some(super::EventDetail {
        id,
        name: Some(name),
        kind,
        begin_at,
        end_at,
        duration,
        location,
        description,
        current_subscribers,
        max_subscribers,
        is_subscribed,
        csrf_token,
        subscribe_url,
        unsubscribe_url,
    })
}

/// Turn the page-relative footer href into an absolute URL.
fn absolutize(href: &str) -> String {
    if href.starts_with("http") {
        href.to_owned()
    } else {
        format!("{}{href}", super::PROFILE_BASE)
    }
}

/// Collapse runs of whitespace (HTML indentation) into single spaces.
fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `2026-09-29 16:00:00 +0200` (data-long-date) -> RFC 3339.
fn long_date_to_rfc3339(raw: &str) -> Option<String> {
    chrono::DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S %z")
        .map(|at| at.to_rfc3339())
        .ok()
}

/// Percent-encode a query value without pulling in a whole crate for it.
fn urlencoding_lite(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
