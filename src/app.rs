//! Top-level application state and tab routing.

use std::time::Instant;

use crate::bus::{Command, CommandSink};
use crate::state::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Login,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Projects,
    Slots,
    Search,
    User,
    Clusters,
}

impl Tab {
    pub const ORDER: [Tab; 6] = [
        Tab::Dashboard,
        Tab::Projects,
        Tab::Slots,
        Tab::Search,
        Tab::User,
        Tab::Clusters,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Projects => "Projects",
            Tab::Slots => "Slots",
            Tab::Search => "Search",
            Tab::User => "User",
            Tab::Clusters => "Clusters",
        }
    }

    pub fn from_index(index: usize) -> Tab {
        Tab::ORDER[index.min(Tab::ORDER.len() - 1)]
    }

    pub fn index_of(self) -> usize {
        Tab::ORDER
            .iter()
            .position(|tab| *tab == self)
            .unwrap_or_default()
    }

    /// Neighbour tab with wrap-around, for Ctrl+Left / Ctrl+Right.
    pub fn step(self, delta: i32) -> Tab {
        let len = Tab::ORDER.len() as i32;
        let index = (self.index_of() as i32 + delta).rem_euclid(len) as usize;
        Tab::ORDER[index]
    }
}

pub struct App {
    pub screen: Screen,
    pub tab: Tab,
    pub login: LoginState,
    pub dash: Dashboard,
    pub projects: ProjectsState,
    pub slots: SlotsState,
    pub search: SearchState,
    pub user: UserView,
    pub clusters: ClustersState,
    pub subject_view: Option<crate::state::SubjectView>,
    pub notifications_open: bool,
    pub notifications_sel: usize,
    pub event_popup: Option<crate::state::EventPopup>,
    pub help_open: bool,
    pub status: Option<(String, Instant)>,
    pub tick: u64,
    /// Editor to launch once the event loop can suspend the TUI, with the
    /// folder it should open (`<editor> .` from that directory).
    pub pending_editor: Option<(String, String)>,
    /// Available update tag, shown as a banner.
    pub update_available: Option<String>,
    cmd: CommandSink,
    /// Guards so background preloads fire at most once per session tab.
    pub loaded: LoadedFlags,
}

#[derive(Debug, Default)]
pub struct LoadedFlags {
    pub projects: bool,
    pub clusters: bool,
    pub slots: bool,
}

impl App {
    pub fn new(cmd: CommandSink) -> Self {
        Self {
            screen: Screen::Login,
            tab: Tab::Dashboard,
            login: LoginState::default(),
            dash: Dashboard::default(),
            projects: ProjectsState::default(),
            slots: SlotsState::default(),
            search: SearchState::default(),
            user: UserView::default(),
            clusters: ClustersState::default(),
            subject_view: None,
            notifications_open: false,
            notifications_sel: 0,
            event_popup: None,
            help_open: false,
            status: None,
            tick: 0,
            pending_editor: None,
            update_available: None,
            cmd,
            loaded: LoadedFlags::default(),
        }
    }

    pub fn send(&self, command: Command) {
        let _ = self.cmd.send(command);
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status.as_ref().map(|(message, at)| {
            if at.elapsed().as_secs() > 6 {
                ""
            } else {
                message.as_str()
            }
        })
    }

    /// Enter a tab, lazily triggering its data load exactly once.
    pub fn enter_tab(&mut self, tab: Tab) {
        self.tab = tab;
        match tab {
            Tab::Projects if !self.loaded.projects => {
                self.loaded.projects = true;
                self.projects.graph.start();
                self.projects.ongoing.start();
                self.projects.marked.start();
                self.send(Command::LoadProjects { fresh: false });
            }
            Tab::Clusters if !self.loaded.clusters => {
                self.loaded.clusters = true;
                self.clusters.seats.start();
                self.send(Command::LoadClusters { fresh: false });
            }
            Tab::Slots if !self.loaded.slots => {
                self.loaded.slots = true;
                self.slots.projects.start();
                self.slots.open.start();
                self.slots.reserved.start();
                self.send(Command::LoadSlotsOverview {
                    anchor: self.slots.week_anchor,
                });
            }
            _ => {}
        }
    }

    /// Open another student's profile (from search).
    pub fn open_user(&mut self, login: &str) {
        if login.is_empty() {
            return;
        }
        self.user = UserView {
            login: login.to_owned(),
            ..Default::default()
        };
        self.send(Command::LoadUser {
            login: login.to_owned(),
        });
        self.enter_tab(Tab::User);
    }

