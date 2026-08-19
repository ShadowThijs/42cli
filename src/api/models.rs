//! Serde models for every remote endpoint we consume.
//!
//! Fields are deliberately `Option`-heavy: 42's payloads vary between
//! endpoints and campuses, and a missing field must never crash the TUI.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------- auth ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: i64,
}

// ------------------------------------------------------------ intrapy ----

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeSummary {
    pub id: Option<u32>,
    pub login: Option<String>,
    pub profile_picture: Option<String>,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Group {
    pub group_id: Option<u32>,
    pub name: Option<String>,
    pub color: Option<String>,
}

/// `GET /api/v1/users/{login}` — the full user record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Option<u32>,
    pub login: Option<String>,
    pub displayed_login: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub location: Option<String>,
    pub wallet: Option<i64>,
    pub evaluation_points: Option<i64>,
    pub is_active: Option<bool>,
    pub alumnized_at: Option<String>,
    pub data_erasure_date: Option<String>,
    pub profile_picture: Option<String>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub titles: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cursus {
    pub id: Option<u32>,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub grade: Option<String>,
    pub level: Option<f64>,
    pub progress: Option<u32>,
    pub blackholed_at: Option<String>,
    pub freeze_until: Option<String>,
    #[serde(default)]
    pub can_give_points: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Campus {
    pub id: Option<u32>,
    pub name: Option<String>,
    pub time_zone: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OngoingProject {
    pub project_name: Option<String>,
    pub project_slug: Option<String>,
}

/// A graded project from `/users/{login}/projects/marked`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarkedProject {
    #[serde(default)]
    pub project: Option<ProjectRef>,
    pub status: Option<String>,
    pub final_mark: Option<u32>,
    #[serde(default)]
    pub validated: Option<bool>,
    pub marked_at: Option<String>,
    pub occurrence: Option<u32>,
    /// Raw payload kept for the detail pane.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectRef {
    pub id: Option<u32>,
    pub name: Option<String>,
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntraEvent {
    pub id: Option<u32>,
    pub name: Option<String>,
    pub begin_at: Option<String>,
    pub end_at: Option<String>,
    pub description: Option<String>,
    pub kind: Option<String>,
    pub location: Option<String>,
    pub max_subscribers: Option<u32>,
    pub current_subscribers: Option<u32>,
    #[serde(default)]
    pub is_subscribed: bool,
    #[serde(default)]
    pub is_waitlisted: bool,
}

/// Event page scraped from `profile.intra.42.fr/events/{id}` — richer than
/// the list payload (full description, sign-up count, subscribe state) and
/// available for past events too.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventDetail {
    pub id: u32,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub begin_at: Option<String>,
    pub end_at: Option<String>,
    /// Human duration as printed on the page ("6 days").
    pub duration: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub current_subscribers: Option<u32>,
    pub max_subscribers: Option<u32>,
    #[serde(default)]
    pub is_subscribed: bool,
    /// CSRF token of the scraped page, needed to submit the rails-ujs
    /// subscribe / unsubscribe actions.
    pub csrf_token: Option<String>,
    /// Absolute subscribe (POST) and unsubscribe (DELETE-override) endpoints
    /// from the page footer; absent when the event does not allow the
    /// action (past, full, closed, …).
    pub subscribe_url: Option<String>,
    pub unsubscribe_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Notification {
    pub title: Option<String>,
    pub text: Option<String>,
    pub created_at: Option<String>,
    pub link: Option<String>,
}

/// Unread notifications come wrapped in `{count, notifications}` while the
/// read feed is a bare array; normalize into one display payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationsPayload {
    pub unread: usize,
    pub items: Vec<Notification>,
}

/// Evaluation slots the user must give (`/users/me/scale_teams`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScaleTeam {
    pub id: Option<u64>,
    pub scale_id: Option<u64>,
    pub begin_at: Option<String>,
    pub duration: Option<f64>,
    pub final_mark: Option<u32>,
    pub comment: Option<String>,
    pub feedback: Option<String>,
    #[serde(default)]
    pub correcteds: Vec<Corrected>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Corrected {
    pub login: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Achievement {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tier: Option<String>,
    pub kind: Option<String>,
    pub nbr_of_success: Option<u64>,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatronUser {
    pub id: Option<u32>,
    pub login: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

// --------------------------------------------------------- auxiliary ----

/// `translate` logtime stats: `{"YYYY-MM-DD": "HH:MM:SS"}`.
pub type LocationStats = HashMap<String, String>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttendanceSummary {
    #[serde(default)]
    pub weeks: Vec<AttendanceWeek>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttendanceWeek {
    pub at: Option<String>,
    /// ISO 8601 duration, e.g. `PT26H30M`.
    pub total: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaceProfile {
    #[serde(default)]
    pub is_activated: bool,
    pub activated_at: Option<String>,
    pub milestone: Option<u32>,
    pub deadline: Option<String>,
    pub cursus_begin_date: Option<String>,
    pub eta_end_of_cursus: Option<String>,
    pub pace: Option<u32>,
    #[serde(default)]
    pub milestones: Vec<PaceMilestone>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaceMilestone {
    pub level: Option<u32>,
    pub deadline: Option<String>,
    pub validated_at: Option<String>,
}

// ----------------------------------------------------------- intra web ----

/// One node of `project_data.json` (the holy graph data).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectDataEntry {
    pub state: Option<String>,
    pub final_mark: Option<u32>,
    pub id: Option<u32>,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub project_id: Option<u32>,
    pub difficulty: Option<u32>,
    pub duration: Option<String>,
    pub rules: Option<String>,
    pub description: Option<String>,
    /// Incoming graph edges (parent project + bezier control points).
    #[serde(default)]
    pub by: Vec<ProjectEdge>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectEdge {
    pub parent_id: Option<u32>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResult {
    pub login: Option<String>,
    pub cdn_uri: Option<String>,
}

/// One occupied seat from `meta.intra.42.fr/clusters.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterSeat {
    pub host: Option<String>,
    pub begin_at: Option<String>,
    pub end_at: Option<String>,
    pub login: Option<String>,
    pub campus_id: Option<u32>,
}

/// Team / attachment info scraped from `projects.intra.42.fr/{slug}/mine`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMine {
    pub status: Option<String>,
    pub team_name: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
    pub git_repo: Option<String>,
    pub locked_at: Option<String>,
    pub deadline: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub unsub_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub url: String,
}

// --------------------------------------------------------------- slots ----

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlotsProject {
    pub id: Option<u32>,
    pub name: Option<String>,
}

/// A slot block as returned by the slots API (open hours and project slots).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Slot {
    pub id: Option<u64>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub campus: Option<String>,
    #[serde(default)]
    pub remote: bool,
    /// Set locally for slots coming from a `reserved-*` feed.
    #[serde(default)]
    pub reserved: bool,
    /// Feed name the slot was fetched from (`bx`, `remote-bx`, …) — the
    /// value the booking endpoints expect as `campus`.
    #[serde(default)]
    pub feed: String,
}
