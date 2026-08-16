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
    pub notifications: Loadable<Vec<Notification>>,
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
    pub downloading: HashMap<String, bool>,
    pub focus_details: bool,
}

// ------------------------------------------------------------- slots ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotsMode {
    Overview,
    Hours,
}

#[derive(Debug, Default)]
pub struct SlotForm {
    pub date: Input,
    pub start: Input,
    pub end: Input,
    pub campus_bx: bool,
    pub remote: bool,
    pub focus: u8,
}

#[derive(Debug, Default)]
pub struct SlotsState {
    pub mode: Option<SlotsMode>,
    pub open: Loadable<Vec<Slot>>,
    pub reserved: Loadable<Vec<Slot>>,
    pub projects: Loadable<Vec<SlotsProject>>,
    pub project_slots: Loadable<Vec<Slot>>,
    pub project_sel: usize,
    pub slot_sel: usize,
    pub open_sel: usize,
    pub reserved_sel: usize,
    pub form: SlotForm,
    pub focus: SlotsFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotsFocus {
    #[default]
    List,
    Form,
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

/// One aggregated cluster row.
#[derive(Debug, Clone)]
pub struct ClusterRow {
    pub name: String,
    pub seats: Vec<ClusterSeat>,
}

impl ClustersState {
    /// Group occupied seats by cluster prefix (`fu-r2-p7` -> `fu`).
    pub fn rows(&self) -> Vec<ClusterRow> {
        let mut clusters: HashMap<String, Vec<ClusterSeat>> = HashMap::new();
        if let Some(seats) = self.seats.data() {
            for seat in seats.clone() {
                let host = seat.host.clone().unwrap_or_default();
                let cluster = host
                    .split('-')
                    .next()
                    .filter(|prefix| !prefix.is_empty())
                    .unwrap_or("other")
                    .to_owned();
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
