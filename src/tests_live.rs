//! Live end-to-end tests against the real 42 endpoints.
//!
//! Ignored by default — run explicitly with credentials from the capture
//! environment (never stored in the repo):
//!
//! ```sh
//! CLI42_TEST_USER=… CLI42_TEST_PASS=… cargo test -- --ignored --nocapture
//! ```

#![cfg(test)]

use std::sync::Arc;

use crate::api::{Api, auth};

fn credentials() -> Option<(String, String)> {
    let user = std::env::var("CLI42_TEST_USER").ok()?;
    let pass = std::env::var("CLI42_TEST_PASS").ok()?;
    Some((user, pass))
}

async fn logged_in_api() -> Arc<Api> {
    let Some((user, pass)) = credentials() else {
        panic!("CLI42_TEST_USER / CLI42_TEST_PASS not set");
    };
    let cookies = Arc::new(crate::cookies::PersistentCookieStore::new());
    let api = Arc::new(Api::new(cookies, None).expect("api client"));
    let outcome = auth::login(&api, &user, &pass)
        .await
        .expect("headless login through keycloak -> intra -> 42belgium");
    assert_eq!(outcome.login, user, "jwt identity must match the login");
    assert!(api.has_intra_session(), "intra rails session cookie");
    assert!(api.has_slots_session(), "42belgium django session cookie");
    api
}

#[tokio::test]
#[ignore]
async fn login_and_read_everything() {
    let api = logged_in_api().await;

    // Bearer APIs.
    let summary = api.me_summary(true).await.expect("me summary");
    assert!(summary.login.is_some());

    let id = summary.id.expect("numeric id");
    let cursus = api.user_cursus(id, true).await.expect("cursus");
    assert!(!cursus.is_empty(), "at least one cursus");

    let profile = api
        .user_profile(summary.login.as_deref().unwrap(), true)
        .await
        .expect("profile");
    assert!(profile.evaluation_points.is_some());

    api.my_events(true).await.expect("events");
    let notifications = api.my_notifications().await.expect("notifications");
    assert!(notifications.unread < 100, "sane unread count");
    api.my_scale_teams().await.expect("scale teams");
    api.locations_stats(summary.login.as_deref().unwrap(), true)
        .await
        .expect("logtime stats");
    api.pace_profile(id).await.expect("pace profile");
    api.attendance_summary(id).await.expect("attendance");
    api.user_achievements(id).await.expect("achievements");
    api.marked_projects(summary.login.as_deref().unwrap(), 21)
        .await
        .expect("marked projects");
    api.ongoing_projects(id, 21).await.expect("ongoing");

    // Cookie APIs (intra web).
    let graph = api.project_data(21, 12, true).await.expect("project graph");
    assert!(
        graph.len() > 100,
        "holy graph should be large, got {}",
        graph.len()
    );

    let seats = api.cluster_seats(true).await.expect("clusters");
    assert!(!seats.is_empty(), "someone is logged in");

    let results = api.search_users("asy").await.expect("user search");
    assert!(!results.is_empty(), "prefix search finds users");

    let mine = api
        .project_mine("netpractice", true)
        .await
        .expect("mine page");
    assert!(!mine.attachments.is_empty(), "subject pdf attached");

    // Slots service.
    let anchor = chrono::Local::now().date_naive();
    let _open = api.open_slots(anchor, 21).await.expect("open slots");
    let _reserved = api
        .reserved_slots(anchor, 21)
        .await
        .expect("reserved slots");
    let projects = api.slots_projects().await.expect("slots projects");
    for project in projects.iter().take(1) {
        if let Some(ps_id) = project.id {
            api.project_slots(ps_id, anchor, 21, "anr", true)
                .await
                .expect("project slots");
        }
    }

    // Session file written with cookies for next start.
    auth::persist_session(&api).await;
    let stored = crate::config::load_session().expect("persisted session");
    assert!(!stored.cookies.is_empty(), "cookies persisted");
}

