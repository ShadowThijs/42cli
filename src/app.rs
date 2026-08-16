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

    pub fn index(self) -> usize {
        Tab::ORDER
            .iter()
            .position(|tab| *tab == self)
            .unwrap_or_default()
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
    pub notifications_open: bool,
    pub help_open: bool,
    pub status: Option<(String, Instant)>,
    pub tick: u64,
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
            notifications_open: false,
            help_open: false,
            status: None,
            tick: 0,
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
                self.send(Command::LoadProjects { fresh: false });
            }
            Tab::Clusters if !self.loaded.clusters => {
                self.loaded.clusters = true;
                self.send(Command::LoadClusters { fresh: false });
            }
            Tab::Slots if !self.loaded.slots => {
                self.loaded.slots = true;
                self.send(Command::LoadSlotsOverview);
            }
            _ => {}
        }
    }

    pub fn next_tab(&mut self) {
        let index = (self.tab.index() + 1) % Tab::ORDER.len();
        self.enter_tab(Tab::from_index(index));
    }

    pub fn prev_tab(&mut self) {
        let index = (self.tab.index() + Tab::ORDER.len() - 1) % Tab::ORDER.len();
        self.enter_tab(Tab::from_index(index));
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
    }
}
