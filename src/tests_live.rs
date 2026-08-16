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
    let _open = api.open_slots(21).await.expect("open slots");
    let _reserved = api.reserved_slots(21).await.expect("reserved slots");
    let projects = api.slots_projects().await.expect("slots projects");
    for project in projects.iter().take(1) {
        if let Some(ps_id) = project.id {
            let _slots = api.project_slots(ps_id, 21).await.expect("project slots");
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