#[tokio::test]
#[ignore]
async fn restore_from_persisted_session() {
    // Requires a session file written by `login_and_read_everything`.
    let stored = crate::config::load_session().expect("session file");
    let cookies = Arc::new(crate::cookies::PersistentCookieStore::from_snapshot(
        stored.cookies,
        chrono::Utc::now().timestamp(),
    ));
    let api = Arc::new(Api::new(cookies, None).expect("api client"));
    api.set_tokens(crate::api::TokenSet {
        access_token: stored.access_token,
        refresh_token: stored.refresh_token,
        access_expires_at: 0, // force a refresh
    })
    .await;
    api.refresh().await.expect("refresh from stored token");
    api.me_summary(true).await.expect("summary after refresh");
}

/// Restore the session from the local session file, without credentials —
/// used to develop scrapers against live pages.
async fn api_from_persisted_session() -> Arc<Api> {
    let stored = crate::config::load_session().expect("session file");
    let cookies = Arc::new(crate::cookies::PersistentCookieStore::from_snapshot(
        stored.cookies,
        chrono::Utc::now().timestamp(),
    ));
    let api = Arc::new(Api::new(cookies, None).expect("api client"));
    api.set_tokens(crate::api::TokenSet {
        access_token: stored.access_token,
        refresh_token: stored.refresh_token,
        access_expires_at: stored.access_expires_at,
    })
    .await;
    api.me_summary(false).await.expect("live session");
    if !api.has_intra_session() {
        crate::api::auth::bootstrap_intra_session(&api).await;
    }
    api
}

/// Dump the raw `/{slug}/mine` HTML of done projects to /tmp so selector
/// changes can be developed against the real attempt/evaluation DOM.
#[tokio::test]
#[ignore]
async fn dump_done_project_mine_html() {
    let api = api_from_persisted_session().await;
    for slug in ["call-me-maybe", "a-maze-ing", "fly-in"] {
        let html = api.project_mine_html(slug).await.expect("mine html");
        let path = format!("/tmp/mine-{slug}.html");
        std::fs::write(&path, &html).expect("write dump");
        println!("wrote {path} ({} bytes)", html.len());
    }
}

/// Teammates and evaluation correctors must stay separate: one evaluation
/// entry per attempt, correctors never on the team.
#[tokio::test]
#[ignore]
async fn done_project_evaluations_are_not_team() {
    let api = api_from_persisted_session().await;

    // Group project: real teammates in `.team-users-list`.
    let maze = api.project_mine("a-maze-ing", true).await.expect("mine");
    assert!(
        !maze.members.is_empty(),
        "group project lists its team (got {:?})",
        maze.members
    );
    assert!(
        !maze.evaluations.is_empty(),
        "done project lists its attempts"
    );
    for evaluation in &maze.evaluations {
        assert!(!evaluation.correctors.is_empty(), "attempt has a corrector");
        for corrector in &evaluation.correctors {
            assert!(
                !maze.members.contains(corrector),
                "{corrector} evaluated the project — not a teammate"
            );
        }
        println!(
            "attempt: {} -> {} ({:?})",
            evaluation.correctors.join(","),
            evaluation.result.as_deref().unwrap_or("?"),
            evaluation.comment.as_deref().unwrap_or("")
        );
    }

    // Solo project: no `.team-users-list` on the page, but attempts still
    // parse and never leak into `members`.
    let solo = api.project_mine("call-me-maybe", true).await.expect("mine");
    assert!(solo.members.is_empty(), "solo project has no team list");
    assert!(!solo.evaluations.is_empty(), "solo project has attempts");
    assert!(solo.evaluations.iter().any(|e| e.result.is_some()));
}

