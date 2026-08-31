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
    /// Open hours of mine that a student booked (slots.42belgium.be) —
    /// evaluations to give that only materialize as intra scale_teams
    /// shortly before they begin.
    pub eval_slots: Loadable<Vec<Slot>>,
    /// Selected row in the evaluations-to-give pane (Enter opens it).
    pub duty_sel: usize,
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

    /// True once the cursus list shows any Piscine entry — used to mask
    /// the pace 404 with a friendly placeholder.
    pub fn is_piscine(&self) -> bool {
        self.cursus
            .data()
            .is_some_and(|list| list.iter().any(Cursus::is_piscine))
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
    /// Converted subject markdown per slug (PDF viewer, `v` on an attachment).
    pub subjects: HashMap<String, Loadable<String>>,
    pub downloading: HashMap<String, bool>,
    /// Slugs with a `git clone` in flight (triggered with `g`).
    pub cloning: Vec<String>,
    /// Destination + folder-name prompt opened with `g`.
    pub clone_prompt: Option<ClonePrompt>,
    /// Popup after a clone bounced off a passphrase-protected ssh key.
    pub passphrase_prompt: Option<PassphrasePrompt>,
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

/// The ssh-passphrase popup: the clone parameters to retry plus the masked
/// input; submitting re-fires `CloneRepo` with the passphrase attached.
#[derive(Debug)]
pub struct PassphrasePrompt {
    pub slug: String,
    pub repo: String,
    pub dest: String,
    pub name: Option<String>,
    pub input: Input,
}

// ------------------------------------------------------- subject view ----

/// Full-screen subject viewer opened with `v` from the projects details.
#[derive(Debug)]
pub struct SubjectView {
    pub slug: String,
    pub title: String,
    pub content: Loadable<String>,
    /// Vertical scroll offset in rendered lines.
    pub scroll: u16,
    /// Visible rows of the last render (for page-wise keys).
    pub view_height: std::cell::Cell<u16>,
    /// Total rendered rows of the last render.
    pub total_height: std::cell::Cell<u16>,
}

// ---------------------------------------------------------- evaluation ----

/// Full-screen evaluation view opened with Enter on an upcoming duty.
#[derive(Debug)]
pub struct EvalView {
    pub scale_team_id: u64,
    pub form: Loadable<EvalForm>,
    /// One answer per question, index-aligned with `form.questions()`.
    pub answers: Vec<AnswerInput>,
    /// Index into `form.flags` — the selected rating flag.
    pub flag: usize,
    pub comment: CommentDraft,
    /// The comment editor captures keys while true.
    pub editing_comment: bool,
    /// Selected row: `0..questions.len()` are questions, then flags,
    /// then comment, then submit.
    pub sel: usize,
    pub submitting: bool,
    /// Submit confirmation popup open.
    pub confirming: bool,
}

impl EvalView {
    pub fn open(scale_team_id: u64) -> Self {
        Self {
            scale_team_id,
            form: Loadable::Loading,
            answers: Vec::new(),
            flag: 0,
            comment: CommentDraft::default(),
            editing_comment: false,
            sel: 0,
            submitting: false,
            confirming: false,
        }
    }

    /// Fill the interactive state from a freshly parsed form: defaults
    /// come from whatever the page had preselected.
    pub fn apply_form(&mut self, form: EvalForm) {
        self.flag = form
            .flags
            .iter()
            .position(|flag| Some(flag.id) == form.default_flag)
            .unwrap_or(0);
        self.answers = form
            .questions()
            .iter()
            .map(|question| match &question.kind {
                QuestionKind::Boolean { yes } => AnswerInput::Boolean(*yes),
                QuestionKind::Choice { options } => {
                    AnswerInput::Choice(options.iter().position(|o| o.selected).unwrap_or(0))
                }
                QuestionKind::Range { value, .. } => AnswerInput::Range(*value),
            })
            .collect();
        self.form = Loadable::Ready(form);
    }

    /// The submission the current state would post.
    pub fn submission(&self, form: &EvalForm) -> EvalSubmission {
        let questions = form.questions();
        EvalSubmission {
            scale_team_id: self.scale_team_id,
            action: form.action.clone(),
            authenticity_token: form.authenticity_token.clone(),
            answers: questions
                .iter()
                .zip(self.answers.iter())
                .map(|(question, answer)| EvalAnswer {
                    question_id: question.question_id,
                    value: answer.submit_value(question),
                    answer_id: question.answer_id.clone(),
                })
                .collect(),
            flag_id: form.flags.get(self.flag).map(|flag| flag.id).unwrap_or(0),
            comment: self.comment.text(),
        }
    }
}

/// The answer being edited for one question.
#[derive(Debug, Clone)]
pub enum AnswerInput {
    Boolean(bool),
    /// Index into the question's options.
    Choice(usize),
    Range(i64),
}

impl AnswerInput {
    /// The exact string the original form input would send.
    pub fn submit_value(&self, question: &EvalQuestion) -> String {
        match (self, &question.kind) {
            (AnswerInput::Boolean(yes), QuestionKind::Boolean { .. }) => {
                if *yes {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            (AnswerInput::Choice(index), QuestionKind::Choice { options }) => options
                .get(*index)
                .map(|option| option.value.clone())
                .unwrap_or_default(),
            (AnswerInput::Range(value), QuestionKind::Range { .. }) => value.to_string(),
            _ => String::new(),
        }
    }
}

/// The closing comment caps at 2048 characters (the form's stated limit).
pub const COMMENT_MAX_CHARS: usize = 2048;

/// Minimal multi-line editor for the closing comment.
#[derive(Debug, Clone)]
pub struct CommentDraft {
    lines: Vec<String>,
    row: usize,
    /// Cursor column, counted in characters.
    col: usize,
}

impl Default for CommentDraft {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
        }
    }
}

impl CommentDraft {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn char_count(&self) -> usize {
        self.lines
            .iter()
            .map(|line| line.chars().count() + 1)
            .sum::<usize>()
            .saturating_sub(1)
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.row.min(self.lines.len() - 1), self.col)
    }

    pub fn line(&self, row: usize) -> &str {
        self.lines.get(row).map(String::as_str).unwrap_or("")
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Insert a character at the cursor, honoring the length cap.
    pub fn insert(&mut self, ch: char) {
        if self.char_count() >= COMMENT_MAX_CHARS {
            return;
        }
        let row = self.row.min(self.lines.len() - 1);
        let line = &mut self.lines[row];
        let col = self.col.min(line.chars().count());
        let byte = line
            .char_indices()
            .nth(col)
            .map(|(byte, _)| byte)
            .unwrap_or(line.len());
        line.insert(byte, ch);
        self.row = row;
        self.col = col + 1;
    }

    /// Split the line at the cursor.
    pub fn newline(&mut self) {
        if self.char_count() >= COMMENT_MAX_CHARS {
            return;
        }
        let row = self.row.min(self.lines.len() - 1);
        let line = &mut self.lines[row];
        let col = self.col.min(line.chars().count());
        let byte = line
            .char_indices()
            .nth(col)
            .map(|(byte, _)| byte)
            .unwrap_or(line.len());
        let tail = line.split_off(byte);
        self.lines.insert(row + 1, tail);
        self.row = row + 1;
        self.col = 0;
    }

    /// Delete backwards; at column 0 the previous line joins this one.
    pub fn backspace(&mut self) {
        let row = self.row.min(self.lines.len() - 1);
        if self.col > 0 {
            let line = &mut self.lines[row];
            let byte = line
                .char_indices()
                .nth(self.col - 1)
                .map(|(byte, _)| byte)
                .unwrap_or(0);
            line.remove(byte);
            self.col -= 1;
        } else if row > 0 {
            let tail = self.lines.remove(row);
            let previous = &mut self.lines[row - 1];
            self.col = previous.chars().count();
            previous.push_str(&tail);
            self.row = row - 1;
        }
    }

    pub fn left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
        }
    }

    pub fn right(&mut self) {
        let line = self.line(self.row);
        if self.col < line.chars().count() {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.line(self.row).chars().count());
        }
    }

    pub fn down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.line(self.row).chars().count());
        }
    }

    pub fn home(&mut self) {
        self.col = 0;
    }

    pub fn end(&mut self) {
        self.col = self.line(self.row).chars().count();
    }
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
    pub marked: Loadable<Vec<MarkedProject>>,
    pub ongoing: Loadable<Vec<OngoingProject>>,
    pub pace: Loadable<PaceProfile>,
    pub campuses: Loadable<Vec<Campus>>,
    /// Per-project team/evaluations for this user (`/{slug}/{login}/`).
    pub user_mines: std::collections::HashMap<String, Loadable<ProjectMine>>,
    /// Selection inside the combined marked/ongoing lists.
    pub project_sel: usize,
    pub project_focus: UserProjectFocus,
    /// Popup showing extra project details (mirrors `projects.intra.42.fr/{slug}/{login}/`).
    pub popup: Option<UserPopup>,
}

#[derive(Debug)]
pub struct UserPopup {
    pub slug: String,
    pub scroll: u16,
    pub view_height: std::cell::Cell<u16>,
    pub total_height: std::cell::Cell<u16>,
}

impl UserView {
    /// True once the cursus list shows any Piscine entry — used to mask
    /// the pace 404 with a friendly placeholder.
    pub fn is_piscine(&self) -> bool {
        self.cursus
            .data()
            .is_some_and(|list| list.iter().any(Cursus::is_piscine))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserProjectFocus {
    #[default]
    Ongoing,
    Marked,
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