    pub fn reset_after_logout(&mut self) {
        self.screen = Screen::Login;
        self.tab = Tab::Dashboard;
        self.login = LoginState::default();
        self.dash = Dashboard::default();
        self.projects = ProjectsState::default();
        self.slots = SlotsState::default();
        self.search = SearchState::default();
        self.user = UserView::default();
        self.clusters = ClustersState::default();
        self.loaded = LoadedFlags::default();
        self.notifications_open = false;
        self.notifications_sel = 0;
        self.event_popup = None;
        self.subject_view = None;
        self.update_available = None;
    }

    /// Fill dashboard/projects from stale disk cache without network.
    /// Called optimistically on startup when a vault exists, so the UI
    /// renders instantly and is corrected after verification.
    pub fn hydrate_from_cache(&mut self) {
        use crate::cache::DiskCache;
        let cache = DiskCache::new();

        // Me summary is the identity header — keep it even if stale.
        if let Some(summary) = cache.get_stale::<crate::api::MeSummary>("me/summary") {
            let login = summary.login.clone();
            self.dash.summary = crate::state::Loadable::Ready(summary);
            // Profile, cursus, campus, achievements etc keyed by login/id
            if let Some(login) = login
                && let Some(profile) =
                    cache.get_stale::<crate::api::UserProfile>(&format!("users/{login}"))
            {
                // keep wallet, eval points, location etc optimistically
                self.dash.profile = crate::state::Loadable::Ready(profile);
                // If profile has id, try to load id-based caches
                if let Some(id) = self.dash.profile.data().and_then(|p| p.id) {
                    if let Some(cursus) =
                        cache.get_stale::<Vec<crate::api::Cursus>>(&format!("users/{id}/cursus"))
                    {
                        self.dash.cursus = crate::state::Loadable::Ready(cursus);
                    }
                    if let Some(campus) =
                        cache.get_stale::<Vec<crate::api::Campus>>(&format!("users/{id}/campus"))
                    {
                        self.dash.campuses = crate::state::Loadable::Ready(campus);
                    }
                    if let Some(ach) = cache.get_stale::<Vec<crate::api::Achievement>>(&format!(
                        "users/{id}/achievements"
                    )) {
                        self.dash.achievements = crate::state::Loadable::Ready(ach);
                    }
                    if let Some(cursus) = self.dash.cursus.data().and_then(|list| {
                        list.iter()
                            .find(|c| c.slug.as_deref() == Some("42cursus"))
                            .or_else(|| list.first())
                    }) {
                        let cursus_id = cursus.id.unwrap_or(21);
                        if let Some(campus_id) = self
                            .dash
                            .campuses
                            .data()
                            .and_then(|cs| cs.iter().find(|c| c.is_primary).and_then(|c| c.id))
                        {
                            let key = format!("project_data/{cursus_id}/{campus_id}");
                            if let Some(graph) =
                                cache.get_stale::<Vec<crate::api::ProjectDataEntry>>(&key)
                            {
                                self.projects.graph = crate::state::Loadable::Ready(graph);
                                self.loaded.projects = true;
                            }
                        }
                    }
                }
            }
        }
        if let Some(events) = cache.get_stale::<Vec<crate::api::IntraEvent>>("me/events") {
            // Filter to future events only for optimistic display? Keep as cached but
            // upcoming events are those after now — we keep what we have.
            let now = chrono::Local::now();
            let filtered: Vec<_> = events
                .into_iter()
                .filter(|e| {
                    e.begin_at
                        .as_deref()
                        .and_then(crate::util::parse_datetime)
                        .map(|at| at > now)
                        .unwrap_or(true)
                })
                .collect();
            // Keep wallet/grade etc: they are inside profile/cursus already
            self.dash.events = crate::state::Loadable::Ready(filtered);
        }
        if let Some(logtime) = cache.get_stale::<crate::api::LocationStats>(&format!(
            "logtime/{}",
            self.dash
                .summary
                .data()
                .and_then(|s| s.login.clone())
                .unwrap_or_default()
        )) && !logtime.is_empty()
        {
            self.dash.logtime = crate::state::Loadable::Ready(logtime);
        }
        // Ongoing / marked / clusters and other volatile data stay Loading
        // and will be fetched; skeleton UI will show.
    }
}
