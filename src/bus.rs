//! Bridge between the synchronous TUI loop and the async API world.
//!
//! The UI never awaits: it fires [`Command`]s at the worker thread and
//! consumes [`Msg`] answers on every frame, so HTTP never blocks rendering.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use crate::api::Api;
use crate::api::auth::LoginOutcome;
use crate::api::error::ApiError;
use crate::api::models::*;
use crate::config::StoredSession;

/// Everything the UI can ask the worker to do.
#[derive(Debug, Clone)]
pub enum Command {
    Login {
        username: String,
        password: String,
    },
    Logout,
    /// Rehydrate a persisted session (tokens + cookies) and verify it.
    Restore(StoredSession),

    // Dashboard ("me") — fanned out in parallel by the worker.
    LoadDashboard {
        fresh: bool,
    },
    // Projects tab.
    LoadProjects {
        fresh: bool,
    },
    LoadMine {
        slug: String,
        fresh: bool,
    },
    DownloadAttachment {
        name: String,
        url: String,
    },
    // User search / profile view.
    Search {
        query: String,
    },
    LoadUser {
        login: String,
    },
    // Event detail popup opened from a notification link.
    LoadEvent {
        id: u32,
    },
    SetEventSubscription {
        id: u32,
        url: String,
        csrf_token: String,
        subscribe: bool,
    },
    // Clusters.
    LoadClusters {
        fresh: bool,
    },
    // Slots.
    LoadSlotsOverview {
        anchor: chrono::NaiveDate,
    },
    SyncSlotsProjects,
    LoadProjectSlots {
        ps_id: u32,
        anchor: chrono::NaiveDate,
        campus: String,
        remote: bool,
    },
    CreateSlot {
        begin: chrono::DateTime<chrono::Local>,
        end: chrono::DateTime<chrono::Local>,
        campus: String,
        remote: bool,
    },
    DeleteSlot {
        start: chrono::DateTime<chrono::Local>,
        end: chrono::DateTime<chrono::Local>,
    },
    BookSlot {
        ps_id: u32,
        time: String,
        campus: String,
    },
    CancelSlot {
        ps_id: u32,
        time: String,
        campus: String,
    },
}

/// Every asynchronous answer the UI can receive.
#[derive(Debug, Clone)]
pub enum Msg {
    Login(Result<LoginOutcome, ApiError>),
    LoggedOut,
    SessionRestored {
        login: String,
        ok: bool,
    },

    MeSummary(Result<MeSummary, ApiError>),
    MyProfile(Result<UserProfile, ApiError>),
    MyCursus(Result<Vec<Cursus>, ApiError>),
    MyCampus(Result<Vec<Campus>, ApiError>),
    MyEvents(Result<Vec<IntraEvent>, ApiError>),
    MyNotifications(Result<NotificationsPayload, ApiError>),
    MyScaleTeams(Result<Vec<ScaleTeam>, ApiError>),
    MyLogtime(Result<LocationStats, ApiError>),
    MyPace(Result<PaceProfile, ApiError>),
    MyAttendance(Result<AttendanceSummary, ApiError>),
    MyAchievements(Result<Vec<Achievement>, ApiError>),

    ProjectData(Result<Vec<ProjectDataEntry>, ApiError>),
    Ongoing(Result<Vec<OngoingProject>, ApiError>),
    Marked(Result<Vec<MarkedProject>, ApiError>),
    Mine {
        slug: String,
        result: Result<ProjectMine, ApiError>,
    },
    DownloadDone {
        name: String,
        result: Result<String, ApiError>,
    },

    SearchResults(Result<Vec<SearchResult>, ApiError>),
    UserView {
        login: String,
        result: Result<Box<UserProfile>, ApiError>,
    },
    UserCursus {
        login: String,
        result: Result<Vec<Cursus>, ApiError>,
    },
    UserAchievements {
        login: String,
        result: Result<Vec<Achievement>, ApiError>,
    },
    UserLogtime {
        login: String,
        result: Result<LocationStats, ApiError>,
    },
    UserPatrons {
        login: String,
        patroning: Result<Vec<PatronUser>, ApiError>,
        patroned: Result<Vec<PatronUser>, ApiError>,
    },

    Clusters(Result<Vec<ClusterSeat>, ApiError>),

    EventDetail {
        id: u32,
        result: Result<EventDetail, ApiError>,
    },
    EventWrite {
        subscribe: bool,
        result: Result<(), ApiError>,
    },

    SlotsProjects(Result<Vec<SlotsProject>, ApiError>),
    SlotsSynced(Result<(), ApiError>),
    OpenSlots(Result<Vec<Slot>, ApiError>),
    ReservedSlots(Result<Vec<Slot>, ApiError>),
    ProjectSlots(Result<Vec<Slot>, ApiError>),
    SlotWrite(Result<(), ApiError>),
}

/// Handle used by the UI to enqueue work.
pub type CommandSink = Sender<Command>;
/// Handle used by the UI to drain answers.
pub type MsgStream = Receiver<Msg>;

/// Spawn the worker thread: owns the tokio runtime + `Api`, converts
/// commands into parallel tasks, forwards results to the UI channel.
pub fn spawn_worker(api: Arc<Api>) -> (CommandSink, MsgStream, std::thread::JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
    let (msg_tx, msg_rx) = std::sync::mpsc::channel::<Msg>();
    let handle = std::thread::Builder::new()
        .name("api-worker".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .enable_all()
                .build()
                .expect("tokio runtime");
            for command in cmd_rx {
                let api = Arc::clone(&api);
                let msg_tx = msg_tx.clone();
                runtime.spawn(async move {
                    crate::worker::dispatch(api, msg_tx, command).await;
                });
            }
        })
        .expect("spawn api worker");
    (cmd_tx, msg_rx, handle)
}