/// Inspect the raw slots feeds once: what fields mark a slot as booked and
/// by whom, so the calendar can render booked vs available honestly.
#[tokio::test]
#[ignore]
async fn dump_slots_feed_json() {
    let api = api_from_persisted_session().await;
    let projects = api.slots_projects().await.expect("projects");
    let params = |feed: &str, start: &str, end: &str| {
        vec![
            ("status", feed.to_string()),
            ("start", start.to_string()),
            ("end", end.to_string()),
        ]
    };
    let (start, end) = ("2026-08-17T00:00:00+02:00", "2026-08-31T00:00:00+02:00");
    for feed in ["bx", "reserved-bx"] {
        let raw = api
            .slots_feed_raw("api/slot", &params(feed, start, end))
            .await
            .expect("open feed");
        println!("== api/slot status={feed} ==\n{raw}\n");
    }
    if let Some(project) = projects.iter().find(|p| p.id.is_some()) {
        let ps_id = project.id.unwrap();
        for feed in ["bx", "reserved-bx"] {
            let raw = api
                .slots_feed_raw(
                    &format!("api/project_slots/{ps_id}"),
                    &params(feed, start, end),
                )
                .await
                .unwrap_or_default();
            println!("== api/project_slots/{ps_id} status={feed} ==\n{raw}\n");
        }
    }
}

/// Fetch the slots website's calendar page + its JS bundle so the grid can
/// mirror how the site itself renders booked vs open hours.
#[tokio::test]
#[ignore]
async fn dump_slots_site_assets() {
    let api = api_from_persisted_session().await;
    let html = api.slots_feed_raw("slots", &[]).await.expect("slots page");
    std::fs::write("/tmp/slots-page.html", &html).expect("write");
    println!("page: {} bytes", html.len());
    let mut js = 0;
    for line in html.lines() {
        if let Some(start) = line.find("src=\"/") {
            let rest = &line[start + 6..];
            if let Some(end) = rest.find('"') {
                let path = &rest[..end];
                if path.ends_with(".js") {
                    let bundle = api
                        .slots_feed_raw(&path[1..], &[])
                        .await
                        .unwrap_or_default();
                    let name = path.rsplit('/').next().unwrap_or("bundle.js");
                    std::fs::write(format!("/tmp/slots-{name}"), &bundle).expect("write");
                    println!("js {path}: {} bytes", bundle.len());
                    js += 1;
                }
            }
        }
    }
    assert!(js > 0, "at least one script");
}

/// Fetch the booking calendar page for one project (Codexion when present)
/// so we can mirror exactly which feeds/filters the site itself uses.
#[tokio::test]
#[ignore]
async fn dump_project_slots_page() {
    let api = api_from_persisted_session().await;
    let projects = api.slots_projects().await.expect("projects");
    for project in &projects {
        println!("project: {:?} id={:?}", project.name, project.id);
    }
    let target = projects
        .iter()
        .find(|p| {
            p.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case("codexion"))
        })
        .or_else(|| projects.first());
    let ps_id = target.and_then(|p| p.id).expect("a project id");
    let html = api
        .slots_feed_raw(&format!("projects/slots?ps_id={ps_id}"), &[])
        .await
        .expect("booking page");
    std::fs::write("/tmp/project-slots-page.html", &html).expect("write");
    println!(
        "page for {} (ps_id={ps_id}): {} bytes",
        target.unwrap().name.clone().unwrap_or_default(),
        html.len()
    );
}

/// What does the live intra graph say about Codexion right now?
#[tokio::test]
#[ignore]
async fn codexion_state_now() {
    let api = api_from_persisted_session().await;
    let graph = api.project_data(21, 12, true).await.expect("graph");
    for entry in &graph {
        if entry
            .slug
            .as_deref()
            .is_some_and(|s| s.contains("codexion"))
        {
            println!(
                "live: {} state={:?} mark={:?}",
                entry.name.clone().unwrap_or_default(),
                entry.state,
                entry.final_mark
            );
        }
    }
}

fn write_verdict(error: &crate::api::ApiError) -> String {
    error.to_string()
}

