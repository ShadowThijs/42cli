//! Per-screen mutable state (rendering lives in `ui/`).

use std::collections::HashMap;

use tui_input::Input;

use crate::api::models::*;

/// Async slot state for any piece of UI data.
#[derive(Debug, Clone, Default)]
pub enum Loadable<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn set(&mut self, result: Result<T, crate::api::ApiError>) {
        *self = match result {
            Ok(value) => Loadable::Ready(value),
            Err(error) => Loadable::Failed(error.to_string()),
        };
    }

    pub fn start(&mut self) {
        if !matches!(self, Loadable::Loading) {
            *self = Loadable::Loading;
        }
    }

    pub fn data(&self) -> Option<&T> {
        match self {
            Loadable::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn failed(&self) -> Option<&str> {
        match self {
            Loadable::Failed(message) => Some(message),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Loadable::Loading)
    }
}

// ------------------------------------------------------------- login ----

#[derive(Debug, Default)]
pub struct LoginState {
    pub username: Input,
    pub password: Input,
    pub focus: u8,
    pub state: Loadable<()>,
}

// --------------------------------------------------------- dashboard ----

#[derive(Debug, Default)]
pub struct Dashboard {
    pub summary: Loadable<MeSummary>,
    pub profile: Loadable<UserProfile>,
    pub cursus: Loadable<Vec<Cursus>>,
    pub campuses: Loadable<Vec<Campus>>,
    pub events: Loadable<Vec<IntraEvent>>,
    pub notifications: Loadable<NotificationsPayload>,
    pub scale_teams: Loadable<Vec<ScaleTeam>>,
    pub logtime: Loadable<LocationStats>,
    pub pace: Loadable<PaceProfile>,
    pub attendance: Loadable<AttendanceSummary>,
    pub achievements: Loadable<Vec<Achievement>>,
}

impl Dashboard {
    /// The main cursus ("42cursus", falling back to the first entry).
    pub fn main_cursus(&self) -> Option<&Cursus> {
        self.cursus.data().and_then(|list| {
            list.iter()
                .find(|cursus| cursus.slug.as_deref() == Some("42cursus"))
                .or_else(|| list.first())
        })
    }
}

// ---------------------------------------------------------- projects ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSegment {
    Active,
    Available,
    Done,
    All,
}

impl ProjectSegment {
    pub const ALL: [ProjectSegment; 4] = [
        ProjectSegment::Active,
        ProjectSegment::Available,
        ProjectSegment::Done,
        ProjectSegment::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ProjectSegment::Active => "Active",
            ProjectSegment::Available => "Available",
            ProjectSegment::Done => "Done",
            ProjectSegment::All => "All",
        }
    }
}

#[derive(Debug, Default)]
pub struct ProjectsState {
    pub filter: Input,
    /// Set while the filter box captures keystrokes (toggled with `/`).
    pub filter_focused: bool,
    pub segment: Option<ProjectSegment>,
    pub selection: usize,
    pub attachment_sel: usize,
    pub graph: Loadable<Vec<ProjectDataEntry>>,
    pub ongoing: Loadable<Vec<OngoingProject>>,
    pub marked: Loadable<Vec<MarkedProject>>,
    pub mine: HashMap<String, Loadable<ProjectMine>>,
    /// Evaluation schedule per slug (`/{slug}/scale_teams`).
    pub schedule: HashMap<String, Loadable<Vec<ProjectScheduleEntry>>>,
    pub downloading: HashMap<String, bool>,
    /// Slugs with a `git clone` in flight (triggered with `g`).
    pub cloning: Vec<String>,
    /// Destination + folder-name prompt opened with `g`.
    pub clone_prompt: Option<ClonePrompt>,
    /// Folder of a just-finished clone, offered to an editor.
    pub editor_prompt: Option<String>,
    pub focus_details: bool,
    /// Slug to select once the graph data lands (notification jump made
    /// before the projects tab ever loaded).
    pub pending_focus: Option<String>,
}

/// The `g` popup: where should the project's git repo be cloned, and under
/// what folder name.
#[derive(Debug)]
pub struct ClonePrompt {
    pub slug: String,
    pub repo: String,
    pub dest: Input,
    pub name: Input,
    pub focus: u8,
    /// Path candidates for the destination (Tab completion).
    pub completions: Option<String>,
}

// ------------------------------------------------------------- slots ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotsMode {
    Overview,
    Hours,
}

