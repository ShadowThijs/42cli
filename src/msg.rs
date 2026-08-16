//! Applying worker [`Msg`] answers to [`App`] state.

use crate::app::App;
use crate::bus::Msg;
use crate::state::Loadable;

impl App {
    pub fn on_msg(&mut self, msg: Msg) {
        match msg {
            // ---------------------------------------------------- auth ----
            Msg::Login(result) => {
                match result {
                    Ok(outcome) => {
                        self.login.state = Loadable::Ready(());
                        self.screen = crate::app::Screen::Main;
                        self.login.password.reset();
                        // Seed the summary so the header shows the identity
                        // before the real payload lands.
                        self.dash.summary = Loadable::Ready(crate::api::MeSummary {
                            id: Some(outcome.user_id),
                            login: Some(outcome.login.clone()),
                            ..Default::default()
                        });
                        self.set_status(format!("logged in as {}", outcome.login));
                        self.enter_tab(crate::app::Tab::Dashboard);
                        // Pre-cache the heavy tabs in the background.
                        self.send(crate::bus::Command::LoadProjects { fresh: false });
                        self.loaded.projects = true;
                    }
                    Err(error) => {
                        self.login.state = Loadable::Failed(error.to_string());
                        self.set_status(format!("login failed: {error}"));
                    }
                }
            }
            Msg::LoggedOut => {
                self.reset_after_logout();
                self.set_status("logged out");
            }
            Msg::SessionRestored { login, ok } => {
                if ok {
                    self.screen = crate::app::Screen::Main;
                    self.set_status(format!("welcome back, {login}"));
                    self.enter_tab(crate::app::Tab::Dashboard);
                } else {
                    self.login.state = Loadable::Failed("session expired — log in".into());
                    self.set_status("stored session expired");
                }
            }

            // ----------------------------------------------- dashboard ----
            Msg::MeSummary(result) => self.dash.summary.set(result),
            Msg::MyProfile(result) => self.dash.profile.set(result),
            Msg::MyCursus(result) => self.dash.cursus.set(result),
            Msg::MyCampus(result) => self.dash.campuses.set(result),
            Msg::MyEvents(result) => self.dash.events.set(result),
            Msg::MyNotifications(result) => self.dash.notifications.set(result),
            Msg::MyScaleTeams(result) => self.dash.scale_teams.set(result),
            Msg::MyLogtime(result) => self.dash.logtime.set(result),
            Msg::MyPace(result) => self.dash.pace.set(result),
            Msg::MyAttendance(result) => self.dash.attendance.set(result),
            Msg::MyAchievements(result) => self.dash.achievements.set(result),

            // ------------------------------------------------ projects ----
            Msg::ProjectData(result) => self.projects.graph.set(result),
            Msg::Ongoing(result) => self.projects.ongoing.set(result),
            Msg::Marked(result) => self.projects.marked.set(result),
            Msg::Mine { slug, result } => {
                let slot = self.projects.mine.entry(slug).or_default();
                slot.set(result);
            }
            Msg::DownloadDone { name, result } => {
                self.projects.downloading.remove(&name);
                match result {
                    Ok(path) => self.set_status(format!("saved {path}")),
                    Err(error) => self.set_status(format!("download failed: {error}")),
                }
            }

            // -------------------------------------------- search / user ----
            Msg::SearchResults(result) => {
                self.search.selection = 0;
                self.search.results.set(result);
            }
            Msg::UserView { login, result } => {
                if self.user.login == login {
                    self.user.profile.set(result);
                }
            }
            Msg::UserCursus { login, result } => {
                if self.user.login == login {
                    self.user.cursus.set(result);
                }
            }
            Msg::UserAchievements { login, result } => {
                if self.user.login == login {
                    self.user.achievements.set(result);
                }
            }
            Msg::UserLogtime { login, result } => {
                if self.user.login == login {
                    self.user.logtime.set(result);
                }
            }
            Msg::UserPatrons {
                login,
                patroning,
                patroned,
            } => {
                if self.user.login == login {
                    self.user.patroning.set(patroning);
                    self.user.patroned.set(patroned);
                }
            }

            // ----------------------------------------------- clusters ----
            Msg::Clusters(result) => {
                self.clusters.cluster_sel = 0;
                self.clusters.seats.set(result);
            }

            // -------------------------------------------------- slots ----
            Msg::SlotsProjects(result) => {
                self.slots.project_sel = 0;
                self.slots.slot_sel = 0;
                self.slots.projects.set(result);
            }
            Msg::SlotsSynced(result) => match result {
                Ok(()) => self.set_status("projects synced"),
                Err(error) => self.set_status(format!("sync failed: {error}")),
            },
            Msg::OpenSlots(result) => {
                self.slots.open_sel = 0;
                self.slots.open.set(result);
            }
            Msg::ReservedSlots(result) => {
                self.slots.reserved_sel = 0;
                self.slots.reserved.set(result);
            }
            Msg::ProjectSlots(result) => {
                self.slots.slot_sel = 0;
                self.slots.project_slots.set(result);
            }
            Msg::SlotWrite(result) => match result {
                Ok(()) => self.set_status("done"),
                Err(error) => self.set_status(format!("slots: {error}")),
            },
        }
    }

    /// Mark every slot-related list as stale (used when switching modes).
    pub fn slots_reload(&mut self) {
        self.send(crate::bus::Command::LoadSlotsOverview);
    }
}