/// Emergency cleanup: list and remove any open hours the probes left behind.
#[tokio::test]
#[ignore]
async fn cleanup_probe_hours() {
    let api = api_from_persisted_session().await;
    let anchor = chrono::Local::now().date_naive();
    let open = api.open_slots(anchor, 21).await.expect("open");
    for slot in &open {
        println!(
            "open hour: {} .. {} feed={}",
            slot.start.clone().unwrap_or_default(),
            slot.end.clone().unwrap_or_default(),
            slot.feed
        );
    }
    if open.is_empty() {
        println!("no stray hours");
        return;
    }
    let parse = |v: &str| {
        chrono::DateTime::parse_from_rfc3339(v)
            .unwrap()
            .with_timezone(&chrono::Local)
    };
    for slot in &open {
        let (Some(start), Some(end)) = (
            slot.start.as_deref().map(parse),
            slot.end.as_deref().map(parse),
        ) else {
            continue;
        };
        let result = api.delete_open_slot(start, end).await;
        println!("deleted [{start:?}..{end:?}]: {result:?}");
    }
}
/// Re-probe the booking rules against the live backend. Findings so far
/// (2026-08-19, own account, Codexion):
///
/// * opening an hour: quarter-aligned start required; +15 min lead is
///   403 "Invalid Time", +30 min (aligned) is accepted;
/// * booking: quarter-aligned and strictly more than 30 min ahead — the
///   quarter landing exactly on `now + 30` returns 404;
/// * booking freshly-created own hours is flaky (404) — the backend seems
///   to materialize project slots from corrector hours asynchronously;
/// * cancelling a booking has no working route we could find:
///   `DELETE api/project_slots/<id>` is 405 and `reserved-*` campuses are
///   400 "Invalid campus". Deleting the underlying hour does clear state.
///
/// Panic-free and always closes what it opened.
#[tokio::test]
#[ignore]
async fn probe_booking_rules() {
    use chrono::TimeZone as _;
    let api = api_from_persisted_session().await;
    let projects = api.slots_projects().await.expect("projects");
    let ps_id = projects.iter().find_map(|p| p.id).expect("project id");
    let now = chrono::Local::now();
    println!("now = {}", now.format("%H:%M:%S"));
    let boundary = |offset_min: i64| {
        let ts = (now.timestamp() / 900 + 1) * 900 + offset_min * 60;
        chrono::Local.timestamp_opt(ts, 0).single().unwrap()
    };
    let iso = |at: chrono::DateTime<chrono::Local>| {
        at.with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string()
    };

    // Opening lead: the first aligned start the backend accepts.
    for lead in [0i64, 15, 30, 45] {
        let begin = boundary(lead);
        let end = begin + chrono::Duration::hours(4);
        match api.create_open_slot(begin, end, "anr", false).await {
            Ok(()) => {
                println!("open  +{lead:>3} min: ACCEPTED (kept for booking probes)");
                break;
            }
            Err(error) => println!("open  +{lead:>3} min: rejected: {error}"),
        }
    }

    // Booking offsets, each with the backend's own verdict.
    for offset in [35i64, 45, 60, 90, 150, 210] {
        let target = boundary(offset);
        let ahead = (target - now).num_minutes();
        let verdict = api
            .book_project_slot(ps_id, &iso(target), "anr-local")
            .await
            .map(|_| "ACCEPTED".to_owned())
            .unwrap_or_else(|error| format!("rejected: {error}"));
        println!("book  +{offset:>3} min (~+{ahead:>3} from now): {verdict}");
    }

    // Cleanup: close every open hour on the account right now.
    let anchor = chrono::Local::now().date_naive();
    let open = api.open_slots(anchor, 21).await.expect("open hours");
    let parse = |v: &str| {
        chrono::DateTime::parse_from_rfc3339(v)
            .unwrap()
            .with_timezone(&chrono::Local)
    };
    for slot in &open {
        if let (Some(start), Some(end)) = (
            slot.start.as_deref().map(parse),
            slot.end.as_deref().map(parse),
        ) {
            println!(
                "cleanup delete [{start:?}..{end:?}]: {:?}",
                api.delete_open_slot(start, end).await
            );
        }
    }
    let reserved = api.reserved_slots(anchor, 21).await.expect("reserved");
    println!(
        "reservations after cleanup: {:?}",
        reserved
            .iter()
            .filter_map(|s| s.start.clone())
            .collect::<Vec<_>>()
    );
}
