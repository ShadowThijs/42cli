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

impl Cursus {
    /// Piscine cursus (e.g. `c-piscine`, cursus id 65): its students have
    /// no pace-system profile — requests 404.
    pub fn is_piscine(&self) -> bool {
        self.slug
            .as_deref()
            .is_some_and(|slug| slug.contains("piscine"))
            || self
                .name
                .as_deref()
                .is_some_and(|name| name.to_lowercase().contains("piscine"))
    }
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
    #[serde(default, alias = "is_validated")]
    pub validated: Option<bool>,
    pub marked_at: Option<String>,
    pub occurrence: Option<u32>,
    /// Flat fields used by `intrapy` for other-user marked (top-level
    /// `project_name`/`project_slug` instead of nested `project`).
    pub project_name: Option<String>,
    pub project_slug: Option<String>,
    /// Raw payload kept for the detail pane.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl MarkedProject {
    pub fn display_name(&self) -> Option<&str> {
        self.project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .or(self.project_name.as_deref())
            .or_else(|| self.extra.get("project_name").and_then(|v| v.as_str()))
    }
    pub fn display_slug(&self) -> Option<&str> {
        self.project
            .as_ref()
            .and_then(|p| p.slug.as_deref())
            .or(self.project_slug.as_deref())
            .or_else(|| self.extra.get("project_slug").and_then(|v| v.as_str()))
    }
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

impl ScaleTeam {
    /// Project under evaluation. The v2 `/users/me/scale_teams` payload
    /// nests it as `team.project.name`, intrapy flattens it to
    /// `project_name` — both survive into `extra`.
    pub fn project_name(&self) -> Option<&str> {
        self.extra
            .get("team")
            .and_then(|team| team.get("project"))
            .and_then(|project| project.get("name"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| self.extra.get("project_name")?.as_str())
    }

    /// Logins of the students under evaluation. The v2 payload nests them
    /// in `correcteds[]`; intrapy flattens a single one into `user` (the
    /// caller drops entries whose `kind` is not "evaluator" — there a
    /// non-evaluator `user` names the corrector, i.e. me).
    pub fn corrected_logins(&self) -> Vec<String> {
        let nested: Vec<String> = self
            .correcteds
            .iter()
            .filter_map(|corrected| corrected.login.clone())
            .collect();
        if !nested.is_empty() {
            return nested;
        }
        self.extra
            .get("user")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .into_iter()
            .collect()
    }

    /// True while the correction still expects a grid: the window may
    /// already be over (intrapy keeps `can_report` true until the report
    /// lands), but no mark was recorded.
    pub fn needs_report(&self) -> bool {
        self.final_mark.is_none()
            && self
                .extra
                .get("can_report")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
    }
}

// --------------------------------------------------------- evaluations ----

/// A scraped evaluation form (`GET /scale_teams/{id}/edit`): everything
/// needed to render and later submit the correction grid.
#[derive(Debug, Clone, Default)]
pub struct EvalForm {
    /// POST target (the Rails form `action`).
    pub action: String,
    pub authenticity_token: String,
    pub project: String,
    /// Logins of the evaluated student(s).
    pub correcteds: Vec<String>,
    /// Vogsphere repository of the team under evaluation.
    pub repo: Option<String>,
    pub sections: Vec<EvalSection>,
    /// Every flag the scale offers, page order (positive first).
    pub flags: Vec<EvalFlag>,
    /// Flag preselected on the page — almost always `Ok`.
    pub default_flag: Option<u32>,
}

impl EvalForm {
    /// Flat question list in page order — the selection indexing used by
    /// the evaluation view.
    pub fn questions(&self) -> Vec<&EvalQuestion> {
        self.sections
            .iter()
            .flat_map(|section| section.questions.iter())
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct EvalSection {
    pub title: String,
    pub questions: Vec<EvalQuestion>,
}

#[derive(Debug, Clone, Default)]
pub struct EvalQuestion {
    pub question_id: u64,
    pub name: String,
    /// Requirement text under the question name (HTML stripped).
    pub guidelines: String,
    /// Server id of an existing answer when the form was already
    /// partially submitted — sent back unchanged on submission.
    pub answer_id: Option<String>,
    pub kind: QuestionKind,
}

/// How a question is answered and rendered. The value submitted is
/// always the exact string the original form input would send.
#[derive(Debug, Clone, PartialEq)]
pub enum QuestionKind {
    /// Yes/No radio pair — submits `"1"` / `"0"`.
    Boolean {
        /// The page's preselected answer (default `No`).
        yes: bool,
    },
    /// Radios with more options than plain yes/no.
    Choice { options: Vec<EvalChoice> },
    /// Slider — a number in `min..=max` moved in `step`s. Sliders are
    /// grouped per skill (`data-slider-id` + `data-kind`); the combined
    /// total of a group is capped by its kind (standard ≤ 100, bonus ≤ 25)
    /// — moving one lowers the ceiling of its siblings.
    Range {
        min: i64,
        max: i64,
        step: i64,
        value: i64,
        skill: Option<String>,
        kind: Option<String>,
    },
}

impl Default for QuestionKind {
    fn default() -> Self {
        QuestionKind::Boolean { yes: false }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvalChoice {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

/// One flag of the ratings row above the comment box.
#[derive(Debug, Clone, Default)]
pub struct EvalFlag {
    pub id: u32,
    /// Page label, e.g. `Outstanding project`, `Cheat`, `Crash`.
    pub name: String,
    /// Ok / Outstanding project grade normally; negative flags end the
    /// grading with a 0 (cheating: -42).
    pub positive: bool,
}

/// One answer ready for submission.
#[derive(Debug, Clone, Default)]
pub struct EvalAnswer {
    pub question_id: u64,
    /// Exactly what the original form input would send (`"1"`, `"0"`,
    /// a slider number, a choice value).
    pub value: String,
    /// Existing answer id, when the form carried one.
    pub answer_id: Option<String>,
}

/// Everything a finished evaluation submits.
#[derive(Debug, Clone, Default)]
pub struct EvalSubmission {
    pub scale_team_id: u64,
    pub action: String,
    pub authenticity_token: String,
    pub answers: Vec<EvalAnswer>,
    pub flag_id: u32,
    pub comment: String,
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
    /// Actual teammates — the `.team-users-list` users, *not* the
    /// evaluators that also live under `.team-content`.
    #[serde(default)]
    pub members: Vec<String>,
    /// One entry per evaluation attempt shown on the page.
    #[serde(default)]
    pub evaluations: Vec<ProjectEvaluation>,
    pub git_repo: Option<String>,
    pub locked_at: Option<String>,
    pub deadline: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub unsub_url: Option<String>,
    /// "Finish / close" form of the team (`PUT .../teams/{id}/close`).
    #[serde(default)]
    pub close_action: Option<TeamAction>,
    /// "Give up" form of the team (`PUT .../teams/{id}/give_up`).
    #[serde(default)]
    pub give_up_action: Option<TeamAction>,
}

/// A Rails `PUT` form of the mine page: absolute target + the form's
/// own `authenticity_token` (the page issues one per form).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamAction {
    pub url: String,
    pub token: String,
}

/// Which team action to replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamActionKind {
    Close,
    GiveUp,
}

impl TeamActionKind {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Close => "finish",
            Self::GiveUp => "give up",
        }
    }
}

/// One evaluation attempt: who corrected, with which result badge
/// (`125%`, `success`, `fail`, …) and their closing comment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectEvaluation {
    #[serde(default)]
    pub correctors: Vec<String>,
    pub result: Option<String>,
    pub comment: Option<String>,
    /// When the defense took place (`data-long-date`, RFC 3339).
    pub evaluated_at: Option<String>,
    /// Flag reason when the attempt was flagged (`empty_work`, `cheat`, …).
    pub flag_reason: Option<String>,
    /// Feedback form URL while feedback on this correction is still pending.
    pub feedback_url: Option<String>,
    /// Scale-team id (parsed from the feedback URL) — same id the
    /// correction grid at `/scale_teams/{id}/edit` lives under.
    pub scale_team_id: Option<u64>,
}

/// One row of a project's evaluation schedule (`/{slug}/scale_teams`):
/// who corrects whom, when, and the outcome once it happened.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectScheduleEntry {
    pub corrector: Option<String>,
    /// Corrected login (team entries render as `login's group` on the page;
    /// we keep the bare login from the tooltip attribute).
    pub corrected: Option<String>,
    /// Raw scheduled time as printed, e.g. `August 19, 2026 21:45`.
    pub scheduled_at: Option<String>,
    /// Final mark once graded (`100 %`); `None` while still upcoming.
    pub result: Option<String>,
    /// Corrector's closing comment.
    pub comment: Option<String>,
    /// Corrected's feedback summary tooltip (`Nice: 4 / 4; …`).
    pub feedback: Option<String>,
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