#[derive(Debug)]
pub struct SlotsState {
    pub mode: Option<SlotsMode>,
    pub open: Loadable<Vec<Slot>>,
    pub reserved: Loadable<Vec<Slot>>,
    pub projects: Loadable<Vec<SlotsProject>>,
    pub project_slots: Loadable<Vec<Slot>>,
    pub project_sel: usize,
    pub focus: SlotsFocus,
    /// Any date inside the week the calendar shows (snapped to its Monday).
    pub week_anchor: chrono::NaiveDate,
    /// Calendar cursor: (weekday 0..6, half-hour row).
    pub cursor: (usize, usize),
    /// First cell of an open-hour range being dragged out.
    pub range_start: Option<(usize, usize)>,
    /// New open hours go to the toggled campus; `t` flips inter-campus.
    pub campus_bx: bool,
    pub remote: bool,
    /// First visible half-hour row. The calendar measures its viewport
    /// while rendering (draw only sees `&App`), so both scroll fields are
    /// plain cells the UI thread mutates single-handedly.
    pub grid_scroll: std::cell::Cell<u16>,
    /// Visible row count of the last render, for key-side scrolling.
    pub grid_view: std::cell::Cell<u16>,
}

impl Default for SlotsState {
    fn default() -> Self {
        Self {
            mode: None,
            open: Loadable::default(),
            reserved: Loadable::default(),
            projects: Loadable::default(),
            project_slots: Loadable::default(),
            project_sel: 0,
            focus: SlotsFocus::Strip,
            week_anchor: chrono::Local::now().date_naive(),
            cursor: (0, 0),
            range_start: None,
            campus_bx: false,
            remote: false,
            grid_scroll: std::cell::Cell::new(0),
            grid_view: std::cell::Cell::new(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotsFocus {
    /// The horizontal bookable-projects strip.
    Strip,
    /// The week calendar itself.
    #[default]
    Grid,
}

impl SlotsState {
    pub fn selected_project(&self) -> Option<&SlotsProject> {
        self.projects
            .data()
            .and_then(|projects| projects.get(self.project_sel))
    }
}

// ------------------------------------------------------------ search ----

#[derive(Debug, Default)]
pub struct SearchState {
    pub input: Input,
    pub results: Loadable<Vec<SearchResult>>,
    pub selection: usize,
    pub last_query: String,
}

#[derive(Debug, Default)]
pub struct UserView {
    pub login: String,
    pub profile: Loadable<Box<UserProfile>>,
    pub cursus: Loadable<Vec<Cursus>>,
    pub achievements: Loadable<Vec<Achievement>>,
    pub logtime: Loadable<LocationStats>,
    pub patroning: Loadable<Vec<PatronUser>>,
    pub patroned: Loadable<Vec<PatronUser>>,
}

// ---------------------------------------------------------- clusters ----

#[derive(Debug, Default)]
pub struct ClustersState {
    pub seats: Loadable<Vec<ClusterSeat>>,
    pub cluster_sel: usize,
}

// ------------------------------------------------------ notifications ----

/// Event detail popup opened from a notification link; it replaces the
/// notifications list while open.
#[derive(Debug)]
pub struct EventPopup {
    pub event_id: u32,
    pub event: Loadable<EventDetail>,
}

/// One aggregated cluster row.
#[derive(Debug, Clone)]
pub struct ClusterRow {
    pub name: String,
    pub seats: Vec<ClusterSeat>,
}

impl ClustersState {
    /// Group occupied seats of one campus by cluster prefix
    /// (`fu-r2-p7` -> `fu`, `wifi-5` -> `wifi`).
    pub fn rows(&self, campus_id: Option<u32>) -> Vec<ClusterRow> {
        let mut clusters: HashMap<String, Vec<ClusterSeat>> = HashMap::new();
        if let Some(seats) = self.seats.data() {
            for seat in seats.clone() {
                if let Some(campus) = campus_id
                    && seat.campus_id != Some(campus)
                {
                    continue;
                }
                let host = seat.host.clone().unwrap_or_default();
                let cluster = host
                    .split('-')
                    .next()
                    .filter(|prefix| !prefix.is_empty())
                    .unwrap_or("other")
                    .to_owned();
                let cluster = cluster_display_name(seat.campus_id, &cluster);
                clusters.entry(cluster).or_default().push(seat);
            }
        }
        let mut rows: Vec<ClusterRow> = clusters
            .into_iter()
            .map(|(name, mut seats)| {
                seats.sort_by(|a, b| a.host.cmp(&b.host));
                ClusterRow { name, seats }
            })
            .collect();
        rows.sort_by(|a, b| b.seats.len().cmp(&a.seats.len()).then(a.name.cmp(&b.name)));
        rows
    }
}

/// Translate a hostname prefix into the cluster name used on campus.
///
/// Brussels (campus 12) hostnames are off by one versus the physical
/// signage: the rooms signed a1/a2 carry machines named a2-*/a3-*.
fn cluster_display_name(campus_id: Option<u32>, prefix: &str) -> String {
    if campus_id == Some(12) {
        match prefix {
            "a2" => "a1".to_owned(),
            "a3" => "a2".to_owned(),
            other => other.to_owned(),
        }
    } else {
        prefix.to_owned()
    }
}
