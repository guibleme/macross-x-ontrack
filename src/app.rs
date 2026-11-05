use crate::herdr::{Agent, LaunchContext, LaunchOutcome};
use crate::worktrees::{self, Action as WorktreeAction, CleanupPolicy, ManagedWorktree};
use crate::{
    Tui,
    beads::{self, Bead, CreateBead, UpdateBead},
    event,
    hotkeys::{self, Help},
    screenshots,
    settings::{self, DefaultAgent, Settings, Theme},
    ui,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::Rect;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

fn spawn_epic_planning(id: String, replies: mpsc::Sender<Response>) {
    thread::spawn(move || {
        let session = match crate::epic::launch(&id) {
            Ok(session) => session,
            Err(error) => {
                let _ = replies.send(Response::EpicLaunched(id, Err(error)));
                return;
            }
        };
        if replies
            .send(Response::EpicLaunched(id.clone(), Ok(session.clone())))
            .is_ok()
        {
            let result = crate::epic::finish(&session, &id);
            let _ = replies.send(Response::EpicFinished(id, result));
        }
    });
}

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);
pub const BOARD_STATUSES: [&str; 6] = [
    "open",
    "ready",
    "blocked",
    "in_progress",
    "ready_to_merge",
    "closed",
];

pub(super) enum Request {
    List,
    Show(String, u64),
    LoadEdit(String, u64),
    Create(CreateBead),
    GenerateEpic(String),
    CreateChild(CreateBead, String),
    Update(UpdateBead),
    SetReady(String, bool),
    ResetFailedHandoff(u64, String, String),
    SetTreeReady(String, bool, HashSet<String>, HashSet<String>),
    Move(String, String),
    Close(String),
    Delete(String),
    DiscoverHarnesses(u64),
    Discover(u64),
    Launch(u64, LaunchContext, Agent, Bead),
    LaunchReady(u64, LaunchContext, Agent, Bead),
    ListWorktrees,
    FocusWorkspace(String),
    WorktreeAction(String, WorktreeAction),
    RestartWorktree(String),
    ContinueWorktree(String),
    ReconcileWorktrees(CleanupPolicy),
    ObserveAgents,
    ReadScreenshot(u64),
    PlanLaunch(u64, String, String),
    PlanRead(u64, crate::epic::Session, String),
    PlanReply(u64, crate::epic::Session, String, String),
    PlanCreate(u64, crate::planning::Plan),
}
pub(super) enum Response {
    List(std::io::Result<Vec<Bead>>),
    Show(String, u64, std::io::Result<serde_json::Value>),
    EditLoaded(String, u64, std::io::Result<serde_json::Value>),
    Created(std::io::Result<String>),
    EpicLaunched(String, std::io::Result<crate::epic::Session>),
    EpicFinished(String, std::io::Result<Vec<String>>),
    Updated(String, std::io::Result<()>),
    ReadySet(String, bool, std::io::Result<Vec<String>>),
    FailedHandoffReset(u64, String, String, std::io::Result<()>),
    Moved(String, String, std::io::Result<()>),
    Closed(String, std::io::Result<()>),
    Deleted(std::io::Result<()>),
    HarnessesDiscovered(u64, std::io::Result<Vec<String>>),
    Discovered(u64, std::io::Result<LaunchContext>),
    Launched(u64, std::io::Result<LaunchOutcome>),
    Worktrees(std::io::Result<Vec<ManagedWorktree>>),
    WorkspaceFocused(String, std::io::Result<()>),
    WorktreeChanged(std::io::Result<ManagedWorktree>),
    WorktreeRestarted(std::io::Result<String>),
    WorktreeContinued(std::io::Result<String>),
    AgentsObserved(std::io::Result<Vec<worktrees::AgentDisplay>>),
    ScreenshotRead(u64, std::io::Result<Vec<u8>>),
    PlanLaunched(u64, std::io::Result<crate::epic::Session>),
    PlanRead(
        u64,
        String,
        std::io::Result<Option<crate::planning::Message>>,
    ),
    PlanReplied(u64, String, std::io::Result<()>),
    PlanCreated(u64, Result<Vec<String>, String>),
}
pub(super) enum WorktreeFocusFeedback {
    Success(String),
    Error(String),
}
pub(super) struct DrawerAnimation {
    pub worktrees: bool,
    pub started: Instant,
    pub from: f32,
    pub to: f32,
}
impl DrawerAnimation {
    pub fn fraction(&self, now: Instant) -> f32 {
        let progress =
            (now.saturating_duration_since(self.started).as_secs_f32() / 0.22).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        self.from + (self.to - self.from) * eased
    }
}
pub struct App {
    pub beads: Vec<Bead>,
    pub focused_column: usize,
    pub column_offsets: [usize; 6],
    pub board_columns: Vec<(usize, Rect)>,
    pub board_cards: Vec<(usize, Rect)>,
    pub(crate) column_rows: [usize; 6],
    pub selected: Option<usize>,
    pub detail: Option<serde_json::Value>,
    pub focus_detail: bool,
    pub detail_offset: u16,
    pub drawer_animation: Option<DrawerAnimation>,
    pub list_loading: bool,
    pub detail_loading: bool,
    pub list_error: Option<String>,
    pub detail_error: Option<String>,
    pub(super) detail_seq: u64,
    pub(super) edit_seq: u64,
    pub(super) harness_seq: u64,
    pub(super) launch_seq: u64,
    pub(super) handoff: HashMap<u64, PendingHandoff>,
    /// Accepted agent starts not yet acknowledged by a fresh durable scan.
    pub(crate) handoff_capacity: HashSet<String>,
    pub toast: Option<Toast>,
    pub list_rect: Rect,
    pub detail_rect: Rect,
    pub modal: Option<Modal>,
    pub help: Option<Help>,
    pub(crate) easter_egg: bool,
    pub(super) easter_egg_progress: usize,
    pub settings: Settings,
    pub theme: Theme,
    pub settings_path: Option<PathBuf>,
    pub settings_load_error: Option<String>,
    pub worktrees: Vec<ManagedWorktree>,
    /// Accepted handoffs include removed checkouts, which are hidden from `worktrees`.
    pub accepted_handoffs: HashSet<String>,
    pub(crate) known_handoffs: HashSet<String>,
    pub(crate) scheduler_attempted: HashSet<String>,
    pub selected_worktree: usize,
    pub worktree_offset: usize,
    pub worktree_detail_offset: u16,
    pub worktrees_view: bool,
    pub worktrees_loading: bool,
    pub worktrees_error: Option<String>,
    pub(super) worktrees_focus_feedback: Option<WorktreeFocusFeedback>,
    pub(super) worktrees_focus_pending: bool,
    pub worktrees_inflight: bool,
    pub worktrees_discovered: bool,
    pub worktrees_refresh_pending: bool,
    pub worktree_action_pending: bool,
    pub worktrees_next_refresh: Instant,
    pub(crate) agent_observation_inflight: bool,
    pub(crate) next_agent_observation: Instant,
    pub notice: Option<String>,
    pub mutation_busy: bool,
    /// Ready and board moves exclude other mutations without locking navigation.
    pub ready_pending: bool,
    pub planning_epics: HashSet<String>,
    pub modal_ready: bool,
    pub(crate) paste_seq: u64,
    pub(crate) paste_pending: bool,
    pub(crate) created_screenshots: Vec<PathBuf>,
    pub preferred_id: Option<String>,
    pub(super) refresh_inflight: bool,
    pub(super) refresh_pending: bool,
    pub(super) next_refresh: Instant,
    pub(super) requests: mpsc::Sender<Request>,
    pub(super) responses: mpsc::Receiver<Response>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    #[allow(dead_code)] // Legacy modal is retained but has no browser entry point.
    Planning(PlanningForm),
    Create(CreateForm),
    EditLoading {
        id: String,
        seq: u64,
        error: Option<String>,
    },
    Edit {
        id: String,
        form: CreateForm,
    },
    #[allow(dead_code)] // Legacy confirmation is retained but has no browser entry point.
    ConfirmGenerateEpic {
        id: String,
        title: String,
    },
    ConfirmClose {
        id: String,
        title: String,
    },
    Move {
        id: String,
        title: String,
        selected: usize,
    },
    ConfirmDelete {
        id: String,
        title: String,
    },
    Settings {
        draft: Settings,
        error: Option<String>,
        dropdown_open: bool,
        focus: usize,
    },
    ConfirmWorktree {
        id: String,
        action: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningStage {
    #[allow(dead_code)] // No launch path constructs this legacy stage.
    Draft,
    Waiting,
    Question {
        text: String,
        options: Vec<String>,
    },
    Review(crate::planning::Plan),
    Revision,
    Creating,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningForm {
    pub seq: u64,
    pub proposal: String,
    pub input: String,
    pub marker: String,
    pub round: u64,
    pub session: Option<crate::epic::Session>,
    pub stage: PlanningStage,
    pub selected: usize,
    pub poll_inflight: bool,
    pub next_poll: Instant,
    pub scroll: u16,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub error: bool,
    pub expires_at: Option<Instant>,
}
impl Toast {
    fn progress(message: String) -> Self {
        Self {
            message,
            error: false,
            expires_at: None,
        }
    }
    fn result(message: String, error: bool) -> Self {
        Self {
            message,
            error,
            expires_at: Some(Instant::now() + Duration::from_secs(if error { 15 } else { 7 })),
        }
    }
    pub fn visible(&self, now: Instant) -> bool {
        self.expires_at.is_none_or(|expires| now < expires)
    }
}
#[derive(Debug, Clone)]
pub(super) struct PendingHandoff {
    seq: u64,
    bead: Bead,
    launch_queued: bool,
    scheduled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateForm {
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    pub issue_type: usize,
    pub priority: usize,
    pub harness: usize,
    pub harnesses: Vec<String>,
    pub harness_loading: bool,
    pub harness_error: Option<String>,
    pub harness_seq: u64,
    pub previous_harness: Option<String>,
    pub focus: usize,
    pub cursor: usize,
    pub description_cursor: usize,
    pub description_preferred_col: Option<usize>,
    pub dropdown: Option<usize>,
    pub dropdown_original: Option<usize>,
}
impl Default for CreateForm {
    fn default() -> Self {
        Self {
            parent_id: None,
            title: String::new(),
            description: String::new(),
            issue_type: 0,
            priority: 2,
            harness: 0,
            harnesses: Vec::new(),
            harness_loading: false,
            harness_error: None,
            harness_seq: 0,
            previous_harness: None,
            focus: 0,
            cursor: 0,
            description_cursor: 0,
            description_preferred_col: None,
            dropdown: None,
            dropdown_original: None,
        }
    }
}

impl CreateForm {
    fn insert_screenshot(&mut self, path: &std::path::Path) {
        let reference = screenshots::markdown_reference(path);
        let cursor = self.description_cursor.min(self.description.len());
        self.description.insert_str(cursor, &reference);
        self.description_cursor = cursor + reference.len();
        self.description_preferred_col = None;
    }

    fn set_harnesses(&mut self, harnesses: Vec<String>, preferred: DefaultAgent) {
        self.harnesses = harnesses;
        self.harness = match preferred {
            DefaultAgent::Unassigned => 0,
            DefaultAgent::Kind(kind) => self
                .harnesses
                .iter()
                .position(|available| available == kind)
                .map(|index| index + 1)
                .unwrap_or_else(|| self.automatic_harness()),
            DefaultAgent::Automatic => self.automatic_harness(),
        };
        self.harness_loading = false;
        self.harness_error = None;
        self.dropdown = None;
        self.dropdown_original = None;
    }

    fn automatic_harness(&self) -> usize {
        self.harnesses
            .iter()
            .position(|kind| kind == "codex")
            .map_or_else(
                || usize::from(!self.harnesses.is_empty()),
                |index| index + 1,
            )
    }
}

fn edit_form(record: &serde_json::Value) -> Result<CreateForm, String> {
    let title = record
        .get("title")
        .and_then(serde_json::Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .ok_or("Bead has no editable title")?;
    let issue_type = record
        .get("issue_type")
        .and_then(serde_json::Value::as_str)
        .and_then(|kind| beads::BEAD_TYPES.iter().position(|known| *known == kind))
        .ok_or("Bead type is missing or unsupported by the edit form")?;
    let priority = record
        .get("priority")
        .and_then(serde_json::Value::as_u64)
        .filter(|priority| *priority <= 4)
        .ok_or("Bead priority is missing or outside P0–P4")? as usize;
    let description = match record.get("description") {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(value)) => value.clone(),
        _ => return Err("Bead description is not text".into()),
    };
    Ok(CreateForm {
        title: title.into(),
        cursor: title.len(),
        description_cursor: description.len(),
        description,
        issue_type,
        priority,
        harness: beads::harness(record)
            .and_then(|kind| beads::HARNESSES.iter().position(|known| *known == kind))
            .map_or(0, |index| index + 1),
        harnesses: beads::HARNESSES
            .iter()
            .map(|kind| (*kind).to_owned())
            .collect(),
        previous_harness: beads::harness(record).map(str::to_owned),
        ..Default::default()
    })
}
fn previous_boundary(s: &str, cursor: usize) -> usize {
    s[..cursor.min(s.len())]
        .char_indices()
        .last()
        .map_or(0, |(i, _)| i)
}
fn next_boundary(s: &str, cursor: usize) -> usize {
    let cursor = cursor.min(s.len());
    s[cursor..]
        .char_indices()
        .nth(1)
        .map_or(s.len(), |(i, _)| cursor + i)
}
/// Return the next choice index, wrapping in either direction.
fn cycle_choice(index: usize, choices: usize, forward: bool) -> usize {
    assert!(
        choices > 0,
        "a choice control must have at least one option"
    );
    if forward {
        (index % choices + 1) % choices
    } else {
        (index % choices + choices - 1) % choices
    }
}

fn cycle_theme(theme: Theme, forward: bool) -> Theme {
    let index = Theme::ALL
        .iter()
        .position(|choice| *choice == theme)
        .unwrap_or(0);
    Theme::ALL[cycle_choice(index, Theme::ALL.len(), forward)]
}
fn execute_request(request: Request, out: &mpsc::Sender<Response>) {
    let response = match request {
        Request::List => Response::List(beads::list_browser_beads()),
        Request::Show(id, seq) => Response::Show(id.clone(), seq, beads::show_bead(&id)),
        Request::LoadEdit(id, seq) => Response::EditLoaded(id.clone(), seq, beads::show_bead(&id)),
        Request::Create(input) => Response::Created(beads::create_bead(&input)),
        Request::GenerateEpic(id) => {
            spawn_epic_planning(id, out.clone());
            return;
        }
        Request::CreateChild(input, parent) => {
            Response::Created(beads::create_child(&input, &parent))
        }
        Request::Update(input) => Response::Updated(input.id.clone(), beads::update_bead(&input)),
        Request::ReadScreenshot(seq) => {
            Response::ScreenshotRead(seq, screenshots::read_clipboard())
        }
        Request::PlanLaunch(seq, proposal, marker) => {
            Response::PlanLaunched(seq, crate::planning::launch(&proposal, &marker))
        }
        Request::PlanRead(seq, session, marker) => {
            let result = crate::planning::read(&session, &marker);
            Response::PlanRead(seq, marker, result)
        }
        Request::PlanReply(seq, session, message, marker) => {
            let result = crate::planning::reply(&session, &message, &marker);
            Response::PlanReplied(seq, marker, result)
        }
        Request::PlanCreate(seq, plan) => {
            Response::PlanCreated(seq, crate::planning::create(&plan))
        }
        Request::ResetFailedHandoff(seq, id, message) => {
            let result = beads::set_ready(&id, false);
            Response::FailedHandoffReset(seq, id, message, result)
        }
        Request::SetReady(id, ready) => Response::ReadySet(
            id.clone(),
            ready,
            beads::set_ready(&id, ready).map(|()| vec![id]),
        ),
        Request::SetTreeReady(id, ready, known_handoffs, protected_handoffs) => Response::ReadySet(
            id.clone(),
            ready,
            beads::set_tree_ready(&id, ready, &known_handoffs, &protected_handoffs),
        ),
        Request::Move(id, target) => {
            Response::Moved(id.clone(), target.clone(), beads::move_bead(&id, &target))
        }
        Request::Close(id) => Response::Closed(id.clone(), worktrees::close_integrated_bead(&id)),
        Request::Delete(id) => Response::Deleted(beads::delete_bead(&id)),
        Request::DiscoverHarnesses(seq) => {
            Response::HarnessesDiscovered(seq, crate::herdr::installed_harnesses())
        }
        Request::Discover(seq) => Response::Discovered(seq, crate::herdr::discover()),
        Request::Launch(seq, context, agent, bead) => {
            Response::Launched(seq, crate::herdr::launch(&context, &agent, &bead))
        }
        Request::LaunchReady(seq, context, agent, bead) => {
            Response::Launched(seq, crate::herdr::launch_ready(&context, &agent, &bead))
        }
        Request::ListWorktrees => Response::Worktrees(worktrees::list()),
        Request::FocusWorkspace(id) => {
            Response::WorkspaceFocused(id.clone(), crate::herdr::focus_workspace(&id))
        }
        Request::WorktreeAction(id, action) => {
            Response::WorktreeChanged(worktrees::act(&id, action))
        }
        Request::RestartWorktree(id) => Response::WorktreeRestarted(worktrees::restart(&id)),
        Request::ContinueWorktree(id) => Response::WorktreeContinued(worktrees::continue_task(&id)),
        Request::ReconcileWorktrees(policy) => Response::Worktrees(worktrees::reconcile(policy)),
        Request::ObserveAgents => Response::AgentsObserved(worktrees::observe_agents()),
    };
    let _ = out.send(response);
}

/// Board moves/closes share the managed FIFO with reconciliation and cleanup.
/// Browser reads, Ready cascades, and independent Herdr jobs stay separate.
/// Detached jobs are not joined or cancelled when the terminal app exits.
fn start_workers(
    execute: impl Fn(Request, &mpsc::Sender<Response>) + Send + Sync + 'static,
) -> (
    mpsc::Sender<Request>,
    mpsc::Receiver<Response>,
    mpsc::Sender<Response>,
) {
    let execute = Arc::new(execute);
    let (requests, incoming) = mpsc::channel();
    let (out, responses) = mpsc::channel();
    let (browser, browser_in) = mpsc::channel();
    let (managed, managed_in) = mpsc::channel();
    let (ready, ready_in) = mpsc::channel();
    for lane in [browser_in, managed_in, ready_in] {
        let execute = execute.clone();
        let out = out.clone();
        thread::spawn(move || {
            while let Ok(request) = lane.recv() {
                execute(request, &out);
            }
        });
    }
    let replies = out.clone();
    thread::spawn(move || {
        while let Ok(request) = incoming.recv() {
            if matches!(
                request,
                Request::Discover(_)
                    | Request::PlanLaunch(..)
                    | Request::PlanRead(..)
                    | Request::PlanReply(..)
                    | Request::DiscoverHarnesses(_)
                    | Request::Launch(..)
                    | Request::LaunchReady(..)
                    | Request::FocusWorkspace(_)
                    | Request::ReadScreenshot(_)
                    | Request::ObserveAgents
            ) {
                let execute = execute.clone();
                let out = replies.clone();
                thread::spawn(move || execute(request, &out));
            } else if matches!(
                request,
                Request::ListWorktrees
                    | Request::Move(..)
                    | Request::Close(_)
                    | Request::WorktreeAction(..)
                    | Request::RestartWorktree(_)
                    | Request::ContinueWorktree(_)
                    | Request::ReconcileWorktrees(_)
            ) {
                if managed.send(request).is_err() {
                    break;
                }
            } else if matches!(
                request,
                Request::SetReady(..) | Request::SetTreeReady(..) | Request::ResetFailedHandoff(..)
            ) {
                if ready.send(request).is_err() {
                    break;
                }
            } else if browser.send(request).is_err() {
                break;
            }
        }
    });
    (requests, responses, out)
}

impl App {
    fn set_drawer(&mut self, worktrees: bool, open: bool, now: Instant) {
        let from = self
            .drawer_animation
            .as_ref()
            .filter(|animation| animation.worktrees == worktrees)
            .map(|animation| animation.fraction(now))
            .unwrap_or(
                if (worktrees && self.worktrees_view) || (!worktrees && self.focus_detail) {
                    1.0
                } else {
                    0.0
                },
            );
        self.drawer_animation = self.settings.animate_drawers.then_some(DrawerAnimation {
            worktrees,
            started: now,
            from,
            to: if open { 1.0 } else { 0.0 },
        });
        self.worktrees_view = worktrees && open;
        self.focus_detail = !worktrees && open;
    }

    fn new() -> Self {
        let (tx, responses, _) = start_workers(execute_request);
        let (settings, settings_path, settings_error) = match settings::config_path() {
            Ok(path) => match settings::load_from(&path) {
                Ok(value) => (value, Some(path), None),
                Err(error) => (
                    Settings::default(),
                    Some(path),
                    Some(format!("Settings could not be loaded: {error}")),
                ),
            },
            Err(error) => (
                Settings::default(),
                None,
                Some(format!("Settings location unavailable: {error}")),
            ),
        };
        let theme = settings.theme;
        let mut a = Self {
            beads: vec![],
            focused_column: 0,
            column_offsets: [0; 6],
            board_columns: Vec::new(),
            board_cards: Vec::new(),
            column_rows: [0; 6],
            selected: None,
            detail: None,
            focus_detail: false,
            detail_offset: 0,
            drawer_animation: None,
            list_loading: true,
            detail_loading: false,
            list_error: None,
            detail_error: None,
            detail_seq: 0,
            edit_seq: 0,
            harness_seq: 0,
            launch_seq: 0,
            handoff: HashMap::new(),
            handoff_capacity: HashSet::new(),
            toast: None,
            list_rect: Rect::default(),
            detail_rect: Rect::default(),
            modal: None,
            help: None,
            easter_egg: false,
            easter_egg_progress: 0,
            settings,
            theme,
            settings_path,
            settings_load_error: settings_error.clone(),
            worktrees: Vec::new(),
            accepted_handoffs: HashSet::new(),
            known_handoffs: HashSet::new(),
            scheduler_attempted: HashSet::new(),
            selected_worktree: 0,
            worktree_offset: 0,
            worktree_detail_offset: 0,
            worktrees_view: false,
            worktrees_loading: false,
            worktrees_error: None,
            worktrees_focus_feedback: None,
            worktrees_focus_pending: false,
            worktrees_inflight: false,
            worktrees_discovered: false,
            worktrees_refresh_pending: false,
            worktree_action_pending: false,
            worktrees_next_refresh: Instant::now(),
            agent_observation_inflight: false,
            next_agent_observation: Instant::now(),
            notice: settings_error,
            mutation_busy: false,
            ready_pending: false,
            planning_epics: HashSet::new(),
            modal_ready: true,
            paste_seq: 0,
            paste_pending: false,
            created_screenshots: Vec::new(),
            preferred_id: None,
            refresh_inflight: false,
            refresh_pending: false,
            next_refresh: Instant::now(),
            requests: tx,
            responses,
        };
        a.refresh();
        a
    }
    fn refresh(&mut self) {
        self.refresh_at(Instant::now());
    }
    fn refresh_at(&mut self, now: Instant) {
        if self.refresh_inflight {
            // Explicit refreshes and mutation invalidations need one newer snapshot.
            self.refresh_pending = true;
        } else {
            self.start_refresh(now);
        }
    }
    fn poll_refresh(&mut self, now: Instant) -> bool {
        if !self.refresh_inflight && now >= self.next_refresh {
            self.start_refresh(now);
            return !self.refresh_inflight;
        }
        false
    }
    fn poll_planning(&mut self, now: Instant) -> bool {
        let Some(Modal::Planning(form)) = self.modal.as_mut() else {
            return false;
        };
        if !matches!(form.stage, PlanningStage::Waiting)
            || form.poll_inflight
            || now < form.next_poll
        {
            return false;
        }
        let Some(session) = form.session.clone() else {
            return false;
        };
        form.poll_inflight = true;
        form.next_poll = now + Duration::from_secs(2);
        if self
            .requests
            .send(Request::PlanRead(form.seq, session, form.marker.clone()))
            .is_err()
        {
            form.poll_inflight = false;
            form.stage = PlanningStage::Failed("Agent output read could not be queued".into());
            return true;
        }
        false
    }
    /// Report progress so the loop can drain eligible work without idle scans.
    fn poll_scheduler(&mut self) -> bool {
        if !self.worktrees_discovered || self.mutation_busy || self.list_error.is_some() {
            return false;
        }
        // A child must branch from its managed parent checkout. Preparing a
        // parent only reserves that checkout; it does not occupy an agent slot.
        if let Some(parent) = self
            .beads
            .iter()
            .find(|bead| {
                if !beads::is_ready(bead)
                    || !self.scheduler_eligible(bead)
                    || self.scheduler_attempted.contains(&bead.id)
                    || self.known_handoffs.contains(&bead.id)
                    || self.worktrees.iter().any(|item| item.bead_id == bead.id)
                    || self
                        .handoff
                        .values()
                        .any(|pending| pending.bead.id == bead.id)
                {
                    return false;
                }
                let descendants = beads::open_descendants_in(&self.beads, &bead.id);
                self.beads.iter().any(|child| {
                    beads::is_ready(child)
                        && child.dependency_ready
                        && child.data["issue_type"] != "epic"
                        && !beads::explicitly_unassigned(&child.data)
                        && (!beads::has_harness_label(&child.data)
                            || beads::harness(&child.data).is_some())
                        && !self.known_handoffs.contains(&child.id)
                        && descendants.contains(&child.id)
                })
            })
            .cloned()
        {
            self.scheduler_attempted.insert(parent.id.clone());
            self.start_launch(parent, true);
            return true;
        }
        let active_agents =
            self.worktrees
                .iter()
                .filter(|item| {
                    item.handoff_accepted
                        && matches!(
                            item.status,
                            worktrees::WorktreeStatus::InProgress
                                | worktrees::WorktreeStatus::DirtyCheckout
                        )
                })
                .count()
                + self
                    .handoff_capacity
                    .iter()
                    .filter(|id| {
                        !self
                            .worktrees
                            .iter()
                            .any(|item| &item.bead_id == *id && item.handoff_accepted)
                    })
                    .count()
                + self
                    .handoff
                    .values()
                    .filter(|pending| {
                        // Reserving a parent checkout for open descendants starts no
                        // agent. Resuming that reserved parent after its children does.
                        !self.reserves_parent(&pending.bead)
                            && !self.worktrees.iter().any(|item| {
                                item.bead_id == pending.bead.id && item.handoff_accepted
                            })
                    })
                    .count();
        if !self.settings.agent_limit.allows(active_agents) {
            return false;
        }
        let next = self
            .beads
            .iter()
            .filter(|bead| {
                beads::is_ready(bead)
                    && bead.dependency_ready
                    && bead.data["issue_type"] != "epic"
                    && self.scheduler_eligible(bead)
                    && !self.scheduler_attempted.contains(&bead.id)
            })
            .min_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)))
            .cloned();
        if let Some(bead) = next {
            self.scheduler_attempted.insert(bead.id.clone());
            if beads::explicitly_unassigned(&bead.data) {
                let message = format!(
                    "{} is Ready but explicitly Unassigned; edit it to select an agent",
                    bead.id
                );
                self.notice = Some(message.clone());
                self.toast = Some(Toast::result(message, true));
            } else if beads::has_harness_label(&bead.data) && beads::harness(&bead.data).is_none() {
                let message = format!(
                    "{} has an unsupported agent harness; edit it to select one",
                    bead.id
                );
                self.notice = Some(message.clone());
                self.toast = Some(Toast::result(message, true));
            } else {
                self.start_launch(bead, true);
            }
            return true;
        }
        false
    }
    fn scheduler_eligible(&self, bead: &Bead) -> bool {
        if self
            .handoff
            .values()
            .any(|pending| pending.bead.id == bead.id)
        {
            return false;
        }
        if let Some(parent) = bead.data["parent"].as_str()
            && !self.worktrees.iter().any(|item| {
                item.bead_id == parent
                    && matches!(
                        item.status,
                        worktrees::WorktreeStatus::Reserved | worktrees::WorktreeStatus::InProgress
                    )
            })
        {
            return false;
        }
        if !self.known_handoffs.contains(&bead.id) {
            return true;
        }
        let Some(reserved) = self.worktrees.iter().find(|item| {
            item.bead_id == bead.id && item.status == worktrees::WorktreeStatus::Reserved
        }) else {
            return false;
        };
        beads::open_descendants_in(&self.beads, &bead.id).is_empty()
            && !self.worktrees.iter().any(|item| {
                item.parent_bead_id.as_deref() == Some(bead.id.as_str())
                    && !matches!(
                        item.status,
                        worktrees::WorktreeStatus::Integrated
                            | worktrees::WorktreeStatus::WorkspaceClosed
                            | worktrees::WorktreeStatus::CheckoutRemoved
                    )
            })
            && !reserved.workspace_id.is_empty()
    }
    fn reserves_parent(&self, bead: &Bead) -> bool {
        !beads::open_descendants_in(&self.beads, &bead.id).is_empty()
            || self.worktrees.iter().any(|item| {
                item.parent_bead_id.as_deref() == Some(bead.id.as_str())
                    && !matches!(
                        item.status,
                        worktrees::WorktreeStatus::Integrated
                            | worktrees::WorktreeStatus::WorkspaceClosed
                            | worktrees::WorktreeStatus::CheckoutRemoved
                    )
            })
    }
    fn poll_worktrees(&mut self, now: Instant) -> bool {
        let automatic = self.settings.auto_merge
            || self.settings.auto_merge_children
            || self.settings.auto_close_workspace
            || self.settings.auto_remove_checkout;
        let should_poll = !self.worktrees_discovered
            || automatic
            || !self.known_handoffs.is_empty()
            || !self.worktrees.is_empty()
            || self.worktrees_view;
        if should_poll
            && !self.worktrees_inflight
            && !self.worktree_action_pending
            && now >= self.worktrees_next_refresh
        {
            self.worktrees_inflight = true;
            let was_loading = self.worktrees_loading;
            // Only the initial scan hides empty-state guidance. A refresh or
            // retry keeps the settled view visible while the request is in flight.
            self.worktrees_loading = !self.worktrees_discovered && self.worktrees_error.is_none();
            let policy = CleanupPolicy {
                auto_merge: self.settings.auto_merge,
                auto_merge_children: self.settings.auto_merge_children,
                auto_close_workspace: self.settings.auto_close_workspace,
                auto_remove_checkout: self.settings.auto_remove_checkout,
            };
            let request = if !self.handoff.is_empty()
                || !self.worktrees_discovered
                || (self.worktrees_view && self.worktrees.is_empty())
            {
                Request::ListWorktrees
            } else {
                Request::ReconcileWorktrees(policy)
            };
            if self.requests.send(request).is_err() {
                self.worktrees_inflight = false;
                self.worktrees_loading = false;
                self.worktrees_error = Some("Worktree refresh could not be queued".into());
                self.worktrees_next_refresh = now + REFRESH_INTERVAL;
                return true;
            }
            return was_loading != self.worktrees_loading;
        }
        false
    }
    fn poll_agent_observations(&mut self, now: Instant) {
        if self.agent_observation_inflight
            || self.worktrees.is_empty()
            || now < self.next_agent_observation
        {
            return;
        }
        self.agent_observation_inflight = true;
        if self.requests.send(Request::ObserveAgents).is_err() {
            self.agent_observation_inflight = false;
            self.next_agent_observation = now + Duration::from_secs(2);
        }
    }
    fn start_refresh(&mut self, now: Instant) {
        self.refresh_inflight = true;
        if self.requests.send(Request::List).is_err() {
            self.refresh_inflight = false;
            self.list_loading = false;
            self.list_error = Some("Refresh could not be queued; retrying".into());
            self.next_refresh = now + REFRESH_INTERVAL;
        }
    }
    fn refresh_detail(&mut self) {
        let Some(bead) = self.selected.and_then(|i| self.beads.get(i)) else {
            return;
        };
        // Every snapshot invalidates older shows, even when the list is unchanged:
        // comments and other show-only fields can change independently.
        self.detail_seq = self.detail_seq.wrapping_add(1);
        if self
            .requests
            .send(Request::Show(bead.id.clone(), self.detail_seq))
            .is_err()
        {
            self.detail_loading = false;
            self.detail_error = Some("Details could not be queued; retrying".into());
        }
    }
    fn select(&mut self, index: usize) {
        if self.beads.is_empty() {
            self.clear_selection();
            return;
        }
        let index = index.min(self.beads.len() - 1);
        self.focused_column = status_order(display_status(&self.beads[index]));
        let indices = self.column_indices(self.focused_column);
        self.column_rows[self.focused_column] =
            indices.iter().position(|&i| i == index).unwrap_or(0);
        if self.selected == Some(index) {
            return;
        }
        let capacity = self
            .board_cards
            .iter()
            .filter(|&&(i, _)| {
                self.beads
                    .get(i)
                    .is_some_and(|bead| status_order(display_status(bead)) == self.focused_column)
            })
            .count();
        if capacity > 0 {
            let row = self.column_rows[self.focused_column];
            let offset = &mut self.column_offsets[self.focused_column];
            if row < *offset {
                *offset = row;
            } else if row >= offset.saturating_add(capacity) {
                *offset = row + 1 - capacity;
            }
        }
        self.selected = Some(index);
        self.detail_seq = self.detail_seq.wrapping_add(1);
        self.detail = Some(self.beads[index].data.clone());
        self.detail_offset = 0;
        self.detail_loading = true;
        self.detail_error = None;
        let _ = self
            .requests
            .send(Request::Show(self.beads[index].id.clone(), self.detail_seq));
    }
    fn clear_selection(&mut self) {
        self.selected = None;
        self.detail = None;
        self.detail_loading = false;
        self.detail_error = None;
        self.detail_offset = 0;
    }
    fn step(&mut self, d: isize) {
        let indices = self.column_indices(self.focused_column);
        if indices.is_empty() {
            self.clear_selection();
            return;
        }
        let next = self
            .selected
            .and_then(|index| indices.iter().position(|&i| i == index))
            .map_or(
                self.column_rows[self.focused_column].min(indices.len() - 1),
                |row| row.saturating_add_signed(d).min(indices.len() - 1),
            );
        self.select(indices[next]);
    }
    fn column_indices(&self, column: usize) -> Vec<usize> {
        list_layout(&self.beads, &HashSet::new()).groups[column]
            .indices
            .clone()
    }
    fn focus_column(&mut self, column: usize) {
        self.focused_column = column.min(BOARD_STATUSES.len() - 1);
        let indices = self.column_indices(self.focused_column);
        if indices.is_empty() {
            self.clear_selection();
        } else {
            self.select(indices[self.column_rows[self.focused_column].min(indices.len() - 1)]);
        }
    }
    fn move_selected(&mut self, direction: isize) {
        if let Some(bead) = self.selected.and_then(|i| self.beads.get(i)) {
            let column = status_order(display_status(bead));
            let target = column
                .saturating_add_signed(direction)
                .min(BOARD_STATUSES.len() - 1);
            if column != target {
                self.move_bead(bead.id.clone(), bead.title.clone(), target);
            }
        }
    }
    fn move_bead(&mut self, id: String, title: String, target: usize) {
        if self.mutation_busy {
            return;
        }
        if BOARD_STATUSES[target] == "closed" {
            self.modal = Some(Modal::ConfirmClose { id, title });
            self.modal_ready = false;
            return;
        }
        self.mutation_busy = true;
        self.ready_pending = true;
        self.notice = Some(format!("Moving {id} to {}…", BOARD_STATUSES[target]));
        if self
            .requests
            .send(Request::Move(id, BOARD_STATUSES[target].into()))
            .is_err()
        {
            self.mutation_busy = false;
            self.ready_pending = false;
            self.notice = Some("Move could not be queued".into());
        } else {
            self.modal = None;
        }
    }
    fn receive(&mut self) -> bool {
        self.receive_at(Instant::now())
    }
    fn receive_at(&mut self, now: Instant) -> bool {
        let mut received = false;
        while let Ok(r) = self.responses.try_recv() {
            received = true;
            match r {
                Response::List(res) => {
                    self.refresh_inflight = false;
                    self.next_refresh = now + REFRESH_INTERVAL;
                    self.list_loading = false;
                    match res {
                        Ok(items) => {
                            let id = self
                                .selected
                                .and_then(|i| self.beads.get(i))
                                .map(|b| b.id.clone());
                            self.beads = items;
                            self.beads.sort_by(|a, b| {
                                status_order(display_status(a))
                                    .cmp(&status_order(display_status(b)))
                                    .then_with(|| a.status.cmp(&b.status))
                                    .then_with(|| a.id.cmp(&b.id))
                            });
                            // An in-flight pre-mutation snapshot must not consume
                            // the new ID before the trailing refresh can see it.
                            let preferred = if self.refresh_pending {
                                None
                            } else {
                                self.preferred_id
                                    .take()
                                    .and_then(|id| self.beads.iter().position(|b| b.id == id))
                            };
                            let same_id = id
                                .as_ref()
                                .and_then(|id| self.beads.iter().position(|b| &b.id == id));
                            // Indices may now refer to different beads. Only the
                            // surviving ID can reuse the existing detail and scroll.
                            self.selected = None;
                            if let Some(i) = preferred.or(same_id) {
                                if Some(i) == same_id {
                                    self.selected = Some(i);
                                    self.select(i);
                                    self.refresh_detail();
                                } else {
                                    self.select(i);
                                }
                                if preferred.is_some() {
                                    self.column_offsets[self.focused_column] =
                                        self.column_rows[self.focused_column];
                                }
                            } else {
                                self.focus_column(self.focused_column);
                            }
                            self.list_error = None;
                        }
                        Err(e) => self.list_error = Some(e.to_string()),
                    }
                    if std::mem::take(&mut self.refresh_pending) {
                        self.start_refresh(now);
                    }
                }
                Response::Show(id, seq, res) => {
                    if self
                        .selected
                        .and_then(|i| self.beads.get(i))
                        .is_some_and(|b| b.id == id)
                        && seq == self.detail_seq
                    {
                        match res {
                            Ok(v) => {
                                self.detail = Some(v);
                                self.detail_loading = false;
                                self.detail_error = None;
                            }
                            Err(e) => {
                                self.detail_loading = false;
                                self.detail_error = Some(e.to_string());
                            }
                        }
                    }
                }
                Response::EditLoaded(id, seq, result) => {
                    if let Some(Modal::EditLoading {
                        id: pending,
                        seq: active,
                        error,
                    }) = self.modal.as_mut()
                        && *pending == id
                        && *active == seq
                    {
                        match result
                            .map_err(|e| e.to_string())
                            .and_then(|v| edit_form(&v))
                        {
                            Ok(form) => self.modal = Some(Modal::Edit { id, form }),
                            Err(message) => *error = Some(message),
                        }
                    }
                }
                Response::Created(result) => {
                    self.mutation_busy = false;
                    match result {
                        Ok(id) => {
                            self.notice = Some(format!("Created {id}"));
                            self.created_screenshots.clear();
                            self.modal = None;
                            self.preferred_id = Some(id);
                            self.refresh_at(now);
                        }
                        Err(e) => {
                            self.notice = Some(format!("Create failed: {e}"));
                        }
                    }
                }
                Response::EpicLaunched(id, result) => match result {
                    Ok(session) => {
                        self.planning_epics.insert(id.clone());
                        let message = format!(
                            "Planning {id} in Herdr workspace {} tab {}…",
                            session.workspace_id, session.tab_id
                        );
                        self.notice = Some(message.clone());
                        self.toast = Some(Toast::progress(message));
                    }
                    Err(error) => {
                        self.planning_epics.remove(&id);
                        let message = format!("Epic {id}: planning failed: {error}");
                        self.notice = Some(message.clone());
                        self.toast = Some(Toast::result(message, true));
                    }
                },
                Response::EpicFinished(id, result) => {
                    self.planning_epics.remove(&id);
                    let (message, error) = match result {
                        Ok(children) => (
                            format!(
                                "Epic {id}: {} child Beads created; planning tab closed",
                                children.len()
                            ),
                            false,
                        ),
                        Err(error) => (
                            format!("Epic {id}: planning needs attention: {error}"),
                            true,
                        ),
                    };
                    self.notice = Some(message.clone());
                    self.toast = Some(Toast::result(message, error));
                    self.refresh_at(now);
                }
                Response::PlanLaunched(seq, result) => {
                    if let Some(Modal::Planning(form)) = self.modal.as_mut()
                        && form.seq == seq
                    {
                        match result {
                            Ok(session) => {
                                form.session = Some(session);
                                form.next_poll = now;
                            }
                            Err(error) => form.stage = PlanningStage::Failed(error.to_string()),
                        }
                    } else if let Ok(session) = result {
                        self.notice = Some(format!(
                            "Cancelled planning started in Herdr tab {}; inspect or close it there",
                            session.tab_id
                        ));
                    }
                }
                Response::PlanRead(seq, marker, result) => {
                    if let Some(Modal::Planning(form)) = self.modal.as_mut()
                        && form.seq == seq
                        && form.marker == marker
                    {
                        form.poll_inflight = false;
                        match result {
                            Ok(Some(crate::planning::Message::Question { text, options })) => {
                                form.stage = PlanningStage::Question { text, options };
                                form.selected = 0;
                                form.input.clear();
                                form.scroll = 0;
                            }
                            Ok(Some(crate::planning::Message::Plan(plan))) => {
                                form.stage = PlanningStage::Review(plan);
                                form.scroll = 0;
                            }
                            Ok(None) => {}
                            Err(error) => form.stage = PlanningStage::Failed(error.to_string()),
                        }
                    }
                }
                Response::PlanReplied(seq, marker, result) => {
                    if let Some(Modal::Planning(form)) = self.modal.as_mut()
                        && form.seq == seq
                        && form.marker == marker
                    {
                        if let Err(error) = result {
                            form.stage = PlanningStage::Failed(error.to_string());
                        } else {
                            form.next_poll = now;
                        }
                    }
                }
                Response::PlanCreated(seq, result) => {
                    if !matches!(self.modal, Some(Modal::Planning(ref form)) if form.seq == seq) {
                        continue;
                    }
                    self.mutation_busy = false;
                    match result {
                        Ok(ids) => {
                            self.notice =
                                Some(format!("Created planned Beads: {}", ids.join(", ")));
                            self.preferred_id = ids.first().cloned();
                            self.modal = None;
                        }
                        Err(error) => {
                            self.notice = Some(format!("Planning creation failed: {error}"));
                            if let Some(Modal::Planning(form)) = self.modal.as_mut() {
                                form.stage = PlanningStage::Failed(error);
                            }
                        }
                    }
                    self.refresh_at(now);
                }
                Response::Updated(id, result) => {
                    self.mutation_busy = false;
                    match result {
                        Ok(()) => {
                            self.notice = Some(format!("Updated {id}"));
                            self.created_screenshots.clear();
                            self.modal = None;
                            self.preferred_id = Some(id);
                            self.refresh_at(now);
                        }
                        Err(e) => self.notice = Some(format!("Update failed: {e}")),
                    }
                }
                Response::ScreenshotRead(seq, result) => {
                    if seq != self.paste_seq {
                        continue;
                    }
                    self.paste_pending = false;
                    if let Some(Modal::Create(form) | Modal::Edit { form, .. }) =
                        self.modal.as_mut()
                    {
                        match result.and_then(|bytes| screenshots::save(&bytes)) {
                            Ok(path) => {
                                form.insert_screenshot(&path);
                                self.notice = Some(format!("Screenshot saved: {}", path.display()));
                                self.created_screenshots.push(path);
                            }
                            Err(error) => {
                                self.notice = Some(format!("Screenshot paste failed: {error}"))
                            }
                        }
                    }
                }
                Response::FailedHandoffReset(seq, id, message, result) => {
                    let message = match result {
                        Ok(()) => {
                            for bead in self.beads.iter_mut().filter(|bead| bead.id == id) {
                                if let Some(labels) = bead.data["labels"].as_array_mut() {
                                    labels.retain(|label| label != beads::READY_LABEL);
                                }
                            }
                            format!("{message}; {id} removed from Ready")
                        }
                        Err(error) => {
                            format!("{message}; could not remove Ready from {id}: {error}")
                        }
                    };
                    self.handoff_feedback(seq, message, true);
                    self.refresh_at(now);
                }
                Response::ReadySet(id, ready, result) => {
                    self.mutation_busy = false;
                    self.ready_pending = false;
                    match result {
                        Ok(ids) => {
                            for changed_id in &ids {
                                if ready {
                                    self.scheduler_attempted.remove(changed_id);
                                } else {
                                    self.scheduler_attempted.insert(changed_id.clone());
                                }
                            }
                            for bead in self.beads.iter_mut().filter(|bead| ids.contains(&bead.id))
                            {
                                let labels = bead.data["labels"].as_array_mut();
                                if ready {
                                    if let Some(labels) = labels {
                                        if !labels.iter().any(|label| label == beads::READY_LABEL) {
                                            labels.push(serde_json::Value::String(
                                                beads::READY_LABEL.into(),
                                            ));
                                        }
                                    } else {
                                        bead.data["labels"] =
                                            serde_json::json!([beads::READY_LABEL]);
                                    }
                                } else if let Some(labels) = labels {
                                    labels.retain(|label| label != beads::READY_LABEL);
                                }
                            }
                            self.notice = Some(format!(
                                "{id} {} Ready",
                                if ready { "marked" } else { "removed from" }
                            ));
                            if self
                                .selected
                                .and_then(|i| self.beads.get(i))
                                .is_some_and(|bead| bead.id == id)
                            {
                                self.preferred_id = Some(id.clone());
                            }
                            self.refresh_at(now);
                        }
                        Err(error) => {
                            self.notice = Some(format!("Could not update Ready for {id}: {error}"));
                            self.refresh_at(now);
                        }
                    }
                }
                Response::Moved(id, target, result) => {
                    self.mutation_busy = false;
                    self.ready_pending = false;
                    let (message, error) = match result {
                        Ok(()) => {
                            if target == "ready" {
                                self.scheduler_attempted.remove(&id);
                            } else {
                                self.scheduler_attempted.insert(id.clone());
                            }
                            self.preferred_id = Some(id.clone());
                            (format!("Moved {id} to {target}"), false)
                        }
                        Err(error) => (format!("Could not move {id} to {target}: {error}"), true),
                    };
                    self.notice = Some(message.clone());
                    self.toast = Some(Toast::result(message, error));
                    self.refresh_at(now);
                }
                Response::Closed(id, result) => {
                    self.mutation_busy = false;
                    match result {
                        Ok(()) => {
                            self.notice = Some(format!("Bead {id} closed"));
                            self.modal = None;
                            self.preferred_id = Some(id.clone());
                            self.refresh_at(now);
                        }
                        Err(error) => {
                            self.notice = Some(format!("Close failed for {id}: {error}"));
                            self.refresh_at(now);
                        }
                    }
                }
                Response::Deleted(result) => {
                    self.mutation_busy = false;
                    match result {
                        Ok(()) => {
                            self.notice = Some("Bead deleted".into());
                            self.modal = None;
                            self.refresh_at(now);
                        }
                        Err(e) => self.notice = Some(format!("Delete failed: {e}")),
                    }
                }
                Response::HarnessesDiscovered(seq, result) => {
                    if let Some(Modal::Create(form)) = self.modal.as_mut()
                        && form.harness_seq == seq
                    {
                        match result {
                            Ok(harnesses) => {
                                form.set_harnesses(harnesses, self.settings.default_agent)
                            }
                            Err(error) => {
                                form.set_harnesses(Vec::new(), self.settings.default_agent);
                                form.harness_error =
                                    Some(format!("Herdr integration discovery failed: {error}"));
                            }
                        }
                        if self.notice.as_deref().is_some_and(|message| {
                            message.starts_with("Checking Herdr integrations;")
                        }) {
                            self.notice = None;
                        }
                    }
                }
                Response::Discovered(seq, result) => {
                    if let Some(pending) = self
                        .handoff
                        .get(&seq)
                        .filter(|item| item.seq == seq && !item.launch_queued)
                        .cloned()
                    {
                        match result {
                            Ok(context) => {
                                let chosen = beads::harness(&pending.bead.data);
                                let agent = if let Some(kind) = chosen {
                                    context.agents.iter().find(|agent| agent.id == kind)
                                } else if pending.scheduled {
                                    match self.settings.default_agent {
                                        DefaultAgent::Kind(kind) => context
                                            .agents
                                            .iter()
                                            .find(|agent| agent.id == kind)
                                            .or_else(|| {
                                                context
                                                    .agents
                                                    .iter()
                                                    .find(|agent| agent.id == "codex")
                                            })
                                            .or_else(|| context.agents.first()),
                                        DefaultAgent::Automatic | DefaultAgent::Unassigned => {
                                            context
                                                .agents
                                                .iter()
                                                .find(|agent| agent.id == "codex")
                                                .or_else(|| context.agents.first())
                                        }
                                    }
                                } else {
                                    context
                                        .agents
                                        .iter()
                                        .find(|agent| agent.id == "codex")
                                        .or_else(|| context.agents.first())
                                }
                                .cloned();
                                if let Some(agent) = agent {
                                    let bead = pending.bead.clone();
                                    self.handoff.get_mut(&seq).unwrap().launch_queued = true;
                                    if seq == self.launch_seq {
                                        self.toast = Some(Toast::progress(format!(
                                            "Handing {} to {} in Herdr…",
                                            bead.id, agent.label
                                        )));
                                    }
                                    let request = if pending.scheduled {
                                        Request::LaunchReady(seq, context, agent, bead)
                                    } else {
                                        Request::Launch(seq, context, agent, bead)
                                    };
                                    if self.requests.send(request).is_err() {
                                        let message =
                                            "Handoff could not be queued; try again".to_owned();
                                        self.fail_handoff(seq, message);
                                    }
                                } else {
                                    let message = if let Some(kind) = chosen {
                                        format!(
                                            "Harness {kind} for {} is unavailable; change it or mark Ready to retry",
                                            pending.bead.id
                                        )
                                    } else if pending.scheduled {
                                        format!(
                                            "Selected harness for {} is unavailable; change it or mark Ready to retry",
                                            pending.bead.id
                                        )
                                    } else {
                                        "No Herdr agents are available in this workspace".to_owned()
                                    };
                                    self.fail_handoff(seq, message);
                                }
                            }
                            Err(error) => {
                                let message = format!("Handoff discovery failed: {error}");
                                self.fail_handoff(seq, message);
                            }
                        }
                    }
                }
                Response::Launched(seq, result) => {
                    if self
                        .handoff
                        .get(&seq)
                        .is_some_and(|item| item.launch_queued)
                    {
                        let ready_id = self
                            .handoff
                            .get(&seq)
                            .filter(|item| beads::is_ready(&item.bead))
                            .map(|item| item.bead.id.clone());
                        let bead_id = self.handoff[&seq].bead.id.clone();
                        match result {
                            Ok(outcome) => {
                                self.handoff.remove(&seq);
                                self.known_handoffs.insert(bead_id.clone());
                                if !outcome.reserved {
                                    self.handoff_capacity.insert(bead_id);
                                }
                                if let Some(id) = ready_id {
                                    self.known_handoffs.insert(id.clone());
                                    if outcome.reserved {
                                        self.scheduler_attempted.remove(&id);
                                    } else {
                                        self.accepted_handoffs.insert(id);
                                    }
                                }
                                self.handoff_feedback(seq, outcome.message, false);
                                // The launch journal is now active. Reload it even if the
                                // initial worktree scan found no managed handoffs.
                                if self.worktrees_inflight {
                                    self.worktrees_refresh_pending = true;
                                } else {
                                    self.worktrees_discovered = false;
                                    self.worktrees_next_refresh = now;
                                }
                            }
                            Err(error) => {
                                let message = format!("Handoff failed: {error}");
                                self.fail_handoff(seq, message);
                                // A failed launch can still have a durable partial
                                // intent/remote worktree. Always inspect it again.
                                if self.worktrees_inflight {
                                    self.worktrees_refresh_pending = true;
                                } else {
                                    self.worktrees_discovered = false;
                                    self.worktrees_next_refresh = now;
                                }
                            }
                        }
                    }
                }
                Response::Worktrees(result) => {
                    self.worktrees_inflight = false;
                    self.worktrees_loading = false;
                    self.worktrees_next_refresh = now + REFRESH_INTERVAL;
                    self.worktrees_discovered = result.is_ok();
                    if !self.worktree_action_pending {
                        match result {
                            Ok(mut items) => {
                                for item in &mut items {
                                    if matches!(
                                        item.status,
                                        worktrees::WorktreeStatus::Reserved
                                            | worktrees::WorktreeStatus::WorkspaceClosed
                                            | worktrees::WorktreeStatus::CheckoutRemoved
                                    ) {
                                        continue;
                                    }
                                    if let Some(previous) = self.worktrees.iter().find(|previous| {
                                        previous.id == item.id
                                            && previous.workspace_id == item.workspace_id
                                    }) && let Some(line) =
                                        previous.detail.as_deref().and_then(|detail| {
                                            detail.lines().find(|line| {
                                                line.starts_with(
                                                    "Agent observation (display only):",
                                                )
                                            })
                                        })
                                    {
                                        if item.status == worktrees::WorktreeStatus::InProgress
                                            && previous.status == worktrees::WorktreeStatus::Blocked
                                        {
                                            item.status = worktrees::WorktreeStatus::Blocked;
                                        }
                                        let base = item.detail.as_deref().unwrap_or("");
                                        item.detail = Some(if base.is_empty() {
                                            line.into()
                                        } else {
                                            format!("{base}\n{line}")
                                        });
                                    }
                                }
                                if !self.worktrees_refresh_pending {
                                    self.handoff_capacity
                                        .retain(|id| !items.iter().any(|item| &item.bead_id == id));
                                }
                                let selected_id = self
                                    .worktrees
                                    .get(self.selected_worktree)
                                    .map(|item| item.id.clone());
                                let accepted_handoffs: HashSet<_> = items
                                    .iter()
                                    .filter(|item| item.handoff_accepted)
                                    .map(|item| item.bead_id.clone())
                                    .collect();
                                if self.worktrees_refresh_pending {
                                    // This scan started before the successful handoff.
                                    self.accepted_handoffs.extend(accepted_handoffs);
                                } else {
                                    self.accepted_handoffs = accepted_handoffs;
                                }
                                self.known_handoffs
                                    .extend(items.iter().map(|item| item.bead_id.clone()));
                                items.retain(|item| {
                                    item.status != worktrees::WorktreeStatus::CheckoutRemoved
                                });
                                order_worktrees(&mut items);
                                self.worktrees = items;
                                self.selected_worktree = selected_id
                                    .as_deref()
                                    .and_then(|id| {
                                        self.worktrees.iter().position(|item| item.id == id)
                                    })
                                    .unwrap_or(self.selected_worktree)
                                    .min(self.worktrees.len().saturating_sub(1));
                                if selected_id.as_deref()
                                    != self
                                        .worktrees
                                        .get(self.selected_worktree)
                                        .map(|item| item.id.as_str())
                                {
                                    self.worktree_detail_offset = 0;
                                }
                                self.worktrees_error = None;
                            }
                            Err(error) => self.worktrees_error = Some(error.to_string()),
                        }
                    }
                    if std::mem::take(&mut self.worktrees_refresh_pending) {
                        self.worktrees_discovered = false;
                        self.worktrees_next_refresh = now;
                    }
                }
                Response::AgentsObserved(result) => {
                    if !self.agent_observation_inflight {
                        continue;
                    }
                    self.agent_observation_inflight = false;
                    self.next_agent_observation = now + Duration::from_secs(2);
                    for item in &mut self.worktrees {
                        if matches!(
                            item.status,
                            worktrees::WorktreeStatus::Reserved
                                | worktrees::WorktreeStatus::WorkspaceClosed
                                | worktrees::WorktreeStatus::CheckoutRemoved
                        ) {
                            continue;
                        }
                        let observation = match &result {
                            Ok(observations) => observations.iter().find(|observation| {
                                observation.id == item.id
                                    && observation.workspace_id == item.workspace_id
                            }),
                            Err(_) => None,
                        };
                        if matches!(
                            item.status,
                            worktrees::WorktreeStatus::InProgress
                                | worktrees::WorktreeStatus::Blocked
                        ) {
                            item.status =
                                if observation.is_some_and(|observation| observation.blocked) {
                                    worktrees::WorktreeStatus::Blocked
                                } else {
                                    worktrees::WorktreeStatus::InProgress
                                };
                        }
                        let line = match &result {
                            Ok(_) => observation.map(|observation| observation.detail.clone()),
                            Err(error) => Some(format!(
                                "Agent observation (display only): unknown — poll failed: {error}"
                            )),
                        };
                        if let Some(line) = line {
                            let existing = item.detail.as_deref().unwrap_or("");
                            let base = existing
                                .split("\nAgent observation (display only):")
                                .next()
                                .unwrap_or("");
                            let base = if base.starts_with("Agent observation (display only):") {
                                ""
                            } else {
                                base
                            };
                            item.detail = Some(if base.is_empty() {
                                line
                            } else {
                                format!("{base}\n{line}")
                            });
                        }
                    }
                }
                Response::WorkspaceFocused(id, result) => {
                    self.worktrees_focus_pending = false;
                    self.worktrees_focus_feedback = Some(match result {
                        Ok(()) => WorktreeFocusFeedback::Success(format!(
                            "Switched to Herdr workspace {id}"
                        )),
                        Err(error) => WorktreeFocusFeedback::Error(format!(
                            "Could not switch to Herdr workspace {id}: {error}"
                        )),
                    });
                }
                Response::WorktreeChanged(result) => {
                    self.mutation_busy = false;
                    self.modal = None;
                    self.worktree_action_pending = false;
                    match result {
                        Ok(item) => {
                            if item.handoff_accepted {
                                self.accepted_handoffs.insert(item.bead_id.clone());
                            }
                            if let Some(index) = self
                                .worktrees
                                .iter()
                                .position(|existing| existing.id == item.id)
                            {
                                if item.status == worktrees::WorktreeStatus::CheckoutRemoved {
                                    self.worktrees.remove(index);
                                    self.selected_worktree = self
                                        .selected_worktree
                                        .min(self.worktrees.len().saturating_sub(1));
                                    if self.selected_worktree >= index {
                                        self.worktree_detail_offset = 0;
                                    }
                                } else {
                                    self.worktrees[index] = item.clone();
                                }
                            }
                            self.notice = Some(format!(
                                "{} · {}",
                                item.branch,
                                worktree_status_label(item.status)
                            ));
                            self.worktrees_next_refresh = now + REFRESH_INTERVAL;
                        }
                        Err(e) => {
                            self.worktrees_error = Some(e.to_string());
                            self.worktrees_next_refresh = now;
                        }
                    }
                }
                Response::WorktreeRestarted(result) => {
                    self.mutation_busy = false;
                    self.modal = None;
                    self.worktree_action_pending = false;
                    self.worktrees_focus_feedback = Some(match result {
                        Ok(message) => WorktreeFocusFeedback::Success(message),
                        Err(error) => {
                            WorktreeFocusFeedback::Error(format!("Restart failed: {error}"))
                        }
                    });
                    if self.worktrees_inflight {
                        self.worktrees_refresh_pending = true;
                    } else {
                        self.worktrees_discovered = false;
                        self.worktrees_next_refresh = now;
                    }
                }
                Response::WorktreeContinued(result) => {
                    self.mutation_busy = false;
                    self.modal = None;
                    self.worktree_action_pending = false;
                    self.worktrees_focus_feedback = Some(match result {
                        Ok(message) => WorktreeFocusFeedback::Success(message),
                        Err(error) => {
                            WorktreeFocusFeedback::Error(format!("Continue failed: {error}"))
                        }
                    });
                    if self.worktrees_inflight {
                        self.worktrees_refresh_pending = true;
                    } else {
                        self.worktrees_discovered = false;
                        self.worktrees_next_refresh = now;
                    }
                }
            }
        }
        received
    }
    fn input(&mut self, input: Event) -> bool {
        match input {
            Event::Key(key) => return self.key(key),
            Event::Mouse(mouse) => self.mouse(mouse.kind, mouse.column, mouse.row),
            Event::Paste(_) => self.easter_egg_progress = 0,
            _ => {}
        }
        false
    }
    pub(crate) fn key(&mut self, k: KeyEvent) -> bool {
        if self.help.is_some()
            || self.modal.is_some()
            || (self.mutation_busy && !self.ready_pending)
        {
            self.easter_egg_progress = 0;
        }
        if k.kind == KeyEventKind::Release {
            return false;
        }
        if self.easter_egg {
            self.easter_egg_progress = 0;
            if k.code == KeyCode::Esc {
                self.easter_egg = false;
            }
            return event::is_quit(k);
        }
        if self.mutation_busy && !self.ready_pending {
            return false;
        }
        if let Some(help) = self.help.as_mut() {
            if hotkeys::is_help_key(k) || k.code == KeyCode::Esc {
                self.help = None;
            } else {
                match k.code {
                    KeyCode::Up | KeyCode::Char('k') => help.offset = help.offset.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        help.offset = help.offset.saturating_add(1)
                    }
                    KeyCode::PageUp => help.offset = help.offset.saturating_sub(10),
                    KeyCode::PageDown => help.offset = help.offset.saturating_add(10),
                    KeyCode::Home => help.offset = 0,
                    KeyCode::End => help.offset = usize::MAX,
                    _ => {}
                }
            }
            return false;
        }
        if hotkeys::is_help_key(k) {
            self.easter_egg_progress = 0;
            self.help = Some(Help {
                context: hotkeys::Context::current(self),
                offset: 0,
            });
            return false;
        }
        if self.modal.is_some() {
            return self.modal_key(k);
        }
        if event::is_quit(k) {
            return true;
        }
        if k.kind == KeyEventKind::Press {
            use KeyCode::*;
            const SEQUENCE: [KeyCode; 10] = [
                Up,
                Up,
                Down,
                Down,
                Left,
                Right,
                Left,
                Right,
                Char('b'),
                Char('a'),
            ];
            self.easter_egg_progress = if !k.modifiers.is_empty() {
                0
            } else if k.code == SEQUENCE[self.easter_egg_progress] {
                self.easter_egg_progress + 1
            } else if k.code == Up {
                // Only the leading Up pair can overlap a mismatched prefix.
                if self.easter_egg_progress == 2 { 2 } else { 1 }
            } else {
                0
            };
            if self.easter_egg_progress == SEQUENCE.len() {
                self.easter_egg_progress = 0;
                self.easter_egg = true;
                return false;
            }
        }
        // Keep browsing available during Ready and moves, but exclude mutations
        // and handoffs against a partially updated snapshot.
        if self.ready_pending
            && matches!(
                k.code,
                KeyCode::Char(
                    'n' | 'N'
                        | 'e'
                        | 'd'
                        | 'g'
                        | 'h'
                        | 'p'
                        | 's'
                        | 'u'
                        | 'm'
                        | 'c'
                        | 'x'
                        | 'R'
                        | 'a'
                )
            )
        {
            return false;
        }
        if self.worktrees_view {
            match k.code {
                KeyCode::Char('w') | KeyCode::Esc | KeyCode::Backspace => {
                    self.set_drawer(true, false, Instant::now());
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected_worktree = self.selected_worktree.saturating_sub(1);
                    self.worktree_detail_offset = 0;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.selected_worktree =
                        (self.selected_worktree + 1).min(self.worktrees.len().saturating_sub(1));
                    self.worktree_detail_offset = 0;
                }
                KeyCode::PageUp => {
                    self.worktree_detail_offset = self
                        .worktree_detail_offset
                        .saturating_sub(self.detail_rect.height.saturating_sub(2));
                }
                KeyCode::PageDown => {
                    self.worktree_detail_offset = self
                        .worktree_detail_offset
                        .saturating_add(self.detail_rect.height.saturating_sub(2));
                }
                KeyCode::Char('r') => {
                    self.worktrees_next_refresh = Instant::now();
                    self.poll_worktrees(Instant::now());
                }
                KeyCode::Enter => self.focus_selected_worktree(),
                KeyCode::Char('m') => self.confirm_worktree_action(0),
                KeyCode::Char('c') => self.confirm_worktree_action(1),
                KeyCode::Char('x') => self.confirm_worktree_action(2),
                KeyCode::Char('X') => self.confirm_worktree_action(5),
                KeyCode::Char('R') => self.confirm_worktree_action(3),
                KeyCode::Char('a') => self.confirm_worktree_action(4),
                _ => {}
            }
            return false;
        }
        match k.code {
            KeyCode::Char('w') => {
                self.set_drawer(true, true, Instant::now());
                if let Some(index) =
                    self.selected
                        .and_then(|i| self.beads.get(i))
                        .and_then(|bead| {
                            self.worktrees
                                .iter()
                                .position(|item| item.bead_id == bead.id)
                        })
                {
                    self.selected_worktree = index;
                    self.worktree_detail_offset = 0;
                }
                self.worktrees_focus_feedback = None;
                self.worktrees_next_refresh = Instant::now();
                self.poll_worktrees(Instant::now());
            }
            KeyCode::Char('r' | 'R') => self.toggle_ready(),
            KeyCode::Char('s') => {
                let load_error = self
                    .settings_load_error
                    .as_ref()
                    .map(|error| format!("Settings were not loaded; defaults are active. {error}"));
                self.modal = Some(Modal::Settings {
                    draft: self.settings,
                    error: load_error,
                    dropdown_open: false,
                    focus: 0,
                });
                self.modal_ready = false;
            }
            KeyCode::Char('m') if !self.focus_detail => {
                if let Some(bead) = self.selected.and_then(|i| self.beads.get(i)) {
                    self.modal = Some(Modal::Move {
                        id: bead.id.clone(),
                        title: bead.title.clone(),
                        selected: status_order(display_status(bead)),
                    });
                    self.modal_ready = false;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let child = k.code == KeyCode::Char('N')
                    || (k.code == KeyCode::Char('n')
                        && k.modifiers.contains(crossterm::event::KeyModifiers::SHIFT));
                let parent_id = if child {
                    match self.selected.and_then(|i| self.beads.get(i)) {
                        Some(bead)
                            if bead.data["issue_type"] == "task" && bead.status != "closed" =>
                        {
                            Some(bead.id.clone())
                        }
                        _ => {
                            self.notice = Some("Select an open task to create a child".into());
                            return false;
                        }
                    }
                } else {
                    None
                };
                self.harness_seq = self.harness_seq.wrapping_add(1);
                let seq = self.harness_seq;
                self.modal = Some(Modal::Create(CreateForm {
                    parent_id,
                    harness_loading: true,
                    harness_seq: seq,
                    ..CreateForm::default()
                }));
                self.modal_ready = false;
                self.notice = None;
                if self.requests.send(Request::DiscoverHarnesses(seq)).is_err()
                    && let Some(Modal::Create(form)) = self.modal.as_mut()
                {
                    form.harness_loading = false;
                    form.harness_error =
                        Some("Herdr integration discovery could not be queued".into());
                }
            }
            KeyCode::Char('e') => {
                if let Some(b) = self.selected.and_then(|i| self.beads.get(i)) {
                    let id = b.id.clone();
                    self.edit_seq = self.edit_seq.wrapping_add(1);
                    let seq = self.edit_seq;
                    self.modal = Some(Modal::EditLoading {
                        id: id.clone(),
                        seq,
                        error: None,
                    });
                    self.modal_ready = false;
                    self.notice = None;
                    if self.requests.send(Request::LoadEdit(id, seq)).is_err()
                        && let Some(Modal::EditLoading { error, .. }) = self.modal.as_mut()
                    {
                        *error = Some("Edit could not be queued; try again".into());
                    }
                }
            }
            KeyCode::Char('c') if k.modifiers.is_empty() => {
                if let Some(b) = self.selected.and_then(|i| self.beads.get(i))
                    && b.status != "closed"
                {
                    self.modal = Some(Modal::ConfirmClose {
                        id: b.id.clone(),
                        title: b.title.clone(),
                    });
                    self.modal_ready = false;
                    self.notice = None;
                }
            }
            KeyCode::Char('d') => {
                if let Some(b) = self.selected.and_then(|i| self.beads.get(i)) {
                    self.modal = Some(Modal::ConfirmDelete {
                        id: b.id.clone(),
                        title: b.title.clone(),
                    });
                    self.modal_ready = false;
                    self.notice = None;
                }
            }
            KeyCode::Char('h') => self.open_launch(),
            KeyCode::Esc if self.focus_detail => {
                self.set_drawer(false, false, Instant::now());
            }
            KeyCode::Tab if self.focus_detail || self.selected.is_some() => {
                self.set_drawer(false, !self.focus_detail, Instant::now());
            }
            KeyCode::Enter if self.focus_detail => {
                self.set_drawer(false, false, Instant::now());
            }
            KeyCode::Enter if self.selected.is_some() => {
                self.set_drawer(false, true, Instant::now());
            }
            KeyCode::Left | KeyCode::Right
                if !self.focus_detail && k.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                self.move_selected(if k.code == KeyCode::Right { 1 } else { -1 });
            }
            KeyCode::Left if !self.focus_detail => {
                self.focus_column(self.focused_column.saturating_sub(1));
            }
            KeyCode::Right if !self.focus_detail => {
                self.focus_column(self.focused_column.saturating_add(1));
            }
            KeyCode::Up if self.focus_detail => {
                self.detail_offset = self.detail_offset.saturating_sub(1)
            }
            KeyCode::Down if self.focus_detail => {
                self.detail_offset = self.detail_offset.saturating_add(1)
            }
            KeyCode::PageUp if self.focus_detail => {
                self.detail_offset = self
                    .detail_offset
                    .saturating_sub(self.detail_rect.height.saturating_sub(2))
            }
            KeyCode::PageDown if self.focus_detail => {
                self.detail_offset = self
                    .detail_offset
                    .saturating_add(self.detail_rect.height.saturating_sub(2))
            }
            KeyCode::Up => self.step(-1),
            KeyCode::Down => self.step(1),
            _ => {}
        }
        false
    }
    fn focus_selected_worktree(&mut self) {
        if self.worktrees_focus_pending {
            return;
        }
        let Some(item) = self.worktrees.get(self.selected_worktree) else {
            self.worktrees_focus_feedback = Some(WorktreeFocusFeedback::Error(
                "Select a managed worktree first".into(),
            ));
            return;
        };
        if matches!(
            item.status,
            worktrees::WorktreeStatus::WorkspaceClosed | worktrees::WorktreeStatus::CheckoutRemoved
        ) {
            self.worktrees_focus_feedback = Some(WorktreeFocusFeedback::Error(
                "This worktree's Herdr workspace is closed".into(),
            ));
            return;
        }
        self.worktrees_focus_feedback = None;
        self.worktrees_focus_pending = true;
        if self
            .requests
            .send(Request::FocusWorkspace(item.workspace_id.clone()))
            .is_err()
        {
            self.worktrees_focus_pending = false;
            self.worktrees_focus_feedback = Some(WorktreeFocusFeedback::Error(
                "Workspace switch could not be queued".into(),
            ));
        }
    }
    fn confirm_worktree_action(&mut self, action: u8) {
        let Some(item) = self.worktrees.get(self.selected_worktree) else {
            self.worktrees_error = Some("Select a managed worktree first".into());
            return;
        };
        if action == 1 && item.status != worktrees::WorktreeStatus::Integrated {
            self.worktrees_error =
                Some("Cleanup is available only after verified integration".into());
            return;
        }
        if action == 2
            && !matches!(
                item.status,
                worktrees::WorktreeStatus::Integrated | worktrees::WorktreeStatus::WorkspaceClosed
            )
        {
            self.worktrees_error = Some("Checkout removal requires verified integration".into());
            return;
        }
        if action == 0
            && !matches!(
                item.status,
                worktrees::WorktreeStatus::InProgress
                    | worktrees::WorktreeStatus::Blocked
                    | worktrees::WorktreeStatus::Unmerged
                    | worktrees::WorktreeStatus::MergeQueued
                    | worktrees::WorktreeStatus::NeedsResolution
            )
        {
            self.worktrees_error = Some("This worktree is not ready for a merge request".into());
            return;
        }
        if matches!(action, 3 | 5)
            && matches!(
                item.status,
                worktrees::WorktreeStatus::CheckoutRemoved
                    | worktrees::WorktreeStatus::WorkspaceClosed
            )
        {
            self.worktrees_error =
                Some("Select an active checkout to restart or force remove".into());
            return;
        }
        if action == 4
            && !matches!(
                item.status,
                worktrees::WorktreeStatus::InProgress | worktrees::WorktreeStatus::DirtyCheckout
            )
        {
            self.worktrees_error = Some("Select an active task checkout to continue".into());
            return;
        }
        self.modal = Some(Modal::ConfirmWorktree {
            id: item.id.clone(),
            action,
        });
        self.modal_ready = false;
    }
    fn planning_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        let Some(Modal::Planning(form)) = self.modal.as_mut() else {
            return;
        };
        if key.code == Esc {
            if form.session.is_some() {
                self.notice = Some(format!(
                    "Planning cancelled; Herdr tab remains open for review ({})",
                    form.session.as_ref().map_or("", |session| &session.tab_id)
                ));
            }
            self.modal = None;
            return;
        }
        match &form.stage {
            PlanningStage::Draft => match key.code {
                Enter if key.modifiers.contains(KeyModifiers::SHIFT) => form.proposal.push('\n'),
                Enter if !form.proposal.trim().is_empty() => {
                    let request =
                        Request::PlanLaunch(form.seq, form.proposal.clone(), form.marker.clone());
                    if self.requests.send(request).is_ok() {
                        form.stage = PlanningStage::Waiting;
                    } else {
                        form.stage =
                            PlanningStage::Failed("Planning launch could not be queued".into());
                    }
                }
                Backspace => {
                    form.proposal.pop();
                }
                Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => form.proposal.push(c),
                _ => {}
            },
            PlanningStage::Question { options, .. } => {
                let options = options.clone();
                match key.code {
                    Up => form.selected = form.selected.saturating_sub(1),
                    Down => form.selected = (form.selected + 1).min(options.len()),
                    Tab => form.selected = options.len(),
                    Backspace if form.selected == options.len() => {
                        form.input.pop();
                    }
                    Char(c)
                        if form.selected == options.len()
                            && !key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        form.input.push(c)
                    }
                    Enter => {
                        let answer = if form.selected == options.len() {
                            form.input.trim().to_owned()
                        } else {
                            options[form.selected].clone()
                        };
                        if !answer.is_empty() {
                            Self::submit_planning_reply(form, &self.requests, answer);
                        }
                    }
                    _ => {}
                }
            }
            PlanningStage::Review(plan) => match key.code {
                Char('y' | 'Y') => {
                    let request = Request::PlanCreate(form.seq, plan.clone());
                    if self.requests.send(request).is_ok() {
                        form.stage = PlanningStage::Creating;
                        self.mutation_busy = true;
                    } else {
                        form.stage =
                            PlanningStage::Failed("Bead creation could not be queued".into());
                    }
                }
                Char('r') => {
                    form.input.clear();
                    form.stage = PlanningStage::Revision;
                }
                Up => form.scroll = form.scroll.saturating_sub(1),
                Down => form.scroll = form.scroll.saturating_add(1),
                PageUp => form.scroll = form.scroll.saturating_sub(10),
                PageDown => form.scroll = form.scroll.saturating_add(10),
                _ => {}
            },
            PlanningStage::Revision => match key.code {
                Enter if key.modifiers.contains(KeyModifiers::SHIFT) => form.input.push('\n'),
                Enter if !form.input.trim().is_empty() => {
                    Self::submit_planning_reply(form, &self.requests, form.input.trim().to_owned());
                }
                Backspace => {
                    form.input.pop();
                }
                Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => form.input.push(c),
                _ => {}
            },
            PlanningStage::Failed(_) if key.code == Char('r') && form.session.is_some() => {
                form.stage = PlanningStage::Waiting;
                form.next_poll = Instant::now();
            }
            _ => {}
        }
    }

    fn submit_planning_reply(
        form: &mut PlanningForm,
        requests: &mpsc::Sender<Request>,
        answer: String,
    ) {
        let Some(session) = form.session.clone() else {
            form.stage = PlanningStage::Failed("Planning agent session is unavailable".into());
            return;
        };
        form.round = form.round.saturating_add(1);
        form.marker = format!("{}_{}", form.marker, form.round);
        if requests
            .send(Request::PlanReply(
                form.seq,
                session,
                answer,
                form.marker.clone(),
            ))
            .is_ok()
        {
            form.stage = PlanningStage::Waiting;
            form.poll_inflight = false;
            form.next_poll = Instant::now() + Duration::from_secs(2);
        } else {
            form.stage = PlanningStage::Failed("Planning reply could not be queued".into());
        }
    }

    fn modal_key(&mut self, key: KeyEvent) -> bool {
        use crossterm::event::KeyCode::*;
        if self.mutation_busy {
            return false;
        }
        if !self.modal_ready && key.code != Esc {
            return false;
        }
        if matches!(self.modal, Some(Modal::Planning(_))) {
            self.planning_key(key);
            return false;
        }
        if key.code == Esc {
            if let Some(Modal::Settings { dropdown_open, .. }) = self.modal.as_mut() {
                if *dropdown_open {
                    *dropdown_open = false;
                } else {
                    self.modal = None;
                }
                return false;
            }
            if let Some(Modal::Create(form) | Modal::Edit { form, .. }) = self.modal.as_mut()
                && let Some(which) = form.dropdown.take()
            {
                if let Some(old) = form.dropdown_original.take() {
                    match which {
                        2 => form.issue_type = old,
                        3 => form.priority = old,
                        _ => form.harness = old,
                    }
                }
                return false;
            }
            for path in self.created_screenshots.drain(..) {
                let _ = std::fs::remove_file(path);
            }
            self.modal = None;
            self.paste_seq = self.paste_seq.wrapping_add(1);
            self.paste_pending = false;
            return false;
        }
        if self.paste_pending {
            self.notice = Some("Wait for screenshot paste or press Esc to cancel".into());
            return false;
        }
        if let Some(Modal::Move {
            id,
            title,
            selected,
        }) = self.modal.as_mut()
        {
            match key.code {
                Up => *selected = selected.saturating_sub(1),
                Down => *selected = (*selected + 1).min(BOARD_STATUSES.len() - 1),
                Enter => {
                    let (id, title, target) = (id.clone(), title.clone(), *selected);
                    self.move_bead(id, title, target);
                }
                _ => {}
            }
            return false;
        }
        if let Some(Modal::ConfirmWorktree { id, action }) = self.modal.as_ref() {
            if matches!(key.code, Char('y') | Char('Y')) {
                let id = id.clone();
                let request = match action {
                    0 => Request::WorktreeAction(id, WorktreeAction::RequestMerge),
                    1 => Request::WorktreeAction(id, WorktreeAction::CloseWorkspace),
                    2 => Request::WorktreeAction(id, WorktreeAction::RemoveCheckout),
                    3 => Request::RestartWorktree(id),
                    5 => Request::WorktreeAction(id, WorktreeAction::ForceRemoveCheckout),
                    _ => Request::ContinueWorktree(id),
                };
                self.mutation_busy = true;
                if self.requests.send(request).is_err() {
                    self.mutation_busy = false;
                    self.worktrees_error = Some("Action could not be queued".into());
                } else {
                    self.worktree_action_pending = true;
                }
            } else if matches!(key.code, Char('n') | Char('N') | Enter) {
                self.modal = None;
            }
            return false;
        }
        if matches!(self.modal, Some(Modal::Settings { .. })) {
            let mut save_settings = None;
            if let Some(Modal::Settings {
                draft,
                dropdown_open,
                focus,
                ..
            }) = self.modal.as_mut()
            {
                match key.code {
                    Tab | BackTab => {
                        *dropdown_open = false;
                        *focus = (*focus + if key.code == Tab { 1 } else { 8 }) % 9;
                    }
                    Up | Down => {
                        *focus = (*focus + if key.code == Down { 1 } else { 8 }) % 9;
                        *dropdown_open = false;
                    }
                    Left | Right if *focus == 0 => {
                        draft.theme = cycle_theme(draft.theme, key.code == Right);
                        save_settings = Some(*draft);
                    }
                    Left | Right if *focus == 1 => {
                        let count = beads::HARNESSES.len() + 2;
                        let index =
                            cycle_choice(draft.default_agent.index(), count, key.code == Right);
                        draft.default_agent = DefaultAgent::at(index);
                        save_settings = Some(*draft);
                    }
                    Left | Right if *focus == 2 => {
                        draft.agent_limit = draft.agent_limit.step(key.code == Right);
                        save_settings = Some(*draft);
                    }
                    Enter | Char(' ') if *focus <= 2 && *dropdown_open => {
                        *dropdown_open = false;
                        save_settings = Some(*draft);
                    }
                    Enter | Char(' ') if *focus <= 2 => {
                        *dropdown_open = true;
                    }
                    Enter | Char(' ') if (3..=7).contains(focus) => {
                        match *focus {
                            3 => draft.auto_merge = !draft.auto_merge,
                            4 => draft.auto_merge_children = !draft.auto_merge_children,
                            5 => draft.auto_close_workspace = !draft.auto_close_workspace,
                            6 => draft.auto_remove_checkout = !draft.auto_remove_checkout,
                            _ => draft.animate_drawers = !draft.animate_drawers,
                        }
                        save_settings = Some(*draft);
                    }
                    Enter | Char(' ') if *focus == 8 => {
                        self.modal = None;
                    }
                    _ => {}
                }
            }
            if let Some(settings) = save_settings {
                self.save_settings(settings);
                if !self.settings.animate_drawers {
                    self.drawer_animation = None;
                }
            }
            return false;
        }
        if let Some(Modal::ConfirmGenerateEpic { id, .. }) = self.modal.as_ref() {
            if matches!(key.code, Char('y') | Char('Y')) {
                let id = id.clone();
                if !self.planning_epics.contains(&id) {
                    if self
                        .requests
                        .send(Request::GenerateEpic(id.clone()))
                        .is_ok()
                    {
                        self.planning_epics.insert(id.clone());
                        self.modal = None;
                        self.notice = Some(format!("Starting child-task planning for {id}…"));
                        self.toast = Some(Toast::progress(format!(
                            "Starting child-task planning for {id}…"
                        )));
                    } else {
                        self.notice = Some("Epic planning could not be queued; try again".into());
                    }
                }
            } else if matches!(key.code, Char('n') | Char('N') | Enter) {
                self.modal = None;
            }
            return false;
        }
        if let Some(Modal::ConfirmClose { id, .. }) = self.modal.as_ref() {
            if matches!(key.code, Char('y') | Char('Y')) {
                let id = id.clone();
                self.mutation_busy = true;
                if self.requests.send(Request::Close(id)).is_err() {
                    self.mutation_busy = false;
                    self.notice = Some("Close could not be queued; try again".into());
                }
            } else if matches!(key.code, Char('n') | Char('N') | Enter) {
                self.modal = None;
            }
            return false;
        }
        if let Some(Modal::ConfirmDelete { id, .. }) = self.modal.as_ref() {
            if matches!(key.code, Char('y') | Char('Y')) && !self.mutation_busy {
                let id = id.clone();
                self.mutation_busy = true;
                if self.requests.send(Request::Delete(id)).is_err() {
                    self.mutation_busy = false;
                    self.notice = Some("Delete could not be queued; try again".into());
                }
            }
            if matches!(key.code, Char('n') | Char('N') | Enter) {
                self.modal = None;
            }
            return false;
        }
        if let Some(Modal::EditLoading { id, seq, error }) = self.modal.as_mut() {
            if key.code == Char('e') && error.is_some() {
                let id = id.clone();
                self.edit_seq = self.edit_seq.wrapping_add(1);
                *seq = self.edit_seq;
                *error = None;
                if self.requests.send(Request::LoadEdit(id, *seq)).is_err()
                    && let Some(Modal::EditLoading { error, .. }) = self.modal.as_mut()
                {
                    *error = Some("Edit could not be queued; try again".into());
                }
            }
            return false;
        }
        let edit_id = match self.modal.as_ref() {
            Some(Modal::Edit { id, .. }) => Some(id.clone()),
            _ => None,
        };
        let Some(Modal::Create(form) | Modal::Edit { form, .. }) = self.modal.as_mut() else {
            return false;
        };
        if let Some(which) = form.dropdown {
            match key.code {
                Up => {
                    if which == 2 {
                        form.issue_type = form.issue_type.saturating_sub(1)
                    } else if which == 3 {
                        form.priority = form.priority.saturating_sub(1)
                    } else {
                        form.harness = form.harness.saturating_sub(1)
                    }
                }
                Down => {
                    if which == 2 {
                        form.issue_type = (form.issue_type + 1).min(beads::BEAD_TYPES.len() - 1)
                    } else if which == 3 {
                        form.priority = (form.priority + 1).min(4)
                    } else {
                        form.harness = (form.harness + 1).min(form.harnesses.len())
                    }
                }
                Left | Right => {
                    if which == 2 {
                        form.issue_type = cycle_choice(
                            form.issue_type,
                            beads::BEAD_TYPES.len(),
                            key.code == Right,
                        );
                    } else if which == 3 {
                        form.priority = cycle_choice(form.priority, 5, key.code == Right);
                    } else {
                        form.harness =
                            cycle_choice(form.harness, form.harnesses.len() + 1, key.code == Right);
                    }
                }
                Enter | Char(' ') => {
                    form.dropdown = None;
                    form.dropdown_original = None;
                }
                _ => {}
            }
            return false;
        }
        if form.focus == 1 && !matches!(key.code, Up | Down) {
            form.description_preferred_col = None;
        }
        if key.code == Char('v') && key.modifiers.contains(KeyModifiers::CONTROL) && form.focus == 1
        {
            self.paste_seq = self.paste_seq.wrapping_add(1);
            self.paste_pending = true;
            if self
                .requests
                .send(Request::ReadScreenshot(self.paste_seq))
                .is_err()
            {
                self.paste_pending = false;
                self.notice = Some("Screenshot paste could not be queued".into());
            }
            return false;
        }
        match key.code {
            Tab => {
                form.focus = match form.focus {
                    0 => 2,
                    2 => 3,
                    3 => 6,
                    6 => 1,
                    1 => 4,
                    4 => 5,
                    _ => 0,
                }
            }
            BackTab => {
                form.focus = match form.focus {
                    0 => 5,
                    5 => 4,
                    4 => 1,
                    1 => 6,
                    6 => 3,
                    3 => 2,
                    _ => 0,
                }
            }
            Enter if matches!(form.focus, 2 | 3 | 6) => {
                form.dropdown = Some(form.focus);
                form.dropdown_original = Some(match form.focus {
                    2 => form.issue_type,
                    3 => form.priority,
                    _ => form.harness,
                });
            }
            Left | Right if form.focus == 2 => {
                form.issue_type =
                    cycle_choice(form.issue_type, beads::BEAD_TYPES.len(), key.code == Right);
            }
            Left | Right if form.focus == 3 => {
                form.priority = cycle_choice(form.priority, 5, key.code == Right);
            }
            Left | Right if form.focus == 6 => {
                form.harness =
                    cycle_choice(form.harness, form.harnesses.len() + 1, key.code == Right);
            }
            Enter if form.focus == 4 => {
                if !form.title.trim().is_empty() {
                    if edit_id.is_none() && form.harness_loading {
                        self.notice = Some(
                            "Checking Herdr integrations; wait before creating an issue".into(),
                        );
                        return false;
                    }
                    let input = CreateBead {
                        title: form.title.trim().into(),
                        description: form.description.clone(),
                        issue_type: beads::BEAD_TYPES[form.issue_type].into(),
                        priority: form.priority as u8,
                        harness: form
                            .harness
                            .checked_sub(1)
                            .map(|index| form.harnesses[index].clone()),
                    };
                    self.mutation_busy = true;
                    let request = if let Some(id) = edit_id {
                        Request::Update(UpdateBead {
                            id,
                            fields: input,
                            previous_harness: form.previous_harness.clone(),
                        })
                    } else if let Some(parent) = &form.parent_id {
                        Request::CreateChild(input, parent.clone())
                    } else {
                        Request::Create(input)
                    };
                    if self.requests.send(request).is_err() {
                        self.mutation_busy = false;
                        self.notice = Some("Save could not be queued; try again".into());
                    }
                } else {
                    self.notice = Some("Title is required".into());
                }
            }
            Enter if form.focus == 5 => {
                self.modal = None;
                return false;
            }
            Char(' ') if matches!(form.focus, 2 | 3 | 6) => {
                form.dropdown = Some(form.focus);
                form.dropdown_original = Some(match form.focus {
                    2 => form.issue_type,
                    3 => form.priority,
                    _ => form.harness,
                });
            }
            Enter if form.focus == 0 => {
                form.focus = 1;
                form.description_cursor = form.description.len();
            }
            Enter if form.focus == 1 && key.modifiers == crossterm::event::KeyModifiers::SHIFT => {
                let i = form.description_cursor.min(form.description.len());
                form.description.insert(i, '\n');
                form.description_cursor = i + 1;
            }
            Enter if form.focus == 1 && key.modifiers.is_empty() => {
                form.focus = 4;
            }
            Up if form.focus == 1 => {
                form.description_cursor = crate::ui::move_description_vertical(
                    &form.description,
                    form.description_cursor,
                    false,
                    &mut form.description_preferred_col,
                )
            }
            Down if form.focus == 1 => {
                form.description_cursor = crate::ui::move_description_vertical(
                    &form.description,
                    form.description_cursor,
                    true,
                    &mut form.description_preferred_col,
                )
            }
            Left if form.focus == 0 => form.cursor = previous_boundary(&form.title, form.cursor),
            Right if form.focus == 0 => form.cursor = next_boundary(&form.title, form.cursor),
            Home if form.focus == 0 => form.cursor = 0,
            End if form.focus == 0 => form.cursor = form.title.len(),
            Left if form.focus == 1 => {
                form.description_cursor =
                    previous_boundary(&form.description, form.description_cursor)
            }
            Right if form.focus == 1 => {
                form.description_cursor = next_boundary(&form.description, form.description_cursor)
            }
            Home if form.focus == 1 => {
                form.description_cursor = form.description
                    [..form.description_cursor.min(form.description.len())]
                    .rfind('\n')
                    .map_or(0, |i| i + 1)
            }
            End if form.focus == 1 => {
                form.description_cursor = form.description
                    [form.description_cursor.min(form.description.len())..]
                    .find('\n')
                    .map_or(form.description.len(), |i| form.description_cursor + i)
            }
            Backspace if form.focus == 1 && form.description_cursor > 0 => {
                let p = previous_boundary(&form.description, form.description_cursor);
                form.description.drain(p..form.description_cursor);
                form.description_cursor = p;
            }
            Delete if form.focus == 1 && form.description_cursor < form.description.len() => {
                let n = next_boundary(&form.description, form.description_cursor);
                form.description.drain(form.description_cursor..n);
            }
            Backspace if form.focus == 0 && form.cursor > 0 => {
                let p = previous_boundary(&form.title, form.cursor);
                form.title.drain(p..form.cursor);
                form.cursor = p;
            }
            Delete if form.focus == 0 && form.cursor < form.title.len() => {
                let n = next_boundary(&form.title, form.cursor);
                form.title.drain(form.cursor..n);
            }
            Char(c)
                if !key.modifiers.intersects(
                    crossterm::event::KeyModifiers::CONTROL
                        | crossterm::event::KeyModifiers::ALT
                        | crossterm::event::KeyModifiers::SUPER,
                ) && form.focus == 0 =>
            {
                form.title.insert(form.cursor, c);
                form.cursor += c.len_utf8();
            }
            Char(c)
                if form.focus == 1
                    && !key.modifiers.intersects(
                        crossterm::event::KeyModifiers::CONTROL
                            | crossterm::event::KeyModifiers::ALT
                            | crossterm::event::KeyModifiers::SUPER,
                    ) =>
            {
                let i = form.description_cursor.min(form.description.len());
                form.description.insert(i, c);
                form.description_cursor = i + c.len_utf8();
            }
            _ => {}
        }
        false
    }
    fn save_settings(&mut self, proposed: Settings) {
        let result = self
            .settings_path
            .as_deref()
            .ok_or_else(|| "Settings location is unavailable; cannot save.".to_owned())
            .and_then(|path| settings::save_to(path, &proposed).map_err(|error| error.to_string()));
        match result {
            Ok(()) => {
                self.settings = proposed;
                self.theme = proposed.theme;
                self.settings_load_error = None;
                self.notice = Some("Settings saved".into());
                if let Some(Modal::Settings { error, .. }) = self.modal.as_mut() {
                    *error = None;
                }
            }
            Err(error) => {
                if let Some(Modal::Settings { error: message, .. }) = self.modal.as_mut() {
                    *message = Some(format!("Could not save settings: {error}"));
                }
            }
        }
    }
    pub fn selected_is_epic(&self) -> bool {
        self.selected
            .and_then(|i| self.beads.get(i))
            .is_some_and(|bead| bead.data["issue_type"] == "epic")
    }
    fn open_launch(&mut self) {
        let Some(bead) = self
            .selected
            .and_then(|index| self.beads.get(index))
            .cloned()
        else {
            let message = "Select a bead before handing it to an agent".to_owned();
            self.notice = Some(message.clone());
            self.toast = Some(Toast::result(message, true));
            return;
        };
        self.start_launch(bead, false);
    }
    fn toggle_ready(&mut self) {
        if self.mutation_busy {
            return;
        }
        let Some(bead) = self.selected.and_then(|i| self.beads.get(i)) else {
            self.notice = Some("Select an open issue first".into());
            return;
        };
        let epic = bead.data["issue_type"] == "epic";
        if bead.status != "open" {
            self.notice = Some("Only open issues can be marked Ready".into());
            return;
        }
        let id = bead.id.clone();
        let ready = !beads::is_ready(bead);
        if !ready && !self.worktrees_discovered {
            self.notice = Some("Wait for tracked handoffs to load before removing Ready".into());
            return;
        }
        if self.known_handoffs.contains(&id) && ready {
            self.notice = Some("This issue already has a tracked handoff".into());
            return;
        }
        let mut protected_handoffs = self.accepted_handoffs.clone();
        protected_handoffs.extend(self.handoff.values().map(|pending| pending.bead.id.clone()));
        if !ready && !epic && protected_handoffs.contains(&id) {
            self.notice = Some(
                "This issue has been sent or is being handed off; Ready cannot be removed".into(),
            );
            return;
        }
        self.mutation_busy = true;
        self.ready_pending = true;
        self.notice = Some(format!("Updating Ready for {id}…"));
        let request = if epic || !beads::open_descendants_in(&self.beads, &id).is_empty() {
            Request::SetTreeReady(id, ready, self.known_handoffs.clone(), protected_handoffs)
        } else {
            Request::SetReady(id, ready)
        };
        if self.requests.send(request).is_err() {
            self.mutation_busy = false;
            self.ready_pending = false;
            self.notice = Some("Ready change could not be queued".into());
        }
    }
    fn start_launch(&mut self, bead: Bead, scheduled: bool) {
        if self
            .handoff
            .values()
            .any(|pending| pending.bead.id == bead.id)
        {
            return;
        }
        self.launch_seq = self.launch_seq.wrapping_add(1);
        let seq = self.launch_seq;
        self.notice = None;
        self.toast = Some(Toast::progress(format!(
            "Finding a Herdr agent for {}…",
            bead.id
        )));
        self.handoff.insert(
            seq,
            PendingHandoff {
                seq,
                bead,
                launch_queued: false,
                scheduled,
            },
        );
        if self.requests.send(Request::Discover(seq)).is_err() {
            let message = "Workspace discovery could not be queued; try again".to_owned();
            self.fail_handoff(seq, message);
        }
    }
    fn fail_handoff(&mut self, seq: u64, mut message: String) {
        if let Some(pending) = self.handoff.remove(&seq)
            && beads::is_ready(&pending.bead)
        {
            let id = pending.bead.id;
            self.scheduler_attempted.insert(id.clone());
            if self
                .requests
                .send(Request::ResetFailedHandoff(seq, id, message.clone()))
                .is_err()
            {
                message.push_str(
                    "; Ready removal could not be queued; inspect and remove Ready manually",
                );
            }
        }
        self.handoff_feedback(seq, message, true);
    }
    fn handoff_feedback(&mut self, seq: u64, message: String, error: bool) {
        if seq == self.launch_seq {
            self.notice = Some(message.clone());
            self.toast = Some(Toast::result(message, error));
        }
    }
    pub(crate) fn mouse(&mut self, k: MouseEventKind, x: u16, y: u16) {
        if self.mutation_busy && !self.ready_pending {
            self.easter_egg_progress = 0;
        }
        if self.easter_egg || self.modal.is_some() || self.help.is_some() {
            self.easter_egg_progress = 0;
            return;
        }
        if self.worktrees_view {
            match k {
                MouseEventKind::ScrollUp if self.detail_rect.contains((x, y).into()) => {
                    self.worktree_detail_offset = self.worktree_detail_offset.saturating_sub(3);
                }
                MouseEventKind::ScrollDown if self.detail_rect.contains((x, y).into()) => {
                    self.worktree_detail_offset = self.worktree_detail_offset.saturating_add(3);
                }
                MouseEventKind::Down(MouseButton::Left)
                    if self.list_rect.contains((x, y).into())
                        && y > self.list_rect.y
                        && y < self.list_rect.bottom().saturating_sub(1) =>
                {
                    let row = usize::from(y - self.list_rect.y - 1) / 2 + self.worktree_offset;
                    if row < self.worktrees.len() {
                        self.selected_worktree = row;
                        self.worktree_detail_offset = 0;
                    }
                }
                _ => {}
            }
            return;
        }
        if self.focus_detail {
            if self.detail_rect.contains((x, y).into()) {
                match k {
                    MouseEventKind::ScrollUp => {
                        self.detail_offset = self.detail_offset.saturating_sub(3)
                    }
                    MouseEventKind::ScrollDown => {
                        self.detail_offset = self.detail_offset.saturating_add(3)
                    }
                    _ => {}
                }
            }
            return;
        }
        if let Some(&(column, _)) = self
            .board_columns
            .iter()
            .find(|(_, rect)| rect.contains((x, y).into()))
        {
            match k {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(&(index, _)) = self
                        .board_cards
                        .iter()
                        .find(|(_, rect)| rect.contains((x, y).into()))
                    {
                        self.select(index);
                    } else {
                        self.focus_column(column);
                    }
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    if self.focused_column != column {
                        self.focus_column(column);
                    }
                    self.step(if k == MouseEventKind::ScrollUp { -1 } else { 1 });
                }
                _ => {}
            }
        }
    }
}
fn status_order(status: &str) -> usize {
    BOARD_STATUSES
        .iter()
        .position(|&known| known == status)
        .unwrap_or(0)
}
pub use crate::beads::display_status;
pub(super) use crate::worktrees::status_label as worktree_status_label;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayRow {
    Group(String),
    Bead(usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListLayout {
    pub groups: Vec<GroupLayout>,
    pub targets: Vec<ListTarget>,
    pub height: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupLayout {
    pub status: String,
    pub indices: Vec<usize>,
    pub y: usize,
    pub height: usize,
    pub collapsed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListTarget {
    pub row: DisplayRow,
    pub y: usize,
    pub height: usize,
}
pub fn group_statuses(_beads: &[Bead]) -> Vec<String> {
    BOARD_STATUSES.into_iter().map(str::to_owned).collect()
}

/// The visible ancestry of an issue within its status pane, from root to issue.
/// A parent in another pane (or missing from the list) starts a new visible root.
pub(crate) fn issue_path(
    beads: &[Bead],
    positions: &HashMap<&str, usize>,
    index: usize,
) -> Vec<usize> {
    let status = display_status(&beads[index]);
    let mut path = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(index);
    while let Some(i) = current {
        if !seen.insert(i) {
            break;
        }
        path.push(i);
        current = beads[i]
            .data
            .get("parent")
            .and_then(|parent| parent.as_str())
            .and_then(|id| positions.get(id).copied())
            .filter(|&parent| display_status(&beads[parent]) == status);
    }
    path.reverse();
    path
}

fn closed_recency(bead: &Bead) -> Option<time::OffsetDateTime> {
    ["closed_at", "updated_at", "created_at"]
        .into_iter()
        .filter_map(|field| bead.data.get(field).and_then(|value| value.as_str()))
        .find_map(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
        })
}

fn order_worktrees(items: &mut [ManagedWorktree]) {
    let positions: HashMap<_, _> = items
        .iter()
        .enumerate()
        .map(|(index, item)| (item.bead_id.clone(), (index, item.parent_bead_id.clone())))
        .collect();
    items.sort_by_cached_key(|item| {
        let mut path = Vec::new();
        let mut parent = Some(item.bead_id.as_str());
        let mut seen = HashSet::new();
        while let Some(id) = parent {
            if !seen.insert(id) {
                break;
            }
            let Some((index, next)) = positions.get(id) else {
                break;
            };
            path.push(*index);
            parent = next.as_deref();
        }
        path.reverse();
        path
    });
}

pub fn list_layout(beads: &[Bead], collapsed: &HashSet<String>) -> ListLayout {
    let positions = beads
        .iter()
        .enumerate()
        .map(|(index, bead)| (bead.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut layout = ListLayout {
        groups: Vec::new(),
        targets: Vec::new(),
        height: 0,
    };
    for status in group_statuses(beads) {
        let mut indices = beads
            .iter()
            .enumerate()
            .filter(|(_, b)| display_status(b) == status)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if status == "closed" {
            indices.sort_by_cached_key(|&index| {
                (
                    std::cmp::Reverse(closed_recency(&beads[index])),
                    beads[index].id.clone(),
                )
            });
        } else {
            indices.sort_by_cached_key(|&index| issue_path(beads, &positions, index));
        }
        let is_collapsed = collapsed.contains(&status);
        let y = layout.height;
        let height = if is_collapsed {
            2
        } else {
            2 + 2 * indices.len()
        };
        layout.targets.push(ListTarget {
            row: DisplayRow::Group(status.clone()),
            y,
            height: 1,
        });
        if !is_collapsed {
            layout
                .targets
                .extend(indices.iter().enumerate().map(|(n, &i)| ListTarget {
                    row: DisplayRow::Bead(i),
                    y: y + 1 + 2 * n,
                    height: 2,
                }));
        }
        layout.groups.push(GroupLayout {
            status,
            indices,
            y,
            height,
            collapsed: is_collapsed,
        });
        layout.height += height;
    }
    layout
}
pub fn run(t: &mut Tui) -> std::io::Result<()> {
    let start = Instant::now();
    let mut app = App::new();
    let mut redraw = true;
    let mut scheduler_dirty = true;
    let mut next_visual_update = None;
    loop {
        let received = app.receive();
        redraw |= received;
        scheduler_dirty |= received;
        let now = Instant::now();
        redraw |= app.poll_refresh(now);
        redraw |= app.poll_planning(now);
        redraw |= app.poll_worktrees(now);
        app.poll_agent_observations(now);
        if scheduler_dirty {
            scheduler_dirty = app.poll_scheduler();
            redraw |= scheduler_dirty;
        }
        if event::redraw_due(redraw, next_visual_update, now) {
            t.draw(|f| ui::render(f, now.saturating_duration_since(start), &mut app))?;
            redraw = false;
            next_visual_update = ui::next_visual_update(&app, start, now);
        }
        if let Some(e) = event::poll_event(event::wait_timeout(next_visual_update, Instant::now()))?
        {
            if app.input(e) {
                break;
            }
            redraw = true;
            scheduler_dirty = true;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use serde_json::json;

    const EGG_KEYS: [KeyCode; 10] = [
        KeyCode::Up,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Char('b'),
        KeyCode::Char('a'),
    ];

    fn egg_keys(app: &mut App, keys: &[KeyCode]) {
        for &code in keys {
            assert!(!app.input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))));
        }
    }

    #[test]
    fn easter_egg_exact_sequence_works_in_all_browser_views_and_consumes_final_a() {
        for view in 0..3 {
            let (mut a, requests, _) = connected_app();
            assert!(!a.easter_egg);
            a.focus_detail = view == 1;
            a.worktrees_view = view == 2;
            a.worktrees
                .push(observed_row("b0", worktrees::WorktreeStatus::InProgress));
            egg_keys(&mut a, &EGG_KEYS[..9]);
            assert!(!a.easter_egg);
            while requests.try_recv().is_ok() {}
            egg_keys(&mut a, &EGG_KEYS[9..]);
            assert!(a.easter_egg);
            assert_eq!(a.easter_egg_progress, 0);
            assert!(a.modal.is_none());
            assert!(requests.try_recv().is_err());
        }
    }

    #[test]
    fn easter_egg_partial_keys_keep_ordinary_navigation() {
        let mut a = app();
        egg_keys(&mut a, &EGG_KEYS[..4]);
        assert_eq!(a.selected, Some(2));
        assert!(!a.easter_egg);

        a.focus_detail = true;
        a.detail_offset = 10;
        egg_keys(&mut a, &[KeyCode::Up]);
        assert_eq!(a.detail_offset, 9);
        assert!(!a.easter_egg);
    }

    #[test]
    fn easter_egg_wrong_reordered_and_uppercase_sequences_reset() {
        for wrong in [
            KeyCode::Right,
            KeyCode::Char('x'),
            KeyCode::Char('B'),
            KeyCode::Char('A'),
        ] {
            let mut a = app();
            egg_keys(&mut a, &EGG_KEYS[..8]);
            egg_keys(&mut a, &[wrong]);
            assert_eq!(a.easter_egg_progress, 0);
            egg_keys(&mut a, &EGG_KEYS[8..]);
            assert!(!a.easter_egg);
            egg_keys(&mut a, &EGG_KEYS);
            assert!(a.easter_egg);
        }
        for index in 2..EGG_KEYS.len() {
            if EGG_KEYS[index - 1] == EGG_KEYS[index] {
                continue;
            }
            let mut a = app();
            let mut reordered = EGG_KEYS;
            reordered.swap(index - 1, index);
            egg_keys(&mut a, &reordered);
            assert!(!a.easter_egg, "swapped keys at {index}");
        }
        for (index, code) in [(8, KeyCode::Char('B')), (9, KeyCode::Char('A'))] {
            let mut a = app();
            let mut uppercase = EGG_KEYS;
            uppercase[index] = code;
            egg_keys(&mut a, &uppercase);
            assert!(!a.easter_egg);
        }
    }

    #[test]
    fn easter_egg_modified_keys_never_advance_and_reset_progress() {
        for modifiers in [
            KeyModifiers::SHIFT,
            KeyModifiers::ALT,
            KeyModifiers::CONTROL,
        ] {
            for index in 0..EGG_KEYS.len() {
                let mut a = app();
                a.focus_detail = true;
                egg_keys(&mut a, &EGG_KEYS[..index]);
                a.key(KeyEvent::new(EGG_KEYS[index], modifiers));
                assert_eq!(a.easter_egg_progress, 0);
                egg_keys(&mut a, &EGG_KEYS[index + 1..]);
                assert!(!a.easter_egg, "modified key {index}: {modifiers:?}");
                egg_keys(&mut a, &EGG_KEYS);
                assert!(a.easter_egg);
            }
        }
    }

    #[test]
    fn easter_egg_overlapping_up_prefixes_keep_the_valid_suffix() {
        for leading_ups in 1..5 {
            let mut a = app();
            egg_keys(&mut a, &vec![KeyCode::Up; leading_ups]);
            egg_keys(&mut a, &EGG_KEYS);
            assert!(a.easter_egg);
        }
        for prefix in 3..EGG_KEYS.len() {
            let mut a = app();
            egg_keys(&mut a, &EGG_KEYS[..prefix]);
            egg_keys(&mut a, &[KeyCode::Up]);
            assert_eq!(a.easter_egg_progress, 1);
            egg_keys(&mut a, &EGG_KEYS[1..]);
            assert!(a.easter_egg);
        }
    }

    #[test]
    fn easter_egg_help_forms_confirmations_and_mutation_lock_reset_progress() {
        for context in 0..4 {
            let mut a = app();
            egg_keys(&mut a, &EGG_KEYS[..2]);
            match context {
                0 => {
                    a.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
                }
                1 => press(&mut a, KeyCode::Char('n')),
                2 => press(&mut a, KeyCode::Char('d')),
                _ => {
                    a.mutation_busy = true;
                    press(&mut a, KeyCode::Up);
                }
            }
            assert_eq!(a.easter_egg_progress, 0);
            egg_keys(&mut a, &EGG_KEYS);
            assert!(!a.easter_egg);
            assert_eq!(a.easter_egg_progress, 0);
            a.help = None;
            a.modal = None;
            a.mutation_busy = false;
            egg_keys(&mut a, &EGG_KEYS[2..]);
            assert!(!a.easter_egg);
            egg_keys(&mut a, &EGG_KEYS);
            assert!(a.easter_egg);
        }
    }

    #[test]
    fn easter_egg_repeat_and_release_do_not_advance_or_interrupt_presses() {
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            let mut a = app();
            for code in EGG_KEYS {
                a.input(Event::Key(KeyEvent::new_with_kind(
                    code,
                    KeyModifiers::NONE,
                    kind,
                )));
            }
            assert_eq!(a.easter_egg_progress, 0);
            assert!(!a.easter_egg);
            for code in EGG_KEYS {
                egg_keys(&mut a, &[code]);
                a.input(Event::Key(KeyEvent::new_with_kind(
                    KeyCode::Char('x'),
                    KeyModifiers::NONE,
                    kind,
                )));
            }
            assert!(a.easter_egg);
        }
    }

    #[test]
    fn easter_egg_dismissal_preserves_the_underlying_view_and_resets_progress() {
        for view in 0..3 {
            let mut a = app();
            a.set_drawer(view == 2, view != 0, Instant::now());
            a.detail_offset = 12;
            egg_keys(&mut a, &EGG_KEYS[..9]);
            let before = (
                a.focus_detail,
                a.worktrees_view,
                a.drawer_animation
                    .as_ref()
                    .map(|a| (a.worktrees, a.started, a.from, a.to)),
                a.selected,
                a.focused_column,
                a.detail_offset,
                a.selected_worktree,
                a.worktree_detail_offset,
            );
            egg_keys(&mut a, &EGG_KEYS[9..]);
            a.key(KeyEvent::new_with_kind(
                KeyCode::Esc,
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ));
            assert!(a.easter_egg);
            egg_keys(&mut a, &[KeyCode::Esc]);
            assert!(!a.easter_egg);
            assert_eq!(a.easter_egg_progress, 0);
            assert_eq!(
                before,
                (
                    a.focus_detail,
                    a.worktrees_view,
                    a.drawer_animation
                        .as_ref()
                        .map(|a| (a.worktrees, a.started, a.from, a.to)),
                    a.selected,
                    a.focused_column,
                    a.detail_offset,
                    a.selected_worktree,
                    a.worktree_detail_offset,
                )
            );
            egg_keys(&mut a, &EGG_KEYS[9..]);
            assert!(!a.easter_egg);
            egg_keys(&mut a, &EGG_KEYS);
            assert!(a.easter_egg);
        }
    }

    #[test]
    fn easter_egg_blocks_hidden_keyboard_mouse_and_paste_actions_but_allows_quit() {
        for view in 0..3 {
            let (mut a, requests, _) = connected_app();
            a.focus_detail = view == 1;
            a.worktrees_view = view == 2;
            a.worktrees = vec![
                observed_row("b0", worktrees::WorktreeStatus::InProgress),
                observed_row("b1", worktrees::WorktreeStatus::InProgress),
            ];
            egg_keys(&mut a, &EGG_KEYS);
            while requests.try_recv().is_ok() {}
            let before = (
                a.selected,
                a.focused_column,
                a.focus_detail,
                a.worktrees_view,
                a.detail_offset,
                a.selected_worktree,
                a.worktree_detail_offset,
            );
            for code in [
                KeyCode::Enter,
                KeyCode::Tab,
                KeyCode::Down,
                KeyCode::Left,
                KeyCode::Char('n'),
                KeyCode::Char('N'),
                KeyCode::Char('e'),
                KeyCode::Char('d'),
                KeyCode::Char('h'),
                KeyCode::Char('m'),
                KeyCode::Char('s'),
                KeyCode::Char('c'),
                KeyCode::Char('r'),
                KeyCode::Char('w'),
                KeyCode::Char('a'),
            ] {
                egg_keys(&mut a, &[code]);
            }
            for code in [KeyCode::Char('?'), KeyCode::Char('v')] {
                assert!(!a.key(KeyEvent::new(code, KeyModifiers::CONTROL)));
            }
            for (kind, column, row) in [
                (MouseEventKind::Down(MouseButton::Left), 1, 3),
                (MouseEventKind::ScrollDown, 1, 3),
                (MouseEventKind::ScrollDown, 31, 3),
            ] {
                assert!(!a.input(Event::Mouse(crossterm::event::MouseEvent {
                    kind,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                })));
            }
            assert!(!a.input(Event::Paste("nq\u{1b}[Aba".into())));
            assert!(a.easter_egg);
            assert_eq!(
                before,
                (
                    a.selected,
                    a.focused_column,
                    a.focus_detail,
                    a.worktrees_view,
                    a.detail_offset,
                    a.selected_worktree,
                    a.worktree_detail_offset,
                )
            );
            assert!(a.modal.is_none() && a.help.is_none());
            assert!(!a.mutation_busy && !a.paste_pending);
            assert!(requests.try_recv().is_err());
            // The screen keeps quit shortcuts even if background work locks input.
            a.mutation_busy = true;
            for key in [
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            ] {
                assert!(a.input(Event::Key(key)));
            }
        }
    }

    #[test]
    fn easter_egg_paste_does_not_supply_keys_or_carry_partial_progress() {
        let mut a = app();
        egg_keys(&mut a, &EGG_KEYS[..9]);
        assert!(!a.input(Event::Paste("a".into())));
        assert_eq!(a.easter_egg_progress, 0);
        egg_keys(&mut a, &EGG_KEYS[9..]);
        assert!(!a.easter_egg);
    }

    #[test]
    fn easter_egg_locked_mouse_input_resets_partial_progress() {
        let mut a = app();
        egg_keys(&mut a, &EGG_KEYS[..2]);
        a.mutation_busy = true;
        a.mouse(MouseEventKind::ScrollDown, 1, 3);
        assert_eq!(a.easter_egg_progress, 0);
        a.mutation_busy = false;
        egg_keys(&mut a, &EGG_KEYS[2..]);
        assert!(!a.easter_egg);
    }

    #[test]
    fn update_key_no_longer_opens_browser_action() {
        let (mut app, requests, _) = connected_app();
        app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        assert!(app.modal.is_none());
        assert!(requests.try_recv().is_err());
        assert!(
            !hotkeys::Context::Issues
                .entries()
                .iter()
                .any(|(key, _)| *key == "u")
        );
    }

    #[test]
    fn shift_n_creates_child_of_selected_task_and_n_stays_top_level() {
        let mut app = app();
        app.beads[0].data["issue_type"] = json!("task");
        let (tx, rx) = mpsc::channel();
        app.requests = tx;
        app.key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT));
        let seq = take_harness_discovery(&rx);
        supply_harnesses(&mut app, seq, &["codex"]);
        assert!(
            matches!(&app.modal, Some(Modal::Create(form)) if form.parent_id.as_deref() == Some("b0"))
        );
        app.modal_ready = true;
        app.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        for _ in 0..5 {
            app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            matches!(rx.try_recv(), Ok(Request::CreateChild(input, parent)) if input.title == "x" && parent == "b0")
        );
        app.modal = None;
        app.mutation_busy = false;
        app.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(&app.modal, Some(Modal::Create(form)) if form.parent_id.is_none()));
    }

    #[test]
    fn managed_children_sort_immediately_below_parent() {
        let item = |id: &str, parent: Option<&str>| ManagedWorktree {
            id: id.into(),
            bead_id: id.into(),
            parent_bead_id: parent.map(str::to_owned),
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: id.into(),
            target_branch: "trunk".into(),
            workspace_id: id.into(),
            status: worktrees::WorktreeStatus::Unmerged,
            detail: None,
        };
        let mut items = vec![
            item("parent", None),
            item("other", None),
            item("child", Some("parent")),
            item("grandchild", Some("child")),
        ];
        order_worktrees(&mut items);
        assert_eq!(
            items
                .iter()
                .map(|item| item.bead_id.as_str())
                .collect::<Vec<_>>(),
            ["parent", "child", "grandchild", "other"]
        );
    }

    #[test]
    fn issue_layout_groups_children_under_visible_parents() {
        let bead = |id: &str, parent: Option<&str>, status: &str| Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: id.into(),
            title: id.into(),
            status: status.into(),
            priority: 2,
            data: json!({"parent": parent}),
        };
        // Input is ID order, like the refreshed list. A child can precede its parent.
        let beads = vec![
            bead("a-child", Some("z-parent"), "open"),
            bead("b-grandchild", Some("a-child"), "open"),
            bead("c-other", None, "open"),
            bead("d-detached", Some("z-parent"), "closed"),
            bead("e-orphan", Some("missing"), "open"),
            bead("z-parent", None, "open"),
        ];
        let layout = list_layout(&beads, &HashSet::new());
        let open = layout
            .groups
            .iter()
            .find(|group| group.status == "open")
            .unwrap();
        assert_eq!(open.indices, [2, 4, 5, 0, 1]);
        let closed = layout
            .groups
            .iter()
            .find(|group| group.status == "closed")
            .unwrap();
        assert_eq!(closed.indices, [3]);
        let positions = beads
            .iter()
            .enumerate()
            .map(|(index, bead)| (bead.id.as_str(), index))
            .collect();
        assert_eq!(issue_path(&beads, &positions, 1), [5, 0, 1]);
        assert_eq!(issue_path(&beads, &positions, 3), [3]);
        assert_eq!(issue_path(&beads, &positions, 4), [4]);

        let mut app = app();
        app.beads = beads;
        app.selected = Some(5);
        app.step(1);
        assert_eq!(app.selected, Some(0));
        app.step(1);
        assert_eq!(app.selected, Some(1));
        app.board_columns = vec![(0, Rect::new(5, 7, 40, 24))];
        app.board_cards = vec![(5, Rect::new(6, 8, 38, 3))];
        app.mouse(MouseEventKind::Down(MouseButton::Left), 6, 8);
        assert_eq!(app.selected, Some(5));
    }

    #[test]
    fn selection_scrolls_minimally_in_both_directions_and_keeps_column_offsets() {
        let mut a = app();
        a.beads.extend((4..8).map(|i| Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: format!("b{i}"),
            title: format!("issue {i}"),
            status: "open".into(),
            priority: 2,
            data: json!({"id": format!("b{i}")}),
        }));
        a.board_cards.truncate(3);
        a.selected = Some(0);
        a.step(1);
        assert_eq!((a.selected, a.column_offsets[0]), (Some(1), 0));
        a.step(1);
        assert_eq!((a.selected, a.column_offsets[0]), (Some(2), 0));
        a.step(1);
        assert_eq!((a.selected, a.column_offsets[0]), (Some(3), 1));
        a.step(1);
        assert_eq!((a.selected, a.column_offsets[0]), (Some(4), 2));
        a.step(-1);
        assert_eq!((a.selected, a.column_offsets[0]), (Some(3), 2));
        a.column_offsets[1] = 7;
        a.focused_column = 1;
        a.board_columns.clear();
        a.select(0);
        assert_eq!(a.column_offsets[1], 7);
        a.step(-1);
        assert_eq!(a.selected, Some(0));
        a.step(1);
        assert_eq!(a.column_offsets[1], 7);
    }

    #[test]
    fn closed_group_uses_completion_recency_instead_of_id_or_parent_order() {
        let bead = |id: &str, status: &str, data: serde_json::Value| Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: id.into(),
            title: id.into(),
            status: status.into(),
            priority: 2,
            data,
        };
        let beads = vec![
            bead(
                "a-old",
                "closed",
                json!({"closed_at": "2026-10-20T15:00:00Z", "updated_at": "2026-10-21T15:00:00Z"}),
            ),
            bead(
                "b-fallback",
                "closed",
                json!({"closed_at": "invalid", "updated_at": "2026-10-20T14:00:00Z"}),
            ),
            bead(
                "z-new",
                "closed",
                json!({"parent": "a-old", "closed_at": "2026-10-20T17:30:00+02:00"}),
            ),
            bead(
                "d-tie",
                "closed",
                json!({"closed_at": "2026-10-20T15:00:00Z"}),
            ),
            bead("c-undated", "closed", json!({"closed_at": "invalid"})),
            bead(
                "e-created",
                "closed",
                json!({"created_at": "2026-10-20T13:00:00Z"}),
            ),
            bead("open", "open", json!({})),
        ];
        let layout = list_layout(&beads, &HashSet::new());
        let closed = layout
            .groups
            .iter()
            .find(|group| group.status == "closed")
            .unwrap();
        assert_eq!(closed.indices, [2, 0, 3, 1, 5, 4]);
        assert_eq!(
            layout
                .targets
                .iter()
                .filter_map(|target| match target.row {
                    DisplayRow::Bead(index) if beads[index].status == "closed" => Some(index),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            closed.indices
        );
    }

    #[test]
    fn closed_refresh_reorders_rows_and_keeps_selected_issue() {
        let mut a = app();
        let mut items = a.beads[..2].to_vec();
        for item in &mut items {
            item.status = "closed".into();
        }
        items[0].data["closed_at"] = json!("2026-10-20T15:00:00Z");
        items[1].data["closed_at"] = json!("2026-10-20T14:00:00Z");
        respond(&mut a, Response::List(Ok(items.clone())));
        assert_eq!(a.beads[a.selected.unwrap()].id, "b0");
        assert_eq!(
            list_layout(&a.beads, &HashSet::new()).groups[5].indices,
            [0, 1]
        );

        items[1].data["closed_at"] = json!("2026-10-20T16:00:00Z");
        respond(&mut a, Response::List(Ok(items)));
        assert_eq!(a.beads[a.selected.unwrap()].id, "b0");
        assert_eq!(
            list_layout(&a.beads, &HashSet::new()).groups[5].indices,
            [1, 0]
        );
        a.step(-1);
        assert_eq!(a.beads[a.selected.unwrap()].id, "b1");
    }
    fn app() -> App {
        let (tx, _rx) = mpsc::channel();
        let (_out, responses) = mpsc::channel();
        App {
            beads: (0..4)
                .map(|i| Bead {
                    dependency_ready: true,
                    blocked_by: Vec::new(),
                    id: format!("b{i}"),
                    title: format!("issue {i}"),
                    status: "open".into(),
                    priority: 2,
                    data: json!({"id": format!("b{i}"), "description": format!("detail {i}")}),
                })
                .collect(),
            focused_column: 0,
            column_offsets: [0; 6],
            board_columns: vec![(0, Rect::new(0, 0, 30, 12))],
            board_cards: (0..4)
                .map(|i| (i, Rect::new(1, 1 + i as u16 * 2, 28, 2)))
                .collect(),
            column_rows: [0; 6],
            selected: Some(0),
            detail: Some(json!({"id": "b0", "description": "detail 0"})),
            focus_detail: false,
            detail_offset: 0,
            drawer_animation: None,
            list_loading: false,
            detail_loading: false,
            list_error: None,
            detail_error: None,
            detail_seq: 0,
            edit_seq: 0,
            harness_seq: 0,
            launch_seq: 0,
            handoff: HashMap::new(),
            handoff_capacity: HashSet::new(),
            toast: None,
            list_rect: Rect::new(0, 0, 30, 8),
            detail_rect: Rect::new(30, 0, 50, 8),
            modal: None,
            help: None,
            easter_egg: false,
            easter_egg_progress: 0,
            settings: Settings::default(),
            theme: Theme::Dark,
            settings_path: None,
            settings_load_error: None,
            worktrees: Vec::new(),
            accepted_handoffs: HashSet::new(),
            known_handoffs: HashSet::new(),
            scheduler_attempted: HashSet::new(),
            selected_worktree: 0,
            worktree_offset: 0,
            worktree_detail_offset: 0,
            worktrees_view: false,
            worktrees_loading: false,
            worktrees_error: None,
            worktrees_focus_feedback: None,
            worktrees_focus_pending: false,
            worktrees_inflight: false,
            worktrees_discovered: true,
            worktrees_refresh_pending: false,
            worktree_action_pending: false,
            worktrees_next_refresh: Instant::now(),
            agent_observation_inflight: false,
            next_agent_observation: Instant::now(),
            notice: None,
            mutation_busy: false,
            ready_pending: false,
            planning_epics: HashSet::new(),
            modal_ready: true,
            paste_seq: 0,
            paste_pending: false,
            created_screenshots: Vec::new(),
            preferred_id: None,
            refresh_inflight: false,
            refresh_pending: false,
            next_refresh: Instant::now(),
            requests: tx,
            responses,
        }
    }
    fn assert_bead(a: &App, id: &str) {
        let bead = &a.beads[a.selected.expect("selected bead")];
        assert_eq!(bead.id, id);
        assert_eq!(a.detail.as_ref(), Some(&bead.data));
    }
    #[test]
    fn board_navigation_stays_in_column_and_visits_empty_columns_without_wrapping() {
        let mut a = app();
        a.beads[2].status = "blocked".into();
        a.beads[3].status = "closed".into();
        a.step(-1);
        assert_bead(&a, "b0");
        a.step(20);
        assert_bead(&a, "b1");
        press(&mut a, KeyCode::Right);
        assert_eq!(a.focused_column, 1);
        assert!(a.selected.is_none());
        a.step(20);
        assert_eq!(a.focused_column, 1);
        assert!(a.selected.is_none());
        press(&mut a, KeyCode::Right);
        assert_bead(&a, "b2");
        a.step(20);
        assert_bead(&a, "b2");
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Left);
        assert_bead(&a, "b1");
        a.focus_column(5);
        press(&mut a, KeyCode::Right);
        assert_bead(&a, "b3");
        assert_eq!(a.focused_column, 5);
        a.focus_column(0);
        press(&mut a, KeyCode::Left);
        assert_bead(&a, "b1");
        a.beads.clear();
        a.step(1);
        assert!(a.selected.is_none());
    }

    #[test]
    fn board_statuses_are_fixed_and_unknown_data_is_not_rewritten() {
        let mut a = app();
        assert_eq!(group_statuses(&[]), BOARD_STATUSES);
        for status in ["deferred", "custom", "", "unknown"] {
            a.beads[0].status = status.into();
            assert_eq!(display_status(&a.beads[0]), "open");
            let items = a.beads.clone();
            respond(&mut a, Response::List(Ok(items)));
            assert_eq!(
                a.beads.iter().find(|b| b.id == "b0").unwrap().status,
                status
            );
            assert_eq!(group_statuses(&a.beads), BOARD_STATUSES);
        }
        a.beads[0].status = "in_progress".into();
        a.beads[0].data["status"] = json!("in_progress");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, beads::READY_TO_MERGE_LABEL]);
        assert_eq!(display_status(&a.beads[0]), "ready_to_merge");
    }

    #[test]
    fn shift_move_is_per_bead_even_for_parent_with_accepted_handoff() {
        let (mut a, rx, _) = connected_app();
        a.beads[0].data["issue_type"] = json!("epic");
        a.beads[1].data["parent"] = json!("b0");
        a.accepted_handoffs.insert("b0".into());
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(
            matches!(rx.try_recv(), Ok(Request::Move(id, target)) if id == "b0" && target == "ready")
        );
        assert!(a.mutation_busy && a.ready_pending);
        press(&mut a, KeyCode::Down);
        assert_bead(&a, "b1");
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        press(&mut a, KeyCode::Char('m'));
        press(&mut a, KeyCode::Char('r'));
        assert!(a.modal.is_none());
        assert!(matches!(rx.try_recv(), Ok(Request::Show(id, _)) if id == "b1"));
        assert!(rx.try_recv().is_err());

        let (mut a, rx, _) = connected_app();
        a.beads[0].status = "in_progress".into();
        a.accepted_handoffs.insert("b0".into());
        a.select(0);
        a.key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        assert!(
            matches!(rx.try_recv(), Ok(Request::Move(id, target)) if id == "b0" && target == "blocked")
        );
    }

    #[test]
    fn move_chooser_captures_identity_and_closed_uses_existing_confirmation() {
        let (mut a, rx, _) = connected_app();
        a.select(2);
        take_show(&rx, "b2");
        press(&mut a, KeyCode::Char('m'));
        assert!(matches!(a.modal, Some(Modal::Move { ref id, selected: 0, .. }) if id == "b2"));
        a.select(1);
        take_show(&rx, "b1");
        for _ in 0..20 {
            press(&mut a, KeyCode::Down);
        }
        assert!(matches!(a.modal, Some(Modal::Move { selected: 5, .. })));
        press(&mut a, KeyCode::Enter);
        assert!(matches!(a.modal, Some(Modal::ConfirmClose { ref id, .. }) if id == "b2"));
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Char('y'));
        assert!(matches!(rx.try_recv(), Ok(Request::Close(id)) if id == "b2"));

        let (mut a, rx, _) = connected_app();
        press(&mut a, KeyCode::Char('m'));
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Enter);
        assert!(
            matches!(rx.try_recv(), Ok(Request::Move(id, target)) if id == "b0" && target == "blocked")
        );
        assert!(a.modal.is_none());
    }

    #[test]
    fn move_results_refresh_authoritative_state_and_follow_id_without_optimism() {
        let (mut a, rx, tx) = connected_app();
        a.scheduler_attempted.insert("b0".into());
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(matches!(rx.try_recv(), Ok(Request::Move(..))));
        assert_eq!(display_status(&a.beads[0]), "open");
        tx.send(Response::Moved("b0".into(), "ready".into(), Ok(())))
            .unwrap();
        a.receive();
        assert_eq!(display_status(&a.beads[0]), "open");
        assert!(!a.scheduler_attempted.contains("b0"));
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        let mut items = a.beads.clone();
        items[0].data["labels"] = json!([beads::READY_LABEL]);
        items.reverse();
        tx.send(Response::List(Ok(items))).unwrap();
        a.receive();
        assert_eq!(a.beads[a.selected.unwrap()].id, "b0");
        assert_eq!(a.focused_column, 1);
        take_show(&rx, "b0");
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));

        let (mut a, rx, tx) = connected_app();
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(matches!(rx.try_recv(), Ok(Request::Move(..))));
        tx.send(Response::Moved(
            "b0".into(),
            "ready".into(),
            Err(std::io::Error::other("live state changed")),
        ))
        .unwrap();
        a.receive();
        assert!(!a.mutation_busy && !a.ready_pending);
        assert!(
            a.toast
                .as_ref()
                .is_some_and(|toast| toast.error && toast.message.contains("live state changed"))
        );
        assert_eq!(display_status(&a.beads[0]), "open");
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
    }

    #[test]
    fn refresh_preserves_empty_column_drawers_and_moves_selection_by_id() {
        let mut a = app();
        a.focus_column(4);
        a.column_offsets[0] = 2;
        let items = a.beads.clone();
        respond(&mut a, Response::List(Ok(items)));
        assert_eq!(a.focused_column, 4);
        assert!(a.selected.is_none());
        assert_eq!(a.column_offsets[0], 2);
        a.select(2);
        a.focus_detail = true;
        a.detail_offset = 8;
        let mut items = a.beads.clone();
        items[2].status = "closed".into();
        items.reverse();
        respond(&mut a, Response::List(Ok(items)));
        assert_eq!(a.beads[a.selected.unwrap()].id, "b2");
        assert_eq!(a.focused_column, 5);
        assert!(a.focus_detail && !a.worktrees_view);
        assert_eq!(a.detail_offset, 8);
        let items = a.beads.iter().filter(|b| b.id != "b2").cloned().collect();
        respond(&mut a, Response::List(Ok(items)));
        assert_eq!(a.focused_column, 5);
        assert!(a.selected.is_none());
        assert!(a.focus_detail);
    }

    #[test]
    fn drawers_restore_board_selection_and_isolate_move_inputs() {
        let (mut a, rx, _) = connected_app();
        a.worktrees = vec![
            observed_row("b1", worktrees::WorktreeStatus::InProgress),
            observed_row("b0", worktrees::WorktreeStatus::InProgress),
        ];
        press(&mut a, KeyCode::Enter);
        assert!(a.focus_detail && !a.worktrees_view);
        let selected = a.selected;
        press(&mut a, KeyCode::Enter);
        assert!(!a.focus_detail && a.selected == selected);
        press(&mut a, KeyCode::Enter);
        assert!(a.focus_detail && a.selected == selected);
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        press(&mut a, KeyCode::Char('m'));
        press(&mut a, KeyCode::Down);
        assert_eq!(a.selected, Some(0));
        assert_eq!(a.detail_offset, 1);
        assert!(a.modal.is_none());
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Char('w'));
        assert!(a.worktrees_view && !a.focus_detail);
        assert_eq!(a.selected_worktree, 1);
        a.key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        press(&mut a, KeyCode::Up);
        assert_eq!(a.selected_worktree, 0);
        assert_eq!(a.selected, Some(0));
        press(&mut a, KeyCode::Esc);
        assert!(!a.worktrees_view && !a.focus_detail);
        assert_eq!(a.selected, Some(0));
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Esc);
        assert!(!a.focus_detail);
        a.beads[0].data["issue_type"] = json!("epic");
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Enter);
        assert!(!a.focus_detail && a.selected == Some(0));
        press(&mut a, KeyCode::Char('w'));
        press(&mut a, KeyCode::Enter);
        assert!(a.worktrees_view && !a.focus_detail);
    }

    #[test]
    fn board_mouse_uses_renderer_rectangles_and_drawers_prevent_click_through() {
        let mut a = app();
        a.beads[2].status = "blocked".into();
        a.beads[3].status = "blocked".into();
        a.board_columns = vec![(0, Rect::new(10, 5, 20, 20)), (2, Rect::new(30, 5, 20, 20))];
        a.board_cards = vec![
            (1, Rect::new(11, 10, 18, 3)),
            (2, Rect::new(31, 6, 18, 3)),
            (3, Rect::new(31, 10, 18, 3)),
        ];
        a.mouse(MouseEventKind::Down(MouseButton::Left), 12, 11);
        assert_bead(&a, "b1");
        a.mouse(MouseEventKind::ScrollDown, 32, 6);
        assert_bead(&a, "b3");
        assert_eq!(a.focused_column, 2);
        a.mouse(MouseEventKind::Down(MouseButton::Left), 4, 4);
        assert_bead(&a, "b3");
        a.focus_detail = true;
        a.detail_rect = Rect::new(30, 5, 20, 20);
        a.mouse(MouseEventKind::Down(MouseButton::Left), 32, 6);
        a.mouse(MouseEventKind::Down(MouseButton::Left), 12, 11);
        assert_bead(&a, "b3");
        a.mouse(MouseEventKind::ScrollDown, 32, 6);
        assert_eq!(a.detail_offset, 3);
        a.focus_detail = false;
        a.worktrees_view = true;
        a.list_rect = Rect::new(30, 5, 20, 20);
        a.mouse(MouseEventKind::Down(MouseButton::Left), 12, 11);
        assert_bead(&a, "b3");
    }

    #[test]
    fn move_queue_failure_and_chooser_cancel_leave_board_available() {
        let mut a = app();
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(!a.mutation_busy && !a.ready_pending);
        assert!(a.notice.as_deref().unwrap().contains("could not be queued"));
        press(&mut a, KeyCode::Char('m'));
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
        assert_bead(&a, "b0");
    }

    #[test]
    fn board_move_keys_do_not_leak_through_help_or_move_modal() {
        let (mut a, rx, _) = connected_app();
        a.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        press(&mut a, KeyCode::Char('m'));
        assert!(a.help.is_some() && a.modal.is_none());
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('m'));
        assert_eq!(hotkeys::Context::current(&a), hotkeys::Context::Move);
        a.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(matches!(a.modal, Some(Modal::Move { selected: 0, .. })));
        assert_eq!(a.focused_column, 0);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn move_preserves_preferred_id_until_post_mutation_snapshot_and_scrolls_destination() {
        let (mut a, rx, tx) = connected_app();
        a.beads[1].status = "blocked".into();
        let stale = a.beads.clone();
        a.refresh();
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        a.move_bead("b0".into(), "issue 0".into(), 2);
        assert!(matches!(rx.try_recv(), Ok(Request::Move(..))));
        tx.send(Response::Moved("b0".into(), "blocked".into(), Ok(())))
            .unwrap();
        a.receive();
        assert!(a.refresh_pending);
        tx.send(Response::List(Ok(stale.clone()))).unwrap();
        a.receive();
        take_show(&rx, "b0");
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        assert_eq!(a.preferred_id.as_deref(), Some("b0"));
        let mut updated = stale;
        updated[0].status = "blocked".into();
        updated[0].data["parent"] = json!("b1");
        tx.send(Response::List(Ok(updated))).unwrap();
        a.receive();
        take_show(&rx, "b0");
        assert_eq!(a.beads[a.selected.unwrap()].id, "b0");
        assert_eq!(a.focused_column, 2);
        assert_eq!(a.column_offsets[2], 1);
        assert!(a.preferred_id.is_none());
        assert!(a.scheduler_attempted.contains("b0"));
    }

    #[test]
    fn outstanding_handoffs_are_reconciled_with_all_cleanup_preferences_disabled() {
        let (mut a, rx, _) = connected_app();
        a.settings.auto_merge = false;
        a.settings.auto_merge_children = false;
        a.settings.auto_close_workspace = false;
        a.settings.auto_remove_checkout = false;
        a.known_handoffs.insert("b0".into());
        a.poll_worktrees(Instant::now());
        assert!(
            matches!(rx.try_recv(), Ok(Request::ReconcileWorktrees(policy))
            if !policy.auto_merge && !policy.auto_merge_children
                && !policy.auto_close_workspace && !policy.auto_remove_checkout)
        );
    }

    #[test]
    fn ready_group_is_distinct_while_beads_status_stays_open() {
        let mut a = app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        assert_eq!(a.beads[0].status, "open");
        assert_eq!(
            group_statuses(&a.beads),
            [
                "open",
                "ready",
                "blocked",
                "in_progress",
                "ready_to_merge",
                "closed"
            ]
        );
        let layout = list_layout(&a.beads, &HashSet::new());
        assert_eq!(layout.groups[1].indices, [0]);
        a.select(0);
        assert_eq!(a.focused_column, 1);
    }

    #[test]
    fn scheduler_waits_for_dependencies_then_resumes_without_ready_toggle() {
        let (mut a, rx, _) = connected_app();
        a.beads.truncate(1);
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        a.beads[0].dependency_ready = false;
        a.beads[0].blocked_by = vec!["prerequisite".into()];
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
        assert!(!a.scheduler_attempted.contains("b0"));
        assert!(beads::is_ready(&a.beads[0]));
        a.beads[0].dependency_ready = true;
        a.beads[0].blocked_by.clear();
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
    }

    #[test]
    fn scheduler_pauses_on_failed_browser_snapshot_and_skips_blocked_children() {
        let (mut a, rx, _) = connected_app();
        a.beads.truncate(2);
        for bead in &mut a.beads {
            bead.data["issue_type"] = json!("task");
            bead.data["labels"] = json!([beads::READY_LABEL]);
        }
        a.beads[0].data["issue_type"] = json!("epic");
        a.beads[1].data["parent"] = json!("b0");
        a.beads[1].dependency_ready = false;
        a.poll_scheduler();
        assert!(
            rx.try_recv().is_err(),
            "blocked child must not trigger parent preparation"
        );
        a.beads[1].dependency_ready = true;
        a.list_error = Some("readiness lookup failed".into());
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
        a.list_error = None;
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
    }

    #[test]
    fn scheduler_uses_selected_harness_once_and_keeps_ready_after_launch() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:opencode"]);
        a.poll_scheduler();
        let Request::Discover(seq) = rx.try_recv().unwrap() else {
            panic!("discovery not queued")
        };
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
        let (out, responses) = mpsc::channel();
        a.responses = responses;
        out.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        let Request::LaunchReady(sent_seq, _, agent, bead) = rx.try_recv().unwrap() else {
            panic!("launch not queued")
        };
        assert_eq!(sent_seq, seq);
        assert_eq!(agent.id, "opencode");
        assert_eq!(bead.id, "b0");
        out.send(Response::Launched(
            seq,
            Ok(LaunchOutcome {
                message: "Sent".into(),
                reserved: false,
            }),
        ))
        .unwrap();
        a.receive();
        assert!(rx.try_recv().is_err(), "handoff must not clear Ready");
        assert_eq!(display_status(&a.beads[0]), "ready");
        assert!(a.known_handoffs.contains("b0"));
        assert!(a.accepted_handoffs.contains("b0"));
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
        a.beads[0].status = "in_progress".into();
        assert_eq!(display_status(&a.beads[0]), "in_progress");
    }

    #[test]
    fn ready_epic_prepares_parent_before_unlabelled_child_launch() {
        let mut a = app();
        let (requests, queued) = mpsc::channel();
        a.requests = requests;
        let (replies, responses) = mpsc::channel();
        a.responses = responses;
        a.beads[0].data["issue_type"] = json!("epic");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        a.beads[1].data["issue_type"] = json!("bug");
        a.beads[1].data["parent"] = json!("b0");
        a.beads[1].data["labels"] = json!([beads::READY_LABEL]);
        a.settings.default_agent = DefaultAgent::Kind("opencode");
        a.settings.agent_limit = settings::AgentLimit::Limited(1);
        a.worktrees.push(ManagedWorktree {
            id: "active".into(),
            bead_id: "other".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/other".into(),
            target_branch: "main".into(),
            workspace_id: "ws-other".into(),
            status: worktrees::WorktreeStatus::InProgress,
            detail: None,
        });

        a.poll_scheduler();
        let Request::Discover(parent_seq) = queued.try_recv().unwrap() else {
            panic!("parent discovery not queued")
        };
        a.poll_scheduler();
        assert!(queued.try_recv().is_err());
        replies
            .send(Response::Discovered(parent_seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(
            matches!(queued.try_recv(), Ok(Request::LaunchReady(_, _, _, bead)) if bead.id == "b0")
        );
        replies
            .send(Response::Launched(
                parent_seq,
                Ok(LaunchOutcome {
                    message: "Prepared".into(),
                    reserved: true,
                }),
            ))
            .unwrap();
        a.receive();
        assert!(a.known_handoffs.contains("b0"));
        a.poll_scheduler();
        assert!(queued.try_recv().is_err());
        replies
            .send(Response::Worktrees(Ok(vec![ManagedWorktree {
                id: "parent".into(),
                bead_id: "b0".into(),
                parent_bead_id: None,
                handoff_accepted: false,
                bead_title: None,
                bead_description: None,
                branch: "ontrack/parent".into(),
                target_branch: "main".into(),
                workspace_id: "ws-parent".into(),
                status: worktrees::WorktreeStatus::Reserved,
                detail: None,
            }])))
            .unwrap();
        a.receive();
        a.poll_scheduler();
        let Request::Discover(child_seq) = queued.try_recv().unwrap() else {
            panic!("child discovery not queued")
        };
        replies
            .send(Response::Discovered(child_seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(
            matches!(queued.try_recv(), Ok(Request::LaunchReady(_, _, agent, bead))
            if bead.id == "b1" && agent.id == "opencode")
        );
    }

    #[test]
    fn explicit_unassigned_ready_task_is_not_scheduled() {
        let mut a = app();
        let (requests, queued) = mpsc::channel();
        a.requests = requests;
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, beads::UNASSIGNED_LABEL]);
        a.poll_scheduler();
        assert!(queued.try_recv().is_err());
        assert!(a.notice.as_deref().unwrap().contains("Unassigned"));
        a.poll_scheduler();
        assert!(queued.try_recv().is_err());
    }

    #[test]
    fn failed_epic_preparation_is_not_retried_or_duplicated() {
        let mut a = app();
        let (requests, queued) = mpsc::channel();
        a.requests = requests;
        let (replies, responses) = mpsc::channel();
        a.responses = responses;
        a.beads[0].data["issue_type"] = json!("epic");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        a.beads[1].data["issue_type"] = json!("task");
        a.beads[1].data["parent"] = json!("b0");
        a.beads[1].data["labels"] = json!([beads::READY_LABEL]);
        a.poll_scheduler();
        let Request::Discover(seq) = queued.try_recv().unwrap() else {
            panic!("parent discovery not queued")
        };
        replies
            .send(Response::Discovered(
                seq,
                Err(std::io::Error::other("Herdr unavailable")),
            ))
            .unwrap();
        a.receive();
        assert!(
            matches!(queued.try_recv(), Ok(Request::ResetFailedHandoff(_, id, _)) if id == "b0")
        );
        a.poll_scheduler();
        assert!(queued.try_recv().is_err());
        assert!(a.notice.as_deref().unwrap().contains("Herdr unavailable"));

        a.scheduler_attempted.clear();
        a.known_handoffs.insert("b0".into());
        a.poll_scheduler();
        assert!(queued.try_recv().is_err());
    }

    #[test]
    fn scheduler_defers_unavailable_harness_and_existing_handoffs() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:gemini"]);
        a.poll_scheduler();
        let Request::Discover(seq) = rx.try_recv().unwrap() else {
            panic!()
        };
        let (out, responses) = mpsc::channel();
        a.responses = responses;
        out.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(matches!(rx.try_recv(), Ok(Request::ResetFailedHandoff(_, id, _)) if id == "b0"));
        assert!(rx.try_recv().is_err());
        assert!(a.notice.as_deref().unwrap().contains("unavailable"));
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
        a.scheduler_attempted.remove("b0");
        a.known_handoffs.insert("b0".into());
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn scheduler_waits_at_limit_and_resumes_after_active_handoff_finishes() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.settings.agent_limit = settings::AgentLimit::Limited(1);
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
        a.worktrees.push(ManagedWorktree {
            id: "manual".into(),
            bead_id: "other".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/other".into(),
            target_branch: "main".into(),
            workspace_id: "ws-other".into(),
            status: worktrees::WorktreeStatus::DirtyCheckout,
            detail: None,
        });
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
        assert!(!a.scheduler_attempted.contains("b0"));
        let (out, responses) = mpsc::channel();
        a.responses = responses;
        let mut finished = a.worktrees[0].clone();
        finished.status = worktrees::WorktreeStatus::Unmerged;
        out.send(Response::Worktrees(Ok(vec![finished]))).unwrap();
        a.receive();
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
        assert_eq!(a.handoff.values().next().unwrap().bead.id, "b0");
    }

    #[test]
    fn unlimited_scheduler_ignores_active_handoff_count() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.settings.agent_limit = settings::AgentLimit::Unlimited;
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
        a.worktrees.push(ManagedWorktree {
            id: "active".into(),
            bead_id: "other".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/other".into(),
            target_branch: "main".into(),
            workspace_id: "ws-other".into(),
            status: worktrees::WorktreeStatus::InProgress,
            detail: None,
        });
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
    }
    #[test]
    fn scheduler_skips_ready_task_with_journaled_handoff_after_restart() {
        let mut a = app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
        let (requests, rx) = mpsc::channel();
        a.requests = requests;
        let (responses, incoming) = mpsc::channel();
        a.responses = incoming;
        responses
            .send(Response::Worktrees(Ok(vec![ManagedWorktree {
                id: "handoff".into(),
                bead_id: "b0".into(),
                parent_bead_id: None,
                handoff_accepted: true,
                bead_title: None,
                bead_description: None,
                branch: "ontrack/b0".into(),
                target_branch: "main".into(),
                workspace_id: "ws".into(),
                status: worktrees::WorktreeStatus::InProgress,
                detail: None,
            }])))
            .unwrap();
        a.receive();
        assert_eq!(display_status(&a.beads[0]), "ready");
        assert!(a.known_handoffs.contains("b0"));
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn ready_toggle_uses_r_in_both_panes_and_does_not_change_beads_status() {
        let mut a = app();
        a.beads[0].data["issue_type"] = json!("task");
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        press(&mut a, KeyCode::Char('r'));
        assert!(matches!(rx.try_recv(), Ok(Request::SetReady(id, true)) if id == "b0"));
        assert!(rx.try_recv().is_err());
        assert!(!a.refresh_pending);
        assert_eq!(a.beads[0].status, "open");
        a.mutation_busy = false;
        a.ready_pending = false;
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        a.focus_detail = true;
        press(&mut a, KeyCode::Char('r'));
        assert!(matches!(rx.try_recv(), Ok(Request::SetReady(id, false)) if id == "b0"));
        assert!(rx.try_recv().is_err());
        assert!(!a.refresh_pending);
        a.mutation_busy = false;
        a.ready_pending = false;
        press(&mut a, KeyCode::Char('R'));
        assert!(matches!(rx.try_recv(), Ok(Request::SetReady(id, false)) if id == "b0"));
    }

    #[test]
    fn non_task_types_toggle_ready_and_keep_handoff_protection() {
        for issue_type in ["bug", "feature", "chore", "custom"] {
            let (mut app, requests, _) = connected_app();
            app.beads[0].data["issue_type"] = json!(issue_type);
            press(&mut app, KeyCode::Char('r'));
            assert!(matches!(requests.try_recv(), Ok(Request::SetReady(id, true)) if id == "b0"));
            app.mutation_busy = false;
            app.ready_pending = false;
            app.beads[0].data["labels"] = json!([beads::READY_LABEL]);
            assert_eq!(display_status(&app.beads[0]), "ready");
            assert!(
                list_layout(&app.beads, &HashSet::new())
                    .groups
                    .iter()
                    .any(|group| { group.status == "ready" && group.indices.contains(&0) })
            );
            app.accepted_handoffs.insert("b0".into());
            press(&mut app, KeyCode::Char('R'));
            assert!(requests.try_recv().is_err());
            assert!(
                app.notice
                    .as_deref()
                    .unwrap()
                    .contains("Ready cannot be removed")
            );
            app.accepted_handoffs.clear();
            press(&mut app, KeyCode::Char('R'));
            assert!(matches!(requests.try_recv(), Ok(Request::SetReady(id, false)) if id == "b0"));
            assert_eq!(app.beads[0].status, "open");
        }
    }

    #[test]
    fn non_task_ready_issue_enters_scheduled_handoff() {
        for issue_type in ["bug", "feature", "custom"] {
            let (mut app, requests, responses) = connected_app();
            app.beads[0].data["issue_type"] = json!(issue_type);
            app.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:opencode"]);
            app.poll_scheduler();
            let Request::Discover(seq) = requests.try_recv().unwrap() else {
                panic!("Ready issue did not enter handoff discovery");
            };
            responses
                .send(Response::Discovered(seq, Ok(launch_context())))
                .unwrap();
            app.receive();
            assert!(matches!(
                requests.try_recv(),
                Ok(Request::LaunchReady(_, _, agent, bead))
                    if agent.id == "opencode" && bead.id == "b0"
            ));
        }
    }

    #[test]
    fn pending_ready_keeps_browser_input_live_and_excludes_mutations() {
        for epic in [false, true] {
            for ready in [false, true] {
                let (mut a, requests, replies) = connected_app();
                a.beads[0].data["issue_type"] = json!(if epic { "epic" } else { "task" });
                a.beads[1].data["issue_type"] = json!("task");
                if !ready {
                    a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
                    a.beads[1].data["labels"] = json!([beads::READY_LABEL]);
                }
                press(&mut a, KeyCode::Char('r'));
                assert!(a.ready_pending && a.mutation_busy);
                assert!(a.notice.as_deref().unwrap().contains("Updating Ready"));
                assert!(matches!(
                    requests.try_recv(),
                    Ok(Request::SetReady(..) | Request::SetTreeReady(..))
                ));
                for key in ['r', 'R', 'n', 'N', 'e', 'd', 'g', 'h', 's', 'u'] {
                    press(&mut a, KeyCode::Char(key));
                }
                a.poll_scheduler();
                assert!(requests.try_recv().is_err());
                assert!(a.modal.is_none());
                press(&mut a, KeyCode::Down);
                assert_ne!(a.selected, Some(0));
                assert!(matches!(requests.try_recv(), Ok(Request::Show(..))));
                press(&mut a, KeyCode::Tab);
                press(&mut a, KeyCode::Down);
                assert!(a.focus_detail);
                assert_eq!(a.detail_offset, 1);
                // Shift+? opens help while a Ready update is still blocked.
                a.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
                assert!(a.help.is_some());
                press(&mut a, KeyCode::Esc);
                assert!(a.help.is_none());
                assert!(a.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
                assert_eq!(beads::is_ready(&a.beads[0]), !ready);
                replies
                    .send(Response::ReadySet(
                        "b0".into(),
                        ready,
                        Ok(vec!["b0".into()]),
                    ))
                    .unwrap();
                a.receive();
                assert!(!a.ready_pending && !a.mutation_busy);
                assert_eq!(beads::is_ready(&a.beads[0]), ready);
                assert!(matches!(requests.try_recv(), Ok(Request::List)));
            }
        }
    }

    #[test]
    fn ready_queue_failure_releases_input_and_mutation_state() {
        let mut a = app(); // Its request receiver has been dropped.
        a.beads[0].data["issue_type"] = json!("task");
        press(&mut a, KeyCode::Char('r'));
        assert!(!a.ready_pending && !a.mutation_busy);
        assert!(a.notice.as_deref().unwrap().contains("could not be queued"));
        assert!(a.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
    }

    #[test]
    fn blocked_ready_does_not_delay_browser_reads_or_managed_work() {
        for epic in [false, true] {
            let (release, blocked) = mpsc::channel();
            let blocked = std::sync::Mutex::new(blocked);
            let (entered, started) = mpsc::channel();
            let (requests, responses, _) = start_workers(move |request, out| {
                let response = match request {
                    Request::SetReady(id, ready) | Request::SetTreeReady(id, ready, ..) => {
                        entered.send(()).unwrap();
                        // Timeout also releases the worker if a regression fails the test.
                        blocked
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(10))
                            .unwrap();
                        Response::ReadySet(id.clone(), ready, Ok(vec![id]))
                    }
                    Request::List => Response::List(Ok(vec![])),
                    Request::Show(id, seq) => Response::Show(id, seq, Ok(json!({}))),
                    Request::ListWorktrees => Response::Worktrees(Ok(vec![])),
                    _ => panic!("unexpected request"),
                };
                out.send(response).unwrap();
            });
            requests
                .send(if epic {
                    Request::SetTreeReady("epic".into(), true, HashSet::new(), HashSet::new())
                } else {
                    Request::SetReady("task".into(), false)
                })
                .unwrap();
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            requests.send(Request::List).unwrap();
            requests.send(Request::Show("other".into(), 1)).unwrap();
            requests.send(Request::ListWorktrees).unwrap();
            let mut observed = HashSet::new();
            for _ in 0..3 {
                observed.insert(
                    match responses.recv_timeout(Duration::from_secs(5)).unwrap() {
                        Response::List(Ok(_)) => "list",
                        Response::Show(_, _, Ok(_)) => "detail",
                        Response::Worktrees(Ok(_)) => "managed",
                        _ => panic!("Ready must still be blocked"),
                    },
                );
            }
            assert_eq!(observed.len(), 3);
            release.send(()).unwrap();
            assert!(matches!(
                responses.recv_timeout(Duration::from_secs(5)),
                Ok(Response::ReadySet(_, _, Ok(_)))
            ));
        }
    }

    #[test]
    fn ready_key_respects_closed_and_handoff_guards() {
        let (mut a, rx, _) = connected_app();
        a.beads[0].data["issue_type"] = json!("bug");
        a.beads[0].status = "closed".into();
        press(&mut a, KeyCode::Char('r'));
        assert_eq!(
            a.notice.as_deref(),
            Some("Only open issues can be marked Ready")
        );
        assert!(rx.try_recv().is_err());

        a.beads[0].status = "open".into();
        a.beads[0].data["issue_type"] = json!("task");
        a.known_handoffs.insert("b0".into());
        press(&mut a, KeyCode::Char('r'));
        assert_eq!(
            a.notice.as_deref(),
            Some("This issue already has a tracked handoff")
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn sent_ready_task_cannot_be_unreadied_but_unsent_handoff_can() {
        let (mut a, rx, _) = connected_app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        a.known_handoffs.insert("b0".into());
        a.accepted_handoffs.insert("b0".into());
        for focus_detail in [false, true] {
            a.focus_detail = focus_detail;
            press(&mut a, KeyCode::Char('r'));
            assert!(rx.try_recv().is_err());
            assert!(
                a.notice
                    .as_deref()
                    .unwrap()
                    .contains("Ready cannot be removed")
            );
            assert!(beads::is_ready(&a.beads[0]));
        }

        a.accepted_handoffs.clear();
        a.worktrees_discovered = false;
        press(&mut a, KeyCode::Char('r'));
        assert!(rx.try_recv().is_err());
        assert!(
            a.notice
                .as_deref()
                .unwrap()
                .contains("tracked handoffs to load")
        );
        a.worktrees_discovered = true;
        press(&mut a, KeyCode::Char('r'));
        assert!(matches!(rx.try_recv(), Ok(Request::SetReady(id, false)) if id == "b0"));
    }

    #[test]
    fn ready_task_cannot_be_unreadied_during_handoff() {
        let (mut a, rx, _) = connected_app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        press(&mut a, KeyCode::Char('h'));
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
        press(&mut a, KeyCode::Char('r'));
        assert!(rx.try_recv().is_err());
        assert!(
            a.notice
                .as_deref()
                .unwrap()
                .contains("Ready cannot be removed")
        );
    }

    #[test]
    fn epic_ready_toggle_queues_cascade_and_only_tasks_are_scheduled() {
        let mut a = app();
        a.beads[0].data["issue_type"] = json!("epic");
        a.beads[1].data["issue_type"] = json!("task");
        a.beads[1].data["parent"] = json!("b0");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        assert_eq!(display_status(&a.beads[0]), "ready");
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());

        a.beads[0].data["labels"] = json!([]);
        a.known_handoffs.insert("b2".into());
        press(&mut a, KeyCode::Char('r'));
        assert!(matches!(
            rx.try_recv(),
            Ok(Request::SetTreeReady(id, true, handoffs, protected))
                if id == "b0" && handoffs.contains("b2") && protected.is_empty()
        ));
        assert_eq!(a.beads[0].status, "open");
    }

    #[test]
    fn epic_ready_response_updates_confirmed_ids_and_refreshes_after_failure() {
        let mut a = app();
        a.beads[0].data["issue_type"] = json!("epic");
        a.beads[1].data["issue_type"] = json!("task");
        let (requests, queued) = mpsc::channel();
        a.requests = requests;
        let (replies, responses) = mpsc::channel();
        a.responses = responses;
        a.mutation_busy = true;
        a.ready_pending = true;
        replies
            .send(Response::ReadySet(
                "b0".into(),
                true,
                Ok(vec!["b0".into(), "b1".into()]),
            ))
            .unwrap();
        a.receive();
        assert!(a.beads.iter().take(2).all(beads::is_ready));
        assert!(!a.mutation_busy);
        assert!(matches!(queued.try_recv(), Ok(Request::List)));

        a.refresh_inflight = false;
        a.mutation_busy = true;
        a.ready_pending = true;
        replies
            .send(Response::ReadySet(
                "b0".into(),
                false,
                Err(std::io::Error::other("partial update")),
            ))
            .unwrap();
        a.receive();
        assert!(matches!(queued.try_recv(), Ok(Request::List)));
        assert!(a.notice.as_deref().unwrap().contains("partial update"));
        assert!(!a.ready_pending && !a.mutation_busy);
    }

    #[test]
    fn task_ready_toggle_queues_tree_with_pending_and_accepted_protection() {
        let (mut a, rx, _) = connected_app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[1].data["parent"] = json!("b0");
        a.beads[2].data["parent"] = json!("b1");
        a.accepted_handoffs.insert("b1".into());
        a.known_handoffs.insert("b1".into());
        let pending = a.beads[2].clone();
        a.start_launch(pending, false);
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
        press(&mut a, KeyCode::Char('r'));
        assert!(matches!(
            rx.try_recv(),
            Ok(Request::SetTreeReady(id, true, known, protected))
                if id == "b0" && known.contains("b1")
                    && protected.contains("b1") && protected.contains("b2")
        ));
    }

    #[test]
    fn nested_ready_tasks_reserve_ancestors_then_run_agents_from_leaves_up() {
        let (mut a, rx, _) = connected_app();
        a.beads.truncate(3);
        for (i, bead) in a.beads.iter_mut().enumerate() {
            bead.data["issue_type"] = json!("task");
            bead.data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
            // Ancestors have higher priority, but still must wait for descendants.
            bead.priority = i as u64;
            if i > 0 {
                bead.data["parent"] = json!(format!("b{}", i - 1));
            }
        }
        a.settings.agent_limit = settings::AgentLimit::Limited(1);
        a.worktrees
            .push(observed_row("other", worktrees::WorktreeStatus::InProgress));
        // Checkouts must be prepared root first, even at the agent limit.
        for (id, parent) in [("b0", None), ("b1", Some("b0"))] {
            a.poll_scheduler();
            let Request::Discover(seq) = rx.try_recv().unwrap() else {
                panic!("ancestor preparation was not queued");
            };
            let pending = a.handoff.remove(&seq).unwrap();
            assert_eq!(pending.bead.id, id);
            assert!(pending.scheduled);
            a.scheduler_attempted.remove(id);
            a.known_handoffs.insert(id.into());
            let mut row = observed_row(id, worktrees::WorktreeStatus::Reserved);
            row.handoff_accepted = false;
            row.parent_bead_id = parent.map(str::to_owned);
            a.worktrees.push(row);
        }
        a.poll_scheduler();
        assert!(rx.try_recv().is_err(), "leaf must wait for an agent slot");
        a.worktrees.remove(0);
        a.poll_scheduler();
        let Request::Discover(seq) = rx.try_recv().unwrap() else {
            panic!("leaf launch was not queued");
        };
        let pending = a.handoff.remove(&seq).unwrap();
        assert_eq!(pending.bead.id, "b2");
        assert!(pending.scheduled);
        a.known_handoffs.insert("b2".into());
        let mut leaf = observed_row("b2", worktrees::WorktreeStatus::InProgress);
        leaf.parent_bead_id = Some("b1".into());
        a.worktrees.push(leaf);
        assert!(!a.scheduler_eligible(&a.beads[0]));
        assert!(!a.scheduler_eligible(&a.beads[1]));
        a.beads[2].status = "closed".into();
        a.worktrees[2].status = worktrees::WorktreeStatus::Unmerged;
        assert!(
            !a.scheduler_eligible(&a.beads[1]),
            "closed leaf must integrate first"
        );
        a.worktrees[2].status = worktrees::WorktreeStatus::Integrated;
        a.poll_scheduler();
        let Request::Discover(seq) = rx.try_recv().unwrap() else {
            panic!("intermediate parent did not resume");
        };
        let pending = a.handoff.remove(&seq).unwrap();
        assert_eq!(pending.bead.id, "b1");
        assert!(pending.scheduled);
        assert!(!a.scheduler_eligible(&a.beads[0]));
        a.beads[1].status = "closed".into();
        a.worktrees[1].status = worktrees::WorktreeStatus::Unmerged;
        assert!(
            !a.scheduler_eligible(&a.beads[0]),
            "closed child must integrate first"
        );
        a.worktrees[1].status = worktrees::WorktreeStatus::Integrated;
        a.poll_scheduler();
        let Request::Discover(seq) = rx.try_recv().unwrap() else {
            panic!("root parent did not resume");
        };
        let pending = a.handoff.remove(&seq).unwrap();
        assert_eq!(pending.bead.id, "b0");
        assert!(pending.scheduled);
    }

    #[test]
    fn prepared_parent_waits_for_children_then_can_resume() {
        let mut a = app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
        a.beads[1].data["parent"] = json!("b0");
        a.known_handoffs.insert("b0".into());
        a.worktrees.push(ManagedWorktree {
            id: "handoff".into(),
            bead_id: "b0".into(),
            parent_bead_id: None,
            handoff_accepted: false,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/parent".into(),
            target_branch: "main".into(),
            workspace_id: "w1".into(),
            status: worktrees::WorktreeStatus::Reserved,
            detail: None,
        });
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.poll_scheduler();
        assert!(rx.try_recv().is_err());
        a.beads[1].status = "closed".into();
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
    }

    #[test]
    fn ready_child_waits_for_parent_checkout_even_at_higher_priority() {
        let mut a = app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
        a.beads[0].priority = 3;
        a.beads[1].data["issue_type"] = json!("task");
        a.beads[1].data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
        a.beads[1].data["parent"] = json!("b0");
        a.beads[1].priority = 0;
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
        assert_eq!(
            a.handoff.values().next().map(|item| item.bead.id.as_str()),
            Some("b0")
        );
    }
    fn launch_context() -> LaunchContext {
        LaunchContext {
            workspace: crate::herdr::Workspace {
                id: "ws".into(),
                label: "ws".into(),
            },
            agents: vec![
                Agent {
                    id: "codex".into(),
                    label: "Codex".into(),
                },
                Agent {
                    id: "opencode".into(),
                    label: "OpenCode".into(),
                },
            ],
        }
    }
    fn observed_row(bead: &str, status: worktrees::WorktreeStatus) -> ManagedWorktree {
        ManagedWorktree {
            id: format!("handoff-{bead}"),
            bead_id: bead.into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: format!("ontrack/{bead}"),
            target_branch: "main".into(),
            workspace_id: "wTask".into(),
            status,
            detail: Some("Keep recovery evidence".into()),
        }
    }

    #[test]
    fn blocked_launch_does_not_delay_ready_launch_browser_create_or_managed_refresh() {
        use std::sync::Barrier;
        let gate = Arc::new(Barrier::new(2));
        let worker_gate = gate.clone();
        let (entered, started) = mpsc::channel();
        let (requests, responses, _) = start_workers(move |request, out| {
            let response = match request {
                Request::Launch(seq, ..) => {
                    entered.send(()).unwrap();
                    worker_gate.wait();
                    Response::Launched(seq, Err(std::io::Error::other("first failed")))
                }
                Request::LaunchReady(seq, ..) => Response::Launched(
                    seq,
                    Ok(LaunchOutcome {
                        reserved: false,
                        message: "second accepted".into(),
                    }),
                ),
                Request::List => Response::List(Ok(vec![])),
                Request::Create(_) => Response::Created(Ok("created".into())),
                Request::ListWorktrees => Response::Worktrees(Ok(vec![])),
                _ => panic!("unexpected request"),
            };
            out.send(response).unwrap();
        });
        let context = launch_context();
        let bead = app().beads[0].clone();
        requests
            .send(Request::Launch(
                1,
                context.clone(),
                context.agents[0].clone(),
                bead.clone(),
            ))
            .unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        requests
            .send(Request::LaunchReady(
                2,
                context.clone(),
                context.agents[0].clone(),
                bead,
            ))
            .unwrap();
        requests.send(Request::List).unwrap();
        requests
            .send(Request::Create(CreateBead {
                title: "new".into(),
                description: String::new(),
                issue_type: "task".into(),
                priority: 2,
                harness: None,
            }))
            .unwrap();
        requests.send(Request::ListWorktrees).unwrap();
        let mut observed = HashSet::new();
        for _ in 0..4 {
            observed.insert(
                match responses.recv_timeout(Duration::from_secs(5)).unwrap() {
                    Response::Launched(2, Ok(_)) => "ready",
                    Response::List(Ok(_)) => "browser",
                    Response::Created(Ok(_)) => "create",
                    Response::Worktrees(Ok(_)) => "managed",
                    _ => panic!("first launch remains blocked"),
                },
            );
        }
        assert_eq!(observed.len(), 4);
        gate.wait();
        assert!(matches!(
            responses.recv_timeout(Duration::from_secs(5)),
            Ok(Response::Launched(1, Err(_)))
        ));
    }

    #[test]
    fn managed_work_is_serial_and_cannot_occupy_the_browser_worker() {
        use std::sync::Barrier;
        let gate = Arc::new(Barrier::new(2));
        let worker_gate = gate.clone();
        let (tx, started) = mpsc::channel();
        let (steps, received_steps) = mpsc::channel();
        let (requests, responses, _) = start_workers(move |request, out| {
            let response = match request {
                Request::WorktreeAction(_, _) => {
                    tx.send(()).unwrap();
                    worker_gate.wait();
                    steps.send(1).unwrap();
                    Response::WorktreeChanged(Err(std::io::Error::other("finished")))
                }
                Request::ListWorktrees => {
                    steps.send(2).unwrap();
                    Response::Worktrees(Ok(vec![]))
                }
                Request::List => Response::List(Ok(vec![])),
                _ => panic!("unexpected request"),
            };
            out.send(response).unwrap();
        });
        requests
            .send(Request::WorktreeAction(
                "first".into(),
                WorktreeAction::RequestMerge,
            ))
            .unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        requests.send(Request::ListWorktrees).unwrap();
        requests.send(Request::List).unwrap();
        assert!(matches!(
            responses.recv_timeout(Duration::from_secs(5)),
            Ok(Response::List(Ok(_)))
        ));
        assert!(received_steps.try_recv().is_err());
        gate.wait();
        assert_eq!(
            received_steps.recv_timeout(Duration::from_secs(5)).unwrap(),
            1
        );
        assert_eq!(
            received_steps.recv_timeout(Duration::from_secs(5)).unwrap(),
            2
        );
    }

    #[test]
    fn moves_and_closes_serialize_with_reconciliation_without_blocking_reads_or_ready() {
        for reconcile_first in [true, false] {
            let (release, blocked) = mpsc::channel();
            let blocked = std::sync::Mutex::new(blocked);
            let (entered, started) = mpsc::channel();
            let (steps, completed) = mpsc::channel();
            let (requests, responses, _) = start_workers(move |request, out| {
                let wait = || {
                    entered.send(()).unwrap();
                    blocked
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                };
                let response = match request {
                    Request::ReconcileWorktrees(_) => {
                        if reconcile_first {
                            wait();
                        }
                        steps.send("reconcile").unwrap();
                        Response::Worktrees(Ok(vec![]))
                    }
                    Request::Move(id, target) => {
                        if !reconcile_first {
                            wait();
                        }
                        steps.send("move").unwrap();
                        Response::Moved(id, target, Ok(()))
                    }
                    Request::Close(id) => {
                        steps.send("close").unwrap();
                        Response::Closed(id, Ok(()))
                    }
                    Request::List => Response::List(Ok(vec![])),
                    Request::Show(id, seq) => Response::Show(id, seq, Ok(json!({}))),
                    Request::SetReady(id, ready) => {
                        Response::ReadySet(id.clone(), ready, Ok(vec![id]))
                    }
                    _ => panic!("unexpected request"),
                };
                out.send(response).unwrap();
            });
            let reconcile = Request::ReconcileWorktrees(CleanupPolicy {
                auto_merge: false,
                auto_merge_children: false,
                auto_close_workspace: false,
                auto_remove_checkout: false,
            });
            let backward_move = Request::Move("task".into(), "in_progress".into());
            let (first, second) = if reconcile_first {
                (reconcile, backward_move)
            } else {
                (backward_move, reconcile)
            };
            requests.send(first).unwrap();
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            requests.send(second).unwrap();
            requests.send(Request::Close("task".into())).unwrap();
            requests
                .send(Request::SetReady("other".into(), true))
                .unwrap();
            requests.send(Request::List).unwrap();
            requests.send(Request::Show("other".into(), 1)).unwrap();
            let mut observed = HashSet::new();
            for _ in 0..3 {
                observed.insert(
                    match responses.recv_timeout(Duration::from_secs(5)).unwrap() {
                        Response::List(Ok(_)) => "list",
                        Response::Show(_, _, Ok(_)) => "show",
                        Response::ReadySet(_, _, Ok(_)) => "ready",
                        _ => {
                            panic!("managed mutations must remain queued behind the paused request")
                        }
                    },
                );
            }
            assert_eq!(observed, HashSet::from(["list", "show", "ready"]));
            // Ready and browser replies fence the old Move/Close lanes, without sleeps.
            assert!(completed.try_recv().is_err());
            release.send(()).unwrap();
            let expected = if reconcile_first {
                ["reconcile", "move", "close"]
            } else {
                ["move", "reconcile", "close"]
            };
            for step in expected {
                assert_eq!(
                    completed.recv_timeout(Duration::from_secs(5)).unwrap(),
                    step
                );
                let response = responses.recv_timeout(Duration::from_secs(5)).unwrap();
                match step {
                    "reconcile" => assert!(matches!(response, Response::Worktrees(Ok(_)))),
                    "move" => assert!(matches!(response, Response::Moved(id, target, Ok(()))
                        if id == "task" && target == "in_progress")),
                    _ => assert!(matches!(response, Response::Closed(id, Ok(())) if id == "task")),
                }
            }
        }
    }

    #[test]
    fn distinct_manual_and_ready_results_do_not_clear_other_pending_state_or_toast() {
        let (mut a, rx, tx) = connected_app();
        a.start_launch(a.beads[0].clone(), false);
        let Request::Discover(first) = rx.try_recv().unwrap() else {
            panic!()
        };
        a.start_launch(a.beads[1].clone(), true);
        let Request::Discover(second) = rx.try_recv().unwrap() else {
            panic!()
        };
        a.start_launch(a.beads[0].clone(), false);
        assert!(rx.try_recv().is_err());
        tx.send(Response::Discovered(first, Ok(launch_context())))
            .unwrap();
        tx.send(Response::Discovered(second, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(matches!(rx.try_recv(), Ok(Request::Launch(seq,..)) if seq == first));
        assert!(matches!(rx.try_recv(), Ok(Request::LaunchReady(seq,..)) if seq == second));
        let toast = a.toast.as_ref().unwrap().message.clone();
        a.worktrees_inflight = true;
        tx.send(Response::Launched(
            first,
            Ok(LaunchOutcome {
                message: "first success".into(),
                reserved: false,
            }),
        ))
        .unwrap();
        a.receive();
        assert_eq!(a.handoff.len(), 1);
        assert_eq!(a.handoff[&second].bead.id, "b1");
        assert_eq!(a.toast.as_ref().unwrap().message, toast);
        assert!(a.worktrees_refresh_pending);
        assert!(a.handoff_capacity.contains("b0"));
        tx.send(Response::Launched(
            first,
            Err(std::io::Error::other("stale")),
        ))
        .unwrap();
        a.receive();
        assert_eq!(a.toast.as_ref().unwrap().message, toast);
        tx.send(Response::Launched(
            second,
            Err(std::io::Error::other("second uncertain")),
        ))
        .unwrap();
        a.receive();
        assert!(a.handoff.is_empty());
        assert!(
            a.toast
                .as_ref()
                .unwrap()
                .message
                .contains("second uncertain")
        );
        tx.send(Response::Worktrees(Ok(vec![]))).unwrap();
        a.receive();
        assert!(
            !a.worktrees_discovered,
            "a stale scan requires a trailing durable refresh"
        );
        assert!(a.handoff_capacity.contains("b0"));
        a.poll_worktrees(Instant::now());
        assert!(matches!(rx.try_recv(), Ok(Request::ListWorktrees)));
        tx.send(Response::Worktrees(Ok(vec![observed_row(
            "b0",
            worktrees::WorktreeStatus::InProgress,
        )])))
        .unwrap();
        a.receive();
        assert!(!a.handoff_capacity.contains("b0"));
    }

    #[test]
    fn ready_capacity_counts_all_pending_agents_but_not_epic_checkout_reservations() {
        let (mut a, rx, _) = connected_app();
        a.settings.agent_limit = settings::AgentLimit::Limited(1);
        for bead in &mut a.beads {
            bead.data["issue_type"] = json!("task");
            bead.data["labels"] = json!([beads::READY_LABEL, "ontrack:agent:codex"]);
        }
        a.beads[0].data["issue_type"] = json!("epic");
        a.beads[1].data["parent"] = json!("b0");
        a.poll_scheduler();
        assert!(matches!(rx.try_recv(), Ok(Request::Discover(_))));
        assert!(a.handoff.values().any(|pending| pending.bead.id == "b0"));
        a.poll_scheduler();
        assert!(
            matches!(rx.try_recv(), Ok(Request::Discover(_))),
            "reserving an Epic's parent checkout consumes no agent slot"
        );
        assert!(a.handoff.values().any(|pending| pending.bead.id == "b2"));
        a.poll_scheduler();
        assert!(
            rx.try_recv().is_err(),
            "pending agent launch consumes the remaining slot"
        );
        a.selected = Some(2);
        a.toggle_ready();
        assert!(
            rx.try_recv().is_err(),
            "Ready cannot be removed from any pending task"
        );
        a.selected = Some(0);
        a.toggle_ready();
        assert!(
            matches!(rx.try_recv(), Ok(Request::SetTreeReady(_,false,_,protected)) if protected.contains("b0") && protected.contains("b2"))
        );
    }

    #[test]
    fn display_only_agent_polling_is_bounded_and_preserves_nonactive_worktree_state() {
        let (mut a, rx, tx) = connected_app();
        a.worktrees.push(observed_row(
            "b0",
            worktrees::WorktreeStatus::NeedsAttention,
        ));
        let now = Instant::now();
        a.poll_agent_observations(now);
        a.poll_agent_observations(now + Duration::from_secs(50));
        assert!(matches!(rx.try_recv(), Ok(Request::ObserveAgents)));
        assert!(rx.try_recv().is_err());
        tx.send(Response::AgentsObserved(Err(std::io::Error::other(
            "offline",
        ))))
        .unwrap();
        a.receive_at(now);
        assert_eq!(a.worktrees.len(), 1);
        assert_eq!(
            a.worktrees[0].status,
            worktrees::WorktreeStatus::NeedsAttention
        );
        assert!(
            a.worktrees[0]
                .detail
                .as_ref()
                .unwrap()
                .contains("Keep recovery evidence")
        );
        assert!(a.worktrees[0].detail.as_ref().unwrap().contains("unknown"));
        a.poll_agent_observations(now + Duration::from_secs(1));
        assert!(rx.try_recv().is_err());
        a.poll_agent_observations(now + Duration::from_secs(2));
        assert!(matches!(rx.try_recv(), Ok(Request::ObserveAgents)));
        tx.send(Response::AgentsObserved(Ok(vec![
            worktrees::AgentDisplay {
                id: "handoff-b0".into(),
                workspace_id: "wTask".into(),
                detail: "Agent observation (display only): working".into(),
                blocked: false,
            },
        ])))
        .unwrap();
        a.receive_at(now + Duration::from_secs(2));
        assert_eq!(
            a.worktrees[0].status,
            worktrees::WorktreeStatus::NeedsAttention
        );
        assert!(a.worktrees[0].detail.as_ref().unwrap().contains("working"));
        assert!(!a.worktrees[0].detail.as_ref().unwrap().contains("offline"));
        tx.send(Response::Worktrees(Ok(vec![observed_row(
            "b0",
            worktrees::WorktreeStatus::NeedsAttention,
        )])))
        .unwrap();
        a.receive_at(now + Duration::from_secs(2));
        assert!(
            a.worktrees[0]
                .detail
                .as_ref()
                .unwrap()
                .contains("Agent observation (display only): working")
        );
    }
    #[test]
    fn verified_agent_failure_blocks_only_an_in_progress_worktree() {
        let (mut app, _requests, responses) = connected_app();
        app.worktrees.push(observed_row(
            "active",
            worktrees::WorktreeStatus::InProgress,
        ));
        app.worktrees.push(observed_row(
            "dirty",
            worktrees::WorktreeStatus::DirtyCheckout,
        ));
        let now = Instant::now();

        for state in ["error", "blocked"] {
            app.agent_observation_inflight = true;
            responses
                .send(Response::AgentsObserved(Ok(vec![
                    worktrees::AgentDisplay {
                        id: "handoff-active".into(),
                        workspace_id: "wTask".into(),
                        detail: format!("Agent observation (display only): {state}"),
                        blocked: true,
                    },
                    worktrees::AgentDisplay {
                        id: "handoff-dirty".into(),
                        workspace_id: "wTask".into(),
                        detail: format!("Agent observation (display only): {state}"),
                        blocked: true,
                    },
                ])))
                .unwrap();
            app.receive_at(now);
            assert_eq!(app.worktrees[0].status, worktrees::WorktreeStatus::Blocked);
            assert_eq!(worktree_status_label(app.worktrees[0].status), "BLOCKED");
            assert!(app.worktrees[0].detail.as_deref().unwrap().contains(state));
            assert_eq!(
                app.worktrees[1].status,
                worktrees::WorktreeStatus::DirtyCheckout
            );
        }

        app.agent_observation_inflight = true;
        responses
            .send(Response::Worktrees(Ok(vec![
                observed_row("active", worktrees::WorktreeStatus::InProgress),
                observed_row("dirty", worktrees::WorktreeStatus::DirtyCheckout),
            ])))
            .unwrap();
        app.receive_at(now);
        assert_eq!(app.worktrees[0].status, worktrees::WorktreeStatus::Blocked);

        responses
            .send(Response::AgentsObserved(Err(std::io::Error::other(
                "offline",
            ))))
            .unwrap();
        app.receive_at(now);
        assert_eq!(
            app.worktrees[0].status,
            worktrees::WorktreeStatus::InProgress
        );
        assert!(
            app.worktrees[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("unknown")
        );
        app.agent_observation_inflight = true;
        responses
            .send(Response::AgentsObserved(Ok(vec![
                worktrees::AgentDisplay {
                    id: "handoff-active".into(),
                    workspace_id: "wTask".into(),
                    detail: "Agent observation (display only): working".into(),
                    blocked: false,
                },
            ])))
            .unwrap();
        app.receive_at(now);
        assert_eq!(
            app.worktrees[0].status,
            worktrees::WorktreeStatus::InProgress
        );
        assert!(
            app.worktrees[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("working")
        );
        assert_eq!(
            app.worktrees[1].status,
            worktrees::WorktreeStatus::DirtyCheckout
        );
    }
    #[test]
    fn handoff_discovers_and_launches_once_without_dialog_or_confirmation() {
        let (mut a, rx, tx) = connected_app();
        a.beads[0].data["description"] = json!("Full context");
        press(&mut a, KeyCode::Char('h'));
        let seq = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => panic!("expected discovery"),
        };
        assert!(a.modal.is_none());
        assert!(!a.handoff.is_empty());
        assert!(
            a.toast
                .as_ref()
                .is_some_and(|toast| toast.expires_at.is_none())
        );
        press(&mut a, KeyCode::Char('h'));
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Down);
        assert_eq!(a.selected, Some(1), "browsing remains available");
        assert!(matches!(rx.try_recv(), Ok(Request::Show(..))));
        tx.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(
            matches!(rx.try_recv(), Ok(Request::Launch(id, context, agent, bead))
            if id == seq && context.workspace.id == "ws" && agent.id == "codex"
                && bead.id == "b0" && bead.data["description"] == "Full context")
        );
        tx.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(
            rx.try_recv().is_err(),
            "duplicate discovery cannot relaunch"
        );
        press(&mut a, KeyCode::Char('h'));
        assert!(
            matches!(rx.try_recv(), Ok(Request::Discover(next)) if next != seq),
            "a distinct selected bead can launch concurrently"
        );
        assert_eq!(a.handoff.len(), 2);
    }

    #[test]
    fn worktree_poll_uses_short_read_scan_during_pending_handoff() {
        let (mut a, requests, _) = connected_app();
        a.worktrees
            .push(observed_row("b3", worktrees::WorktreeStatus::InProgress));
        press(&mut a, KeyCode::Char('h'));
        assert!(matches!(requests.try_recv(), Ok(Request::Discover(_))));
        a.poll_worktrees(Instant::now());
        assert!(matches!(requests.try_recv(), Ok(Request::ListWorktrees)));
    }
    #[test]
    fn handoff_requires_selection_and_ignores_stale_discovery() {
        let (mut a, rx, tx) = connected_app();
        a.clear_selection();
        press(&mut a, KeyCode::Char('h'));
        assert!(rx.try_recv().is_err());
        assert!(a.toast.as_ref().is_some_and(|toast| toast.error));
        a.select(0);
        let _ = rx.try_recv();
        press(&mut a, KeyCode::Char('h'));
        let seq = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => unreachable!(),
        };
        tx.send(Response::Discovered(seq + 1, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(rx.try_recv().is_err());
        assert!(!a.handoff.is_empty());
        tx.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(matches!(rx.try_recv(), Ok(Request::Launch(..))));
    }
    #[test]
    fn handoff_uses_first_available_agent_if_codex_is_absent() {
        let (mut a, rx, tx) = connected_app();
        press(&mut a, KeyCode::Char('h'));
        let seq = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => unreachable!(),
        };
        let mut context = launch_context();
        context.agents.remove(0);
        tx.send(Response::Discovered(seq, Ok(context))).unwrap();
        a.receive();
        assert!(
            matches!(rx.try_recv(), Ok(Request::Launch(_, _, agent, _)) if agent.id == "opencode")
        );
    }
    #[test]
    fn unavailable_agents_and_discovery_failure_do_not_launch() {
        let (mut a, rx, tx) = connected_app();
        press(&mut a, KeyCode::Char('h'));
        let seq = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => unreachable!(),
        };
        let mut context = launch_context();
        context.agents.clear();
        tx.send(Response::Discovered(seq, Ok(context))).unwrap();
        a.receive();
        assert!(a.handoff.is_empty());
        assert!(a.toast.as_ref().is_some_and(|toast| toast.error));
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Char('h'));
        let next = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => unreachable!(),
        };
        tx.send(Response::Discovered(
            next,
            Err(std::io::Error::other("workspace unavailable")),
        ))
        .unwrap();
        a.receive();
        assert!(a.handoff.is_empty());
        assert!(
            a.toast
                .as_ref()
                .is_some_and(|toast| toast.message.contains("workspace unavailable"))
        );
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn disconnected_handoff_queues_show_toast_and_leave_ui_available() {
        let mut a = app();
        let (requests, rx) = mpsc::channel();
        drop(rx);
        a.requests = requests;
        press(&mut a, KeyCode::Char('h'));
        assert!(a.handoff.is_empty());
        assert!(a.modal.is_none());
        assert!(a.toast.as_ref().is_some_and(|toast| toast.error));

        let (mut a, rx, tx) = connected_app();
        press(&mut a, KeyCode::Char('h'));
        let seq = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => unreachable!(),
        };
        let (requests, receiver) = mpsc::channel();
        drop(receiver);
        a.requests = requests;
        tx.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(a.handoff.is_empty());
        assert!(
            a.toast
                .as_ref()
                .is_some_and(|toast| toast.message.contains("could not be queued"))
        );
    }
    #[test]
    fn handoff_result_sets_toast_and_refreshes_tracked_worktrees() {
        let (mut a, rx, tx) = connected_app();
        a.beads[0].data["issue_type"] = json!("task");
        a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
        press(&mut a, KeyCode::Char('h'));
        let seq = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => unreachable!(),
        };
        tx.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(matches!(rx.try_recv(), Ok(Request::Launch(..))));
        a.worktrees_inflight = true;
        tx.send(Response::Launched(
            seq,
            Ok(LaunchOutcome {
                message: "Prompt sent · workspace ws-new".into(),
                reserved: false,
            }),
        ))
        .unwrap();
        a.receive();
        assert!(a.handoff.is_empty());
        assert_eq!(display_status(&a.beads[0]), "ready");
        assert!(
            rx.try_recv().is_err(),
            "manual handoff must not clear Ready"
        );
        assert!(a.worktrees_refresh_pending);
        assert!(a.accepted_handoffs.contains("b0"));
        tx.send(Response::Worktrees(Ok(Vec::new()))).unwrap();
        a.receive();
        assert!(a.accepted_handoffs.contains("b0"));
        assert!(a.modal.is_none());
        assert!(
            a.toast
                .as_ref()
                .is_some_and(|toast| !toast.error && toast.expires_at.is_some())
        );
        assert_eq!(a.notice.as_deref(), Some("Prompt sent · workspace ws-new"));
        tx.send(Response::Launched(seq, Err(std::io::Error::other("stale"))))
            .unwrap();
        a.receive();
        assert!(
            !a.toast.as_ref().unwrap().error,
            "stale result cannot replace success"
        );
    }
    #[test]
    fn failed_ready_handoffs_remove_only_the_failed_issues_ready_label() {
        for scheduled in [false, true] {
            for stage in ["discovery", "harness", "launch"] {
                let (mut a, requests, replies) = connected_app();
                for bead in &mut a.beads {
                    bead.data["labels"] = json!([beads::READY_LABEL, "keep"]);
                }
                a.start_launch(a.beads[0].clone(), scheduled);
                let Request::Discover(seq) = requests.try_recv().unwrap() else {
                    panic!()
                };
                let response = match stage {
                    "discovery" => {
                        Response::Discovered(seq, Err(std::io::Error::other("original error")))
                    }
                    "harness" => {
                        let mut context = launch_context();
                        context.agents.clear();
                        Response::Discovered(seq, Ok(context))
                    }
                    _ => {
                        replies
                            .send(Response::Discovered(seq, Ok(launch_context())))
                            .unwrap();
                        a.receive();
                        assert!(matches!(
                            requests.try_recv(),
                            Ok(Request::Launch(..) | Request::LaunchReady(..))
                        ));
                        Response::Launched(
                            seq,
                            Err(std::io::Error::other("original error; inspect worktree")),
                        )
                    }
                };
                replies.send(response).unwrap();
                a.receive();
                let Request::ResetFailedHandoff(reset_seq, id, message) =
                    requests.try_recv().unwrap()
                else {
                    panic!()
                };
                assert_eq!(reset_seq, seq);
                assert_eq!(id, "b0");
                assert!(a.handoff.is_empty());
                assert!(beads::is_ready(&a.beads[0]), "wait for confirmed mutation");
                replies
                    .send(Response::FailedHandoffReset(
                        seq,
                        id,
                        message.clone(),
                        Ok(()),
                    ))
                    .unwrap();
                a.receive();
                assert_eq!(display_status(&a.beads[0]), "open");
                assert_eq!(a.beads[0].data["labels"], json!(["keep"]));
                assert!(beads::is_ready(&a.beads[1]), "do not cascade");
                assert!(a.notice.as_ref().unwrap().contains(&message));
                assert!(a.toast.as_ref().unwrap().error);
                assert!(matches!(requests.try_recv(), Ok(Request::List)));
                assert!(a.scheduler_attempted.contains("b0"));
            }
        }
    }

    #[test]
    fn failed_handoff_reset_preserves_advanced_status_and_reports_reset_errors() {
        for fails in [false, true] {
            let (mut a, requests, replies) = connected_app();
            a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
            a.beads[0].status = "in_progress".into();
            a.mutation_busy = true;
            a.ready_pending = true;
            let result = if fails {
                Err(std::io::Error::other("database unavailable"))
            } else {
                Ok(())
            };
            replies
                .send(Response::FailedHandoffReset(
                    0,
                    "b0".into(),
                    "launch error".into(),
                    result,
                ))
                .unwrap();
            a.receive();
            assert_eq!(a.beads[0].status, "in_progress");
            assert!(
                a.mutation_busy && a.ready_pending,
                "preserve unrelated mutation"
            );
            assert_eq!(
                a.beads[0].data["labels"].as_array().unwrap().len(),
                usize::from(fails)
            );
            assert!(a.notice.as_ref().unwrap().contains("launch error"));
            if fails {
                assert!(a.notice.as_ref().unwrap().contains("database unavailable"));
            }
            assert!(matches!(requests.try_recv(), Ok(Request::List)));
        }
    }

    #[test]
    fn ready_handoff_queue_failures_explain_when_ready_cannot_be_removed() {
        for discovery in [false, true] {
            let (mut a, requests, replies) = connected_app();
            a.beads[0].data["labels"] = json!([beads::READY_LABEL]);
            if discovery {
                drop(requests);
                a.start_launch(a.beads[0].clone(), false);
            } else {
                a.start_launch(a.beads[0].clone(), false);
                let Request::Discover(seq) = requests.try_recv().unwrap() else {
                    panic!()
                };
                drop(requests);
                replies
                    .send(Response::Discovered(seq, Ok(launch_context())))
                    .unwrap();
                a.receive();
            }
            assert!(a.handoff.is_empty());
            assert!(
                a.notice
                    .as_ref()
                    .unwrap()
                    .contains("Ready removal could not be queued")
            );
            assert!(
                beads::is_ready(&a.beads[0]),
                "never claim an unconfirmed reset"
            );
            assert!(a.scheduler_attempted.contains("b0"));
        }
    }

    #[test]
    fn failed_handoff_shows_recovery_error_without_modal() {
        let (mut a, rx, tx) = connected_app();
        press(&mut a, KeyCode::Char('h'));
        let seq = match rx.try_recv().unwrap() {
            Request::Discover(seq) => seq,
            _ => unreachable!(),
        };
        tx.send(Response::Discovered(seq, Ok(launch_context())))
            .unwrap();
        a.receive();
        assert!(matches!(rx.try_recv(), Ok(Request::Launch(..))));
        tx.send(Response::Launched(
            seq,
            Err(std::io::Error::other(
                "worktree may exist; inspect before retrying",
            )),
        ))
        .unwrap();
        a.receive();
        assert!(a.handoff.is_empty());
        assert!(
            rx.try_recv().is_err(),
            "non-Ready handoff must not change labels"
        );
        assert!(a.modal.is_none());
        assert!(
            a.toast.as_ref().is_some_and(
                |toast| toast.error && toast.message.contains("inspect before retrying")
            )
        );
    }
    fn respond(a: &mut App, response: Response) {
        let (tx, rx) = mpsc::channel();
        a.responses = rx;
        tx.send(response).unwrap();
        a.receive();
    }
    fn connected_app() -> (App, mpsc::Receiver<Request>, mpsc::Sender<Response>) {
        let mut a = app();
        let (requests, rx) = mpsc::channel();
        let (tx, responses) = mpsc::channel();
        a.requests = requests;
        a.responses = responses;
        (a, rx, tx)
    }

    #[test]
    fn receive_reports_drained_responses_even_when_stale_or_failed() {
        let (mut a, _requests, replies) = connected_app();
        let now = Instant::now();
        assert!(!a.receive_at(now));
        replies
            .send(Response::Show("stale".into(), 99, Ok(json!({}))))
            .unwrap();
        replies
            .send(Response::List(Err(std::io::Error::other("offline"))))
            .unwrap();
        assert!(event::redraw_due(a.receive_at(now), None, now));
        assert_eq!(a.list_error.as_deref(), Some("offline"));
        assert!(!a.receive_at(now));
        drop(replies);
        assert!(!a.receive_at(now));
    }

    #[test]
    fn polling_reports_visible_loading_and_queue_errors_but_not_background_reads() {
        let (mut a, requests, _replies) = connected_app();
        let now = Instant::now();
        a.next_refresh = now;
        a.worktrees_next_refresh = now;
        assert!(!a.poll_refresh(now));
        assert!(!a.poll_worktrees(now));
        assert!(matches!(requests.try_recv().unwrap(), Request::List));
        assert!(requests.try_recv().is_ok());
        a.worktrees_inflight = false;
        a.worktrees_discovered = false;
        assert!(a.poll_worktrees(now));
        assert!(a.worktrees_loading);
        assert!(!a.poll_worktrees(now));

        drop(requests);
        a.refresh_inflight = false;
        a.worktrees_inflight = false;
        assert!(a.poll_refresh(now));
        assert!(a.list_error.is_some());
        assert!(a.poll_worktrees(now));
        assert!(!a.worktrees_loading);
        assert!(a.worktrees_error.is_some());
        assert!(!a.poll_refresh(now));
        assert!(!a.poll_worktrees(now));
    }

    #[test]
    fn scheduler_reports_progress_until_eligible_work_is_drained() {
        let (mut a, _requests, _replies) = connected_app();
        for bead in &mut a.beads {
            bead.data["labels"] = json!(["ontrack:ready", "ontrack:unassigned"]);
        }
        for count in 1..=a.beads.len() {
            assert!(a.poll_scheduler());
            assert_eq!(a.scheduler_attempted.len(), count);
            assert!(a.toast.as_ref().unwrap().error);
        }
        assert!(!a.poll_scheduler());
    }

    #[test]
    fn settled_render_consumes_toast_deadline_without_an_expired_timer_spin() {
        let (mut a, _requests, _replies) = connected_app();
        let start = Instant::now();
        let now = start + Duration::from_secs(2);
        let expires = now + Duration::from_millis(50);
        a.toast = Some(Toast {
            message: "Done".into(),
            error: false,
            expires_at: Some(expires),
        });
        let deadline = ui::next_visual_update(&a, start, now);
        assert_eq!(deadline, Some(expires));
        assert!(!event::redraw_due(false, deadline, now));
        assert!(event::redraw_due(false, deadline, expires));
        let deadline = ui::next_visual_update(&a, start, expires);
        assert_eq!(deadline, None);
        assert!(!event::redraw_due(false, deadline, expires));
        assert_eq!(
            event::wait_timeout(deadline, expires),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn planning_shortcuts_do_not_open_dialogs_or_queue_requests() {
        let (mut app, requests, _) = connected_app();
        app.selected = Some(0);
        app.beads[0].data["issue_type"] = json!("epic");
        for focus_detail in [false, true] {
            app.focus_detail = focus_detail;
            for key in ['p', 'g'] {
                press(&mut app, KeyCode::Char(key));
                assert!(app.modal.is_none());
                assert!(requests.try_recv().is_err());
            }
        }
    }

    #[test]
    fn legacy_planning_repolls_pending_output_until_agent_asks() {
        let (mut app, requests, replies) = connected_app();
        let seq = 1;
        let marker = "ONTRACK_PLAN_TEST".to_owned();
        app.modal = Some(Modal::Planning(PlanningForm {
            seq,
            proposal: String::new(),
            input: String::new(),
            marker: marker.clone(),
            round: 0,
            session: None,
            stage: PlanningStage::Waiting,
            selected: 0,
            poll_inflight: false,
            next_poll: Instant::now(),
            scroll: 0,
        }));
        let session = crate::epic::Session {
            workspace_id: "w1".into(),
            tab_id: "w1:t2".into(),
            pane_id: "w1:p2".into(),
        };
        let now = Instant::now();
        replies
            .send(Response::PlanLaunched(seq, Ok(session.clone())))
            .unwrap();
        app.receive_at(now);
        app.poll_planning(now);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::PlanRead(read_seq, read_session, read_marker))
                if read_seq == seq && read_session == session && read_marker == marker
        ));
        app.poll_planning(now + Duration::from_secs(2));
        assert!(requests.try_recv().is_err(), "one read at a time");

        replies
            .send(Response::PlanRead(seq, marker.clone(), Ok(None)))
            .unwrap();
        app.receive_at(now);
        app.poll_planning(now + Duration::from_secs(2) - Duration::from_nanos(1));
        assert!(requests.try_recv().is_err());
        app.poll_planning(now + Duration::from_secs(2));
        assert!(matches!(requests.try_recv(), Ok(Request::PlanRead(..))));

        replies
            .send(Response::PlanRead(
                seq,
                marker,
                Ok(Some(crate::planning::Message::Question {
                    text: "Which scope?".into(),
                    options: vec!["Small".into(), "Large".into()],
                })),
            ))
            .unwrap();
        app.receive_at(now + Duration::from_secs(2));
        assert!(matches!(
            app.modal,
            Some(Modal::Planning(PlanningForm {
                stage: PlanningStage::Question { .. },
                ..
            }))
        ));
        assert!(
            requests.try_recv().is_err(),
            "polling stops for the question"
        );
    }

    fn take_show(rx: &mpsc::Receiver<Request>, expected_id: &str) -> u64 {
        match rx.try_recv() {
            Ok(Request::Show(id, seq)) => {
                assert_eq!(id, expected_id);
                seq
            }
            _ => panic!("missing show for {expected_id}"),
        }
    }
    fn take_harness_discovery(rx: &mpsc::Receiver<Request>) -> u64 {
        match rx.try_recv() {
            Ok(Request::DiscoverHarnesses(seq)) => seq,
            _ => panic!("missing harness discovery"),
        }
    }
    fn supply_harnesses(app: &mut App, seq: u64, kinds: &[&str]) {
        let (tx, responses) = mpsc::channel();
        app.responses = responses;
        tx.send(Response::HarnessesDiscovered(
            seq,
            Ok(kinds.iter().map(|kind| (*kind).to_owned()).collect()),
        ))
        .unwrap();
        app.receive();
    }
    #[test]
    fn new_form_uses_only_discovered_harnesses_and_saves_selected_label() {
        let (mut a, rx, tx) = connected_app();
        press(&mut a, KeyCode::Char('n'));
        let seq = take_harness_discovery(&rx);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert!(form.harnesses.is_empty());
        assert!(form.harness_loading);
        let Some(Modal::Create(form)) = a.modal.as_mut() else {
            panic!()
        };
        form.title = "New task".into();
        form.focus = 4;
        press(&mut a, KeyCode::Enter);
        assert!(rx.try_recv().is_err());
        assert!(a.notice.as_deref().unwrap().contains("Checking Herdr"));
        tx.send(Response::HarnessesDiscovered(
            seq,
            Ok(vec!["opencode".into(), "hermes".into()]),
        ))
        .unwrap();
        a.receive();
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.harnesses, ["opencode", "hermes"]);
        assert_eq!(form.harness, 1);
        // Right wraps through the installed kinds and Unassigned.
        let Some(Modal::Create(form)) = a.modal.as_mut() else {
            panic!()
        };
        form.focus = 0;
        for _ in 0..3 {
            press(&mut a, KeyCode::Tab);
        }
        press(&mut a, KeyCode::Right);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.harnesses[form.harness - 1], "hermes");
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Right);
        let Some(Modal::Create(form)) = a.modal.as_mut() else {
            panic!()
        };
        assert_eq!(form.harness, 1);
        form.title = "New task".into();
        form.focus = 4;
        press(&mut a, KeyCode::Enter);
        assert!(
            matches!(rx.try_recv(), Ok(Request::Create(input)) if input.harness.as_deref() == Some("opencode"))
        );
    }

    #[test]
    fn new_form_defaults_to_codex_when_current_and_unassigned_on_failure() {
        let (mut a, rx, tx) = connected_app();
        press(&mut a, KeyCode::Char('n'));
        let first = take_harness_discovery(&rx);
        tx.send(Response::HarnessesDiscovered(
            first,
            Ok(vec!["pi".into(), "codex".into()]),
        ))
        .unwrap();
        a.receive();
        assert!(matches!(&a.modal, Some(Modal::Create(form)) if form.harness == 2));
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('n'));
        let second = take_harness_discovery(&rx);
        tx.send(Response::HarnessesDiscovered(
            first,
            Ok(vec!["claude".into()]),
        ))
        .unwrap();
        tx.send(Response::HarnessesDiscovered(
            second,
            Err(std::io::Error::other("offline")),
        ))
        .unwrap();
        a.receive();
        assert!(
            matches!(&a.modal, Some(Modal::Create(form)) if form.harness == 0 && form.harnesses.is_empty() && form.harness_error.as_deref().unwrap().contains("offline"))
        );
    }
    #[test]
    fn saved_default_agent_applies_to_new_and_child_forms_with_safe_fallback() {
        let (mut a, rx, tx) = connected_app();
        a.beads[0].data["issue_type"] = serde_json::json!("task");
        a.settings.default_agent = DefaultAgent::Kind("hermes");
        press(&mut a, KeyCode::Char('n'));
        let seq = take_harness_discovery(&rx);
        tx.send(Response::HarnessesDiscovered(
            seq,
            Ok(vec!["codex".into(), "hermes".into()]),
        ))
        .unwrap();
        a.receive();
        assert!(matches!(&a.modal, Some(Modal::Create(form)) if form.harness == 2));
        press(&mut a, KeyCode::Esc);

        a.settings.default_agent = DefaultAgent::Unassigned;
        press(&mut a, KeyCode::Char('N'));
        let seq = take_harness_discovery(&rx);
        tx.send(Response::HarnessesDiscovered(seq, Ok(vec!["codex".into()])))
            .unwrap();
        a.receive();
        assert!(
            matches!(&a.modal, Some(Modal::Create(form)) if form.parent_id.is_some() && form.harness == 0)
        );
        press(&mut a, KeyCode::Esc);

        a.settings.default_agent = DefaultAgent::Kind("hermes");
        press(&mut a, KeyCode::Char('n'));
        let seq = take_harness_discovery(&rx);
        tx.send(Response::HarnessesDiscovered(seq, Ok(vec!["codex".into()])))
            .unwrap();
        a.receive();
        assert!(matches!(&a.modal, Some(Modal::Create(form)) if form.harness == 1));
        assert_eq!(a.settings.default_agent, DefaultAgent::Kind("hermes"));

        let existing = serde_json::json!({
            "title": "Existing task",
            "issue_type": "task",
            "priority": 2,
            "labels": ["ontrack:agent:codex"]
        });
        let edit = edit_form(&existing).unwrap();
        assert_eq!(edit.harnesses[edit.harness - 1], "codex");
    }

    #[test]
    fn settings_agent_choice_saves_and_reloads() {
        let (mut a, _, _) = connected_app();
        let dir =
            std::env::temp_dir().join(format!("ontrack-agent-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        a.settings_path = Some(path.clone());
        press(&mut a, KeyCode::Char('s'));
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Right);
        assert_eq!(a.settings.default_agent, DefaultAgent::Unassigned);
        assert_eq!(
            settings::load_from(&path).unwrap().default_agent,
            DefaultAgent::Unassigned
        );
        press(&mut a, KeyCode::Right);
        assert_eq!(
            a.settings.default_agent,
            DefaultAgent::Kind(beads::HARNESSES[0])
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn startup_and_interval_polling_wait_for_completion_without_queue_growth() {
        let (mut a, rx, tx) = connected_app();
        a.beads.clear();
        a.clear_selection();
        a.list_loading = true;
        let now = Instant::now();
        a.refresh_at(now);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        assert!(a.list_loading);
        for elapsed in [0, 1, 10, 100] {
            a.poll_refresh(now + Duration::from_secs(elapsed));
        }
        assert!(rx.try_recv().is_err());
        assert!(!a.refresh_pending);
        let completed = now + Duration::from_secs(100);
        tx.send(Response::List(Ok(vec![]))).unwrap();
        a.receive_at(completed);
        assert!(!a.list_loading);
        assert!(!a.refresh_inflight);
        a.poll_refresh(completed + REFRESH_INTERVAL - Duration::from_nanos(1));
        assert!(rx.try_recv().is_err());
        a.poll_refresh(completed + REFRESH_INTERVAL);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        assert!(!a.list_loading, "empty background polls must not flicker");
        a.poll_refresh(completed + REFRESH_INTERVAL * 10);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn repeated_refresh_requests_coalesce_into_one_trailing_snapshot() {
        let (mut a, rx, tx) = connected_app();
        let now = Instant::now();
        a.refresh_at(now);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        for _ in 0..20 {
            a.refresh();
        }
        assert!(rx.try_recv().is_err());
        assert!(a.refresh_pending);
        tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
        a.receive_at(now);
        take_show(&rx, "b0");
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        assert!(!a.refresh_pending);
        a.poll_refresh(now + REFRESH_INTERVAL * 20);
        assert!(rx.try_recv().is_err());
        tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
        a.receive_at(now + REFRESH_INTERVAL * 20);
        take_show(&rx, "b0");
        assert!(rx.try_recv().is_err());
        assert!(!a.refresh_inflight);
    }
    #[test]
    fn mutations_invalidate_older_inflight_snapshot_and_keep_created_id_for_followup() {
        for create in [true, false] {
            let (mut a, rx, tx) = connected_app();
            let now = Instant::now();
            a.refresh_at(now);
            assert!(matches!(rx.try_recv(), Ok(Request::List)));
            tx.send(if create {
                Response::Created(Ok("new-id".into()))
            } else {
                Response::Deleted(Ok(()))
            })
            .unwrap();
            a.receive_at(now);
            assert!(a.refresh_pending);
            assert!(rx.try_recv().is_err());
            tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
            a.receive_at(now);
            take_show(&rx, "b0");
            assert!(matches!(rx.try_recv(), Ok(Request::List)));
            assert_eq!(a.preferred_id.as_deref(), create.then_some("new-id"));
            let mut items = a.beads.clone();
            if create {
                let mut new = items[0].clone();
                new.id = "new-id".into();
                items.push(new);
            } else {
                items.remove(0);
            }
            tx.send(Response::List(Ok(items))).unwrap();
            a.receive_at(now);
            take_show(&rx, if create { "new-id" } else { "b1" });
            assert!(rx.try_recv().is_err());
            assert!(a.preferred_id.is_none());
        }
    }
    #[test]
    fn unchanged_polls_preserve_detail_scroll_drawers_columns_and_modals() {
        for modal in [
            None,
            Some(Modal::Create(CreateForm {
                title: "unfinished draft".into(),
                description: "keep this".into(),
                cursor: 3,
                dropdown: Some(2),
                ..Default::default()
            })),
            Some(Modal::ConfirmDelete {
                id: "b0".into(),
                title: "issue 0".into(),
            }),
        ] {
            let (mut a, rx, tx) = connected_app();
            a.beads[3].status = "closed".into();
            a.column_offsets[5] = 3;
            a.focus_detail = true;
            a.column_offsets[0] = 7;
            a.detail_offset = 23;
            a.detail = Some(json!({"id":"b0", "comments":[{"body":"keep me"}]}));
            a.modal = modal.clone();
            a.modal_ready = false;
            a.notice = Some("keep notice".into());
            let detail = a.detail.clone();
            let now = Instant::now();
            a.poll_refresh(now);
            assert!(matches!(rx.try_recv(), Ok(Request::List)));
            assert!(!a.list_loading);
            assert!(!a.detail_loading);
            tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
            a.receive_at(now);
            let seq = take_show(&rx, "b0");
            assert!(!a.detail_loading);
            assert_eq!(a.detail, detail);
            assert_eq!(a.selected, Some(0));
            assert_eq!(a.column_offsets[5], 3);
            assert!(a.focus_detail);
            assert_eq!((a.column_offsets[0], a.detail_offset), (7, 23));
            assert_eq!(a.modal, modal);
            assert!(!a.modal_ready);
            assert_eq!(a.notice.as_deref(), Some("keep notice"));
            // Show-only changes must be observed even though the list was identical.
            let updated = json!({"id":"b0", "comments":[{"body":"new comment"}]});
            tx.send(Response::Show("b0".into(), seq, Ok(updated.clone())))
                .unwrap();
            a.receive_at(now);
            assert_eq!(a.detail, Some(updated));
            assert_eq!((a.column_offsets[0], a.detail_offset), (7, 23));
            assert_eq!(a.modal, modal);
        }
    }
    #[test]
    fn polling_applies_external_metadata_additions_and_removals_by_id() {
        let (mut a, rx, tx) = connected_app();
        a.select(2);
        take_show(&rx, "b2");
        a.detail_offset = 12;
        a.column_offsets[0] = 3;
        let now = Instant::now();
        a.poll_refresh(now);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        let mut items = a.beads.clone();
        items[2].title = "externally renamed".into();
        items[2].priority = 0;
        items[2].status = "blocked".into();
        items[2].data = json!({"id":"b2", "labels":["external"], "assignee":"someone"});
        let mut added = items[0].clone();
        added.id = "added".into();
        items.push(added);
        items.retain(|b| b.id != "b1");
        items.reverse();
        tx.send(Response::List(Ok(items))).unwrap();
        a.receive_at(now);
        let selected = &a.beads[a.selected.unwrap()];
        assert_eq!(selected.id, "b2");
        assert_eq!(selected.title, "externally renamed");
        assert_eq!(selected.priority, 0);
        assert_eq!(selected.status, "blocked");
        assert_eq!(selected.data["assignee"], "someone");
        assert!(a.beads.iter().any(|b| b.id == "added"));
        assert!(!a.beads.iter().any(|b| b.id == "b1"));
        assert_eq!((a.column_offsets[0], a.detail_offset), (3, 12));
        let seq = take_show(&rx, "b2");
        let updated = a.beads[a.selected.unwrap()].data.clone();
        tx.send(Response::Show("b2".into(), seq, Ok(updated.clone())))
            .unwrap();
        a.receive_at(now);
        assert_eq!(a.detail, Some(updated));
        a.poll_refresh(now + REFRESH_INTERVAL);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        let items = a.beads.iter().filter(|b| b.id != "b2").cloned().collect();
        tx.send(Response::List(Ok(items))).unwrap();
        a.receive_at(now + REFRESH_INTERVAL);
        assert!(a.selected.is_none());
        assert_eq!(a.focused_column, 2);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn transient_list_and_show_errors_keep_data_and_retry_until_success() {
        let (mut a, rx, tx) = connected_app();
        let now = Instant::now();
        let old_data = a.detail.clone();
        let old_ids = a.beads.iter().map(|b| b.id.clone()).collect::<Vec<_>>();
        a.detail_offset = 9;
        a.poll_refresh(now);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        tx.send(Response::List(Err(std::io::Error::other("list offline"))))
            .unwrap();
        a.receive_at(now);
        assert_eq!(a.list_error.as_deref(), Some("list offline"));
        assert_eq!(a.detail, old_data);
        assert_eq!(
            a.beads.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
            old_ids
        );
        a.poll_refresh(now + REFRESH_INTERVAL);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        assert_eq!(a.list_error.as_deref(), Some("list offline"));
        tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
        a.receive_at(now + REFRESH_INTERVAL);
        assert!(a.list_error.is_none());
        let seq = take_show(&rx, "b0");
        tx.send(Response::Show(
            "b0".into(),
            seq,
            Err(std::io::Error::other("show offline")),
        ))
        .unwrap();
        a.receive_at(now + REFRESH_INTERVAL);
        assert_eq!(a.detail_error.as_deref(), Some("show offline"));
        assert_eq!(a.detail, old_data);
        assert_eq!(a.detail_offset, 9);
        a.poll_refresh(now + REFRESH_INTERVAL * 2);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
        a.receive_at(now + REFRESH_INTERVAL * 2);
        let seq = take_show(&rx, "b0");
        assert_eq!(a.detail_error.as_deref(), Some("show offline"));
        let recovered = json!({"id":"b0", "comments":["recovered"]});
        tx.send(Response::Show("b0".into(), seq, Ok(recovered.clone())))
            .unwrap();
        a.receive_at(now + REFRESH_INTERVAL * 2);
        assert_eq!(a.detail, Some(recovered));
        assert!(a.detail_error.is_none());
        assert_eq!(a.detail_offset, 9);
    }
    #[test]
    fn newer_snapshots_reject_older_detail_successes_and_errors_for_same_id() {
        let (mut a, rx, tx) = connected_app();
        let now = Instant::now();
        a.poll_refresh(now);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
        a.receive_at(now);
        let old_seq = take_show(&rx, "b0");
        a.poll_refresh(now + REFRESH_INTERVAL);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        let mut items = a.beads.clone();
        items[0].title = "new snapshot".into();
        tx.send(Response::List(Ok(items))).unwrap();
        a.receive_at(now + REFRESH_INTERVAL);
        let new_seq = take_show(&rx, "b0");
        assert_ne!(old_seq, new_seq);
        let current = json!({"id":"b0", "description":"new detail"});
        tx.send(Response::Show("b0".into(), new_seq, Ok(current.clone())))
            .unwrap();
        tx.send(Response::Show(
            "b0".into(),
            old_seq,
            Ok(json!({"stale":true})),
        ))
        .unwrap();
        tx.send(Response::Show(
            "b0".into(),
            old_seq,
            Err(std::io::Error::other("stale error")),
        ))
        .unwrap();
        a.receive_at(now + REFRESH_INTERVAL);
        assert_eq!(a.detail, Some(current));
        assert!(a.detail_error.is_none());
    }
    #[test]
    fn disconnected_request_channel_does_not_latch_refresh_inflight() {
        let mut a = app();
        let now = Instant::now();
        a.refresh_at(now);
        assert!(!a.refresh_inflight);
        assert!(!a.list_loading);
        assert!(a.list_error.is_some());
        let (requests, rx) = mpsc::channel();
        a.requests = requests;
        a.poll_refresh(now + REFRESH_INTERVAL - Duration::from_nanos(1));
        assert!(rx.try_recv().is_err());
        a.poll_refresh(now + REFRESH_INTERVAL);
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        assert!(a.refresh_inflight);
    }
    #[test]
    fn settings_change_autosaves_and_close_keeps_committed_theme() {
        let mut a = app();
        let dir = std::env::temp_dir().join(format!("ontrack-app-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        a.settings_path = Some(dir.join("ontrack/settings.json"));
        a.settings_load_error = Some("startup settings failed".into());
        a.notice = Some("startup settings failed".into());
        press(&mut a, KeyCode::Char('s'));
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                draft: Settings {
                    theme: Theme::Dark,
                    ..
                },
                ..
            })
        ));
        press(&mut a, KeyCode::Char('n'));
        assert!(
            matches!(a.modal, Some(Modal::Settings { .. })),
            "modal keys must not leak to the browser"
        );
        assert!(!a.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(matches!(a.modal, Some(Modal::Settings { .. })));
        press(&mut a, KeyCode::Char(' '));
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                dropdown_open: true,
                ..
            })
        ));
        press(&mut a, KeyCode::Right);
        assert_eq!(
            a.theme,
            Theme::Light,
            "successful change applies immediately"
        );
        assert_eq!(a.settings.theme, Theme::Light);
        assert!(a.settings_load_error.is_none());
        assert!(matches!(a.modal, Some(Modal::Settings { error: None, .. })));
        assert_eq!(
            settings::load_from(a.settings_path.as_deref().unwrap())
                .unwrap()
                .theme,
            Theme::Light
        );
        press(&mut a, KeyCode::Left);
        assert_eq!(a.theme, Theme::Dark, "left returns to the first choice");
        assert_eq!(
            settings::load_from(a.settings_path.as_deref().unwrap())
                .unwrap()
                .theme,
            Theme::Dark
        );
        press(&mut a, KeyCode::Right);
        assert_eq!(
            a.theme,
            Theme::Light,
            "right selects and saves the second choice"
        );
        press(&mut a, KeyCode::Enter);
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                dropdown_open: false,
                ..
            })
        ));
        assert_eq!(
            a.theme,
            Theme::Light,
            "Enter retries/saves the selected option and closes the dropdown"
        );
        press(&mut a, KeyCode::Left);
        assert_eq!(a.theme, Theme::Dark);
        assert_eq!(
            settings::load_from(a.settings_path.as_deref().unwrap())
                .unwrap()
                .theme,
            Theme::Dark
        );
        press(&mut a, KeyCode::Right);
        assert_eq!(a.theme, Theme::Light);
        assert_eq!(
            settings::load_from(a.settings_path.as_deref().unwrap())
                .unwrap()
                .theme,
            Theme::Light
        );
        press(&mut a, KeyCode::Tab);
        assert!(matches!(a.modal, Some(Modal::Settings { focus: 1, .. })));
        assert_eq!(a.theme, Theme::Light, "Tab only moves focus");
        press(&mut a, KeyCode::BackTab);
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none(), "Esc closes Settings without rollback");
        assert_eq!(a.theme, Theme::Light);
        assert_eq!(a.settings.theme, Theme::Light);
        assert_eq!(
            settings::load_from(a.settings_path.as_deref().unwrap())
                .unwrap()
                .theme,
            Theme::Light
        );
        press(&mut a, KeyCode::Char('s'));
        assert_eq!(
            a.theme,
            Theme::Light,
            "reopening does not change the saved theme"
        );
        assert!(matches!(a.modal, Some(Modal::Settings { .. })));
        for _ in 0..8 {
            press(&mut a, KeyCode::Tab);
        }
        press(&mut a, KeyCode::Enter);
        assert!(a.modal.is_none());
        assert_eq!(
            settings::load_from(a.settings_path.as_deref().unwrap())
                .unwrap()
                .theme,
            Theme::Light
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn settings_dialog_surfaces_startup_load_failure_without_dismissing_notice() {
        let mut a = app();
        let startup_error = "Settings could not be loaded: invalid JSON at /home/test/.config/ontrack/settings.json";
        a.settings_load_error = Some(startup_error.into());
        a.notice = Some(startup_error.into());
        press(&mut a, KeyCode::Char('s'));
        assert!(
            matches!(&a.modal, Some(Modal::Settings { error: Some(message), .. }) if message.contains("defaults are active") && message.contains("settings.json"))
        );
        assert_eq!(a.notice.as_deref(), Some(startup_error));
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Down);
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                error: Some(_),
                dropdown_open: false,
                ..
            })
        ));
        assert_eq!(
            a.notice.as_deref(),
            Some(startup_error),
            "row navigation must not clear startup errors"
        );
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.notice.as_deref(), Some(startup_error));
        assert_eq!(a.theme, Theme::Dark);
    }
    #[test]
    fn settings_arrows_wrap_rows_and_reach_cleanup_toggles() {
        let dir = std::env::temp_dir().join(format!("ontrack-settings-nav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        let mut a = app();
        a.settings_path = Some(path.clone());
        press(&mut a, KeyCode::Char('s'));
        press(&mut a, KeyCode::Up);
        assert!(matches!(a.modal, Some(Modal::Settings { focus: 8, .. })));
        press(&mut a, KeyCode::Down);
        assert!(matches!(a.modal, Some(Modal::Settings { focus: 0, .. })));
        press(&mut a, KeyCode::Down);
        assert!(matches!(a.modal, Some(Modal::Settings { focus: 1, .. })));
        press(&mut a, KeyCode::Down);
        assert!(matches!(a.modal, Some(Modal::Settings { focus: 2, .. })));
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.theme, Theme::Dark);
        assert!(a.settings.auto_merge);
        assert!(settings::load_from(&path).unwrap().auto_merge);
        press(&mut a, KeyCode::Down);
        assert!(matches!(a.modal, Some(Modal::Settings { focus: 4, .. })));
        press(&mut a, KeyCode::Enter);
        assert!(!a.settings.auto_merge_children);
        assert!(!settings::load_from(&path).unwrap().auto_merge_children);
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
        assert!(a.settings.auto_merge);
        press(&mut a, KeyCode::Char('s'));
        assert!(a.settings.animate_drawers);
        for _ in 0..7 {
            press(&mut a, KeyCode::Down);
        }
        a.set_drawer(false, true, Instant::now());
        press(&mut a, KeyCode::Enter);
        assert!(!a.settings.animate_drawers);
        assert!(a.drawer_animation.is_none());
        assert!(!settings::load_from(&path).unwrap().animate_drawers);
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('s'));
        for _ in 0..8 {
            press(&mut a, KeyCode::Tab);
        }
        press(&mut a, KeyCode::Enter);
        assert!(a.modal.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_agent_limit_changes_save_and_survive_reopening() {
        let dir = std::env::temp_dir().join(format!("ontrack-agent-limit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        let mut a = app();
        a.settings_path = Some(path.clone());
        press(&mut a, KeyCode::Char('s'));
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Tab);
        assert!(matches!(a.modal, Some(Modal::Settings { focus: 2, .. })));
        for expected in [4, 3, 2, 1] {
            press(&mut a, KeyCode::Left);
            assert_eq!(
                a.settings.agent_limit,
                settings::AgentLimit::Limited(expected)
            );
            assert_eq!(
                settings::load_from(&path).unwrap().agent_limit,
                a.settings.agent_limit
            );
        }
        press(&mut a, KeyCode::Left);
        assert_eq!(a.settings.agent_limit, settings::AgentLimit::Unlimited);
        press(&mut a, KeyCode::Enter);
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                dropdown_open: true,
                ..
            })
        ));
        press(&mut a, KeyCode::Right);
        assert_eq!(a.settings.agent_limit, settings::AgentLimit::Limited(1));
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
        press(&mut a, KeyCode::Char('s'));
        assert!(
            matches!(a.modal, Some(Modal::Settings { draft, .. }) if draft.agent_limit == settings::AgentLimit::Limited(1))
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn settings_selector_cycles_all_catalog_entries_and_wraps_both_directions() {
        let dir =
            std::env::temp_dir().join(format!("ontrack-settings-cycle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = app();
        a.settings_path = Some(dir.join("settings.json"));
        press(&mut a, KeyCode::Char('s'));
        press(&mut a, KeyCode::Enter);
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                dropdown_open: true,
                ..
            })
        ));
        for theme in Theme::ALL
            .into_iter()
            .cycle()
            .skip(1)
            .take(Theme::ALL.len() - 1)
        {
            press(&mut a, KeyCode::Right);
            assert_eq!(a.theme, theme);
        }
        assert_eq!(a.theme, *Theme::ALL.last().unwrap());
        press(&mut a, KeyCode::Right);
        assert_eq!(
            a.theme,
            Theme::Dark,
            "forward wraps from the last catalog entry"
        );
        press(&mut a, KeyCode::Left);
        assert_eq!(
            a.theme,
            *Theme::ALL.last().unwrap(),
            "backward wraps from the first entry"
        );
        press(&mut a, KeyCode::Esc);
        assert_eq!(
            a.theme,
            *Theme::ALL.last().unwrap(),
            "closing the dropdown retains the last successfully saved theme"
        );
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                dropdown_open: false,
                ..
            })
        ));
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
        let reloaded = settings::load_from(a.settings_path.as_deref().unwrap()).unwrap();
        assert_eq!(
            reloaded.theme, a.settings.theme,
            "reopening reloads the committed theme"
        );
        press(&mut a, KeyCode::Char('s'));
        assert!(matches!(a.modal, Some(Modal::Settings { draft, .. }) if draft == reloaded));
        press(&mut a, KeyCode::Esc);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn detail_focus_and_scroll_work() {
        let mut a = app();
        assert!(!a.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert!(a.focus_detail);
        a.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(a.detail_offset, 1);
        a.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(a.detail_offset, 7);
    }
    #[test]
    fn detail_drawer_arrows_leave_columns_unchanged_and_tab_toggles_drawer() {
        let mut a = app();
        let selected = a.selected;
        let detail = a.detail.clone();
        press(&mut a, KeyCode::Tab);
        assert!(a.focus_detail);
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Right);
        assert!(a.focus_detail);
        assert_eq!(a.focused_column, 0);
        assert_eq!(a.selected, selected);
        assert_eq!(a.detail, detail);
        press(&mut a, KeyCode::Tab);
        assert!(!a.focus_detail);
        press(&mut a, KeyCode::Tab);
        assert!(a.focus_detail);
    }
    #[test]
    fn every_drawer_close_key_starts_a_closing_transition_and_restores_board_input() {
        for (worktrees, close_keys) in [
            (false, vec![KeyCode::Esc, KeyCode::Tab, KeyCode::Enter]),
            (
                true,
                vec![KeyCode::Esc, KeyCode::Backspace, KeyCode::Char('w')],
            ),
        ] {
            for close_key in close_keys {
                let mut a = app();
                let selected = a.selected;
                press(
                    &mut a,
                    if worktrees {
                        KeyCode::Char('w')
                    } else {
                        KeyCode::Enter
                    },
                );
                let opening = a.drawer_animation.as_mut().unwrap();
                assert_eq!(opening.to, 1.0);
                opening.started -= Duration::from_millis(300);
                press(&mut a, close_key);
                assert!(!a.focus_detail && !a.worktrees_view);
                assert_eq!(a.selected, selected);
                let closing = a.drawer_animation.as_ref().unwrap();
                assert_eq!(closing.worktrees, worktrees);
                assert_eq!(closing.from, 1.0);
                assert_eq!(closing.to, 0.0);
                press(&mut a, KeyCode::Right);
                assert_eq!(a.focused_column, 1);
            }
        }
    }

    #[test]
    fn drawer_reversals_preserve_fraction_and_disabled_animation_is_immediate() {
        for worktrees in [false, true] {
            let mut a = app();
            let start = Instant::now();
            a.set_drawer(worktrees, true, start);
            let closing_at = start + Duration::from_millis(80);
            let fraction = a.drawer_animation.as_ref().unwrap().fraction(closing_at);
            a.set_drawer(worktrees, false, closing_at);
            assert_eq!(
                a.drawer_animation.as_ref().unwrap().fraction(closing_at),
                fraction
            );
            let reopening_at = closing_at + Duration::from_millis(80);
            let fraction = a.drawer_animation.as_ref().unwrap().fraction(reopening_at);
            assert!(fraction > 0.0 && fraction < 1.0);
            a.set_drawer(worktrees, true, reopening_at);
            assert_eq!(
                a.drawer_animation.as_ref().unwrap().fraction(reopening_at),
                fraction
            );
            assert_eq!(
                a.drawer_animation
                    .as_ref()
                    .unwrap()
                    .fraction(reopening_at + Duration::from_millis(220)),
                1.0
            );
            a.settings.animate_drawers = false;
            a.set_drawer(worktrees, false, reopening_at);
            assert!(a.drawer_animation.is_none());
            assert!(!a.focus_detail && !a.worktrees_view);
            a.set_drawer(worktrees, true, reopening_at);
            assert!(a.drawer_animation.is_none());
            assert_eq!(a.worktrees_view, worktrees);
            assert_eq!(a.focus_detail, !worktrees);
        }
    }
    #[test]
    fn board_arrows_are_safe_when_empty_and_modal_arrows_remain_form_controls() {
        let mut a = app();
        a.beads.clear();
        a.clear_selection();
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Right);
        assert!(a.selected.is_none());
        assert_eq!(a.focused_column, 1);
        a.beads = app().beads;
        a.modal = Some(Modal::Create(CreateForm {
            title: "xy".into(),
            cursor: 1,
            ..Default::default()
        }));
        press(&mut a, KeyCode::Right);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!("form closed")
        };
        assert_eq!(form.cursor, 2);
        press(&mut a, KeyCode::Left);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!("form closed")
        };
        assert_eq!(form.cursor, 1);
        assert!(!a.focus_detail);
        assert_eq!(a.focused_column, 1);
    }
    #[test]
    fn details_focused_arrows_scroll_without_changing_issue_selection() {
        let mut a = app();
        a.focus_detail = true;
        a.detail_offset = 2;
        press(&mut a, KeyCode::Up);
        assert_eq!(a.detail_offset, 1);
        assert_bead(&a, "b0");
        press(&mut a, KeyCode::Down);
        assert_eq!(a.detail_offset, 2);
        assert_bead(&a, "b0");
    }
    #[test]
    fn stale_detail_response_cannot_replace_selection() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.responses = rx;
        let _ = tx.send(Response::Show("b0".into(), 0, Ok(json!({"old":true}))));
        a.select(1);
        a.receive();
        assert_eq!(a.detail, Some(a.beads[1].data.clone()));
    }
    #[test]
    fn stale_response_for_same_id_after_reselection_is_ignored() {
        let mut a = app();
        a.select(1);
        let old_seq = a.detail_seq;
        a.select(0);
        a.select(1);
        let (tx, rx) = mpsc::channel();
        a.responses = rx;
        let _ = tx.send(Response::Show(
            "b1".into(),
            old_seq,
            Ok(json!({"stale":true})),
        ));
        a.receive();
        assert_eq!(a.detail, Some(a.beads[1].data.clone()));
        assert!(a.detail_loading);
    }

    #[test]
    fn successful_refresh_preserves_selection_and_clears_empty_details() {
        let mut a = app();
        a.selected = Some(2);
        a.detail = Some(a.beads[2].data.clone());
        a.detail_offset = 20;
        let (tx, rx) = mpsc::channel();
        a.responses = rx;
        tx.send(Response::List(Ok(vec![
            a.beads[2].clone(),
            a.beads[0].clone(),
        ])))
        .unwrap();
        a.receive();
        assert_eq!(a.beads[a.selected.unwrap()].id, "b2");
        assert_eq!(a.detail_offset, 20);

        a.detail_error = Some("old error".into());
        a.detail_loading = true;
        tx.send(Response::List(Ok(vec![]))).unwrap();
        a.receive();
        assert_eq!(a.selected, None);
        assert!(a.detail.is_none());
        assert!(!a.detail_loading);
        assert!(a.detail_error.is_none());
    }
    #[test]
    fn missing_preferred_id_falls_back_to_visible_issue_not_header() {
        let mut a = app();
        a.preferred_id = Some("missing".into());
        let items = a.beads.clone();
        respond(&mut a, Response::List(Ok(items)));
        assert_bead(&a, "b0");
        assert!(a.preferred_id.is_none());
    }
    #[test]
    fn selecting_uses_cache_and_clears_selection_error() {
        let mut a = app();
        a.detail_error = Some("old error".into());
        a.select(1);
        assert_eq!(a.detail, Some(a.beads[1].data.clone()));
        assert!(a.detail_loading);
        assert!(a.detail_error.is_none());
    }
    #[test]
    fn create_form_defaults_text_and_quit_keys_do_not_leak() {
        let mut a = app();
        press(&mut a, KeyCode::Char('n'));
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!("form not opened")
        };
        assert_eq!((form.issue_type, form.priority), (0, 2));
        for c in ['q', 'r', 'n', 'd', '界'] {
            a.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!("form closed")
        };
        assert_eq!(form.title, "qrnd界");
        assert!(!a.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
        a.mouse(MouseEventKind::Down(MouseButton::Left), 4, 1);
        assert_eq!(a.selected, Some(0));
    }
    #[test]
    fn epic_form_creates_ordinary_bead_without_planning() {
        let mut app = app();
        let (tx, requests) = mpsc::channel();
        app.requests = tx;
        press(&mut app, KeyCode::Char('n'));
        let seq = take_harness_discovery(&requests);
        supply_harnesses(&mut app, seq, &["codex"]);
        let Some(Modal::Create(form)) = app.modal.as_mut() else {
            panic!()
        };
        form.title = "Release".into();
        form.issue_type = beads::BEAD_TYPES
            .iter()
            .position(|kind| *kind == "epic")
            .unwrap();
        form.focus = 1;
        press(&mut app, KeyCode::Tab);
        let Some(Modal::Create(form)) = app.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.focus, 4);
        press(&mut app, KeyCode::Enter);
        assert!(
            matches!(requests.try_recv(), Ok(Request::Create(input)) if input.issue_type == "epic")
        );
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn description_accepts_shortcut_letters_and_shift_enter_but_not_control_chars() {
        let mut a = app();
        press(&mut a, KeyCode::Char('n'));
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Tab);
        for c in ['q', 'r', 'n', 'd'] {
            press(&mut a, KeyCode::Char(c));
        }
        a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        press(&mut a, KeyCode::Char('界'));
        a.modal_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.description, "qrnd\n界");
    }

    #[test]
    fn screenshot_paste_inserts_reference_at_description_caret() {
        let mut form = CreateForm {
            description: "before after".into(),
            description_cursor: 7,
            focus: 1,
            ..Default::default()
        };
        form.insert_screenshot(std::path::Path::new("/shared/.beads/screenshots/test.png"));
        assert_eq!(
            form.description,
            "before ![Screenshot](</shared/.beads/screenshots/test.png>)after"
        );
        assert_eq!(form.description_cursor, form.description.len() - 5);
    }

    #[test]
    fn failed_screenshot_paste_keeps_description() {
        let mut a = app();
        a.modal = Some(Modal::Create(CreateForm {
            description: "keep this".into(),
            description_cursor: 4,
            focus: 1,
            ..Default::default()
        }));
        a.paste_seq = 1;
        a.paste_pending = true;
        let (tx, rx) = mpsc::channel();
        a.responses = rx;
        tx.send(Response::ScreenshotRead(
            1,
            Err(std::io::Error::other("no image")),
        ))
        .unwrap();
        a.receive();
        let Some(Modal::Create(form)) = a.modal else {
            panic!("create form was closed")
        };
        assert_eq!(form.description, "keep this");
        assert!(!a.paste_pending);
        assert!(a.notice.as_deref().unwrap().contains("no image"));
    }

    #[test]
    fn control_v_requests_image_only_in_description_for_create_and_edit() {
        for editing in [false, true] {
            let mut a = app();
            let mut form = CreateForm {
                focus: 1,
                ..Default::default()
            };
            form.description = "draft".into();
            a.modal = Some(if editing {
                Modal::Edit {
                    id: "b0".into(),
                    form,
                }
            } else {
                Modal::Create(form)
            });
            let (tx, rx) = mpsc::channel();
            a.requests = tx;
            a.modal_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));
            assert!(matches!(rx.try_recv(), Ok(Request::ReadScreenshot(1))));
            assert!(a.paste_pending);
            a.modal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
            let form = match a.modal.as_ref().unwrap() {
                Modal::Create(form) | Modal::Edit { form, .. } => form,
                _ => unreachable!(),
            };
            assert_eq!(form.description, "draft");
        }
    }

    #[test]
    fn cancel_form_removes_unsaved_screenshot() {
        let path =
            std::env::temp_dir().join(format!("ontrack-unsaved-screenshot-{}", std::process::id()));
        std::fs::write(&path, b"image").unwrap();
        let mut a = app();
        a.modal = Some(Modal::Create(CreateForm::default()));
        a.created_screenshots.push(path.clone());
        a.modal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(a.modal.is_none());
        assert!(!path.exists());
    }
    #[test]
    fn enter_from_description_focuses_create_without_submitting() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        press(&mut a, KeyCode::Char('n'));
        let seq = take_harness_discovery(&rx);
        supply_harnesses(&mut a, seq, &["codex"]);
        for c in "Title".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Tab);
        for c in "two words".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        press(&mut a, KeyCode::Left);
        a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!("form closed")
        };
        assert_eq!(form.description, "two word\ns");
        assert_eq!(form.focus, 1);
        press(&mut a, KeyCode::Enter);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!("form closed")
        };
        assert_eq!(form.focus, 4);
        assert_eq!(form.description, "two word\ns");
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Enter);
        let Ok(Request::Create(input)) = rx.try_recv() else {
            panic!("missing create")
        };
        assert_eq!(input.description, "two word\ns");
    }
    #[test]
    fn form_tab_order_follows_visual_layout_and_wraps_both_directions() {
        let mut a = app();
        press(&mut a, KeyCode::Char('n'));
        let mut forward = Vec::new();
        for _ in 0..7 {
            press(&mut a, KeyCode::Tab);
            if let Some(Modal::Create(f)) = &a.modal {
                forward.push(f.focus)
            }
        }
        assert_eq!(forward, [2, 3, 6, 1, 4, 5, 0]);
        let mut reverse = Vec::new();
        for _ in 0..7 {
            press(&mut a, KeyCode::BackTab);
            if let Some(Modal::Create(f)) = &a.modal {
                reverse.push(f.focus)
            }
        }
        assert_eq!(reverse, [5, 4, 1, 6, 3, 2, 0]);
    }
    #[test]
    fn dropdown_escape_closes_choice_before_form_and_delete_captures_selection() {
        let mut a = app();
        press(&mut a, KeyCode::Char('n'));
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Esc);
        assert!(matches!(a.modal, Some(Modal::Create(_))));
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.issue_type, 0);
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('d'));
        assert_eq!(
            a.modal,
            Some(Modal::ConfirmDelete {
                id: "b0".into(),
                title: "issue 0".into()
            })
        );
        a.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(a.modal.is_none());
    }
    fn press(a: &mut App, code: KeyCode) {
        a.key(KeyEvent::new(code, KeyModifiers::NONE));
        // Test actions model the app's next render before the next input event.
        if a.modal.is_some() {
            a.modal_ready = true;
        }
    }
    #[test]
    fn help_tracks_active_view_and_preserves_underlying_form() {
        let mut a = app();
        let help_key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        a.key(help_key);
        assert_eq!(a.help.unwrap().context, hotkeys::Context::Issues);
        press(&mut a, KeyCode::Down);
        assert_eq!(a.selected, Some(0));
        press(&mut a, KeyCode::Esc);
        assert!(a.help.is_none());

        press(&mut a, KeyCode::Tab);
        a.key(help_key);
        assert_eq!(a.help.unwrap().context, hotkeys::Context::Details);
        a.key(help_key);
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Char('n'));
        press(&mut a, KeyCode::Char('/'));
        press(&mut a, KeyCode::Char('?'));
        assert_eq!(a.help.unwrap().context, hotkeys::Context::Form);
        press(&mut a, KeyCode::Esc);
        assert!(matches!(a.modal, Some(Modal::Create(ref form)) if form.title == "/"));
        a.mutation_busy = true;
        a.key(help_key);
        assert!(a.help.is_none());
    }

    #[test]
    fn help_lists_view_specific_commands() {
        let mut a = app();
        a.worktrees_view = true;
        a.key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::SHIFT));
        let context = a.help.unwrap().context;
        assert_eq!(context, hotkeys::Context::Worktrees);
        assert!(context.entries().iter().any(|(key, _)| *key == "Shift+R"));
        assert!(!context.entries().iter().any(|(key, _)| *key == "n"));
    }
    #[test]
    fn escape_navigates_back_without_quitting() {
        let mut a = app();
        assert!(!a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(!a.worktrees_view);
        assert!(a.modal.is_none());

        a.worktrees_view = true;
        assert!(!a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(!a.worktrees_view);

        a.modal = Some(Modal::Settings {
            draft: a.settings,
            error: None,
            dropdown_open: true,
            focus: 0,
        });
        assert!(!a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                dropdown_open: false,
                ..
            })
        ));
        assert!(!a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(a.modal.is_none());

        a.modal = Some(Modal::ConfirmDelete {
            id: "b0".into(),
            title: "issue 0".into(),
        });
        a.mutation_busy = true;
        assert!(!a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(a.modal.is_some());
        a.mutation_busy = false;
        assert!(!a.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(a.modal.is_none());

        assert!(a.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(a.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));
    }
    #[test]
    fn create_submits_payload_once_and_locks_until_error_then_preserves_form() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        press(&mut a, KeyCode::Char('n'));
        let seq = take_harness_discovery(&rx);
        supply_harnesses(&mut a, seq, &["codex"]);
        for c in "q/rn界".chars() {
            press(&mut a, KeyCode::Char(c));
        }
        for _ in 0..5 {
            press(&mut a, KeyCode::Tab);
        }
        press(&mut a, KeyCode::Enter);
        let Ok(Request::Create(input)) = rx.try_recv() else {
            panic!("missing create")
        };
        assert_eq!(input.title, "q/rn界");
        assert_eq!(input.description, "");
        assert_eq!(input.issue_type, "task");
        assert_eq!(input.priority, 2);
        assert!(a.mutation_busy);
        press(&mut a, KeyCode::Esc);
        press(&mut a, KeyCode::Char('n'));
        press(&mut a, KeyCode::Enter);
        assert!(matches!(a.modal, Some(Modal::Create(_))));
        assert!(rx.try_recv().is_err());
        let (out, responses) = mpsc::channel();
        a.responses = responses;
        out.send(Response::Created(Err(std::io::Error::other("offline"))))
            .unwrap();
        a.receive();
        assert!(!a.mutation_busy);
        assert!(matches!(a.modal, Some(Modal::Create(_))));
        assert!(a.notice.as_deref().unwrap().contains("offline"));
    }
    #[test]
    fn empty_title_reports_error_and_delete_uses_captured_id_once() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        press(&mut a, KeyCode::Char('n'));
        take_harness_discovery(&rx);
        for _ in 0..5 {
            press(&mut a, KeyCode::Tab);
        }
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.notice.as_deref(), Some("Title is required"));
        assert!(rx.try_recv().is_err());
        a.modal = None;
        press(&mut a, KeyCode::Char('d'));
        a.selected = Some(2);
        assert_eq!(
            a.modal,
            Some(Modal::ConfirmDelete {
                id: "b0".into(),
                title: "issue 0".into()
            })
        );
        press(&mut a, KeyCode::Char('y'));
        assert!(matches!(rx.try_recv(),Ok(Request::Delete(id)) if id=="b0"));
        press(&mut a, KeyCode::Char('y'));
        assert!(rx.try_recv().is_err());
        assert!(a.mutation_busy);
    }
    #[test]
    fn close_captures_target_locks_duplicates_and_refreshes_after_retry() {
        for focus_detail in [false, true] {
            let (mut a, rx, tx) = connected_app();
            a.focus_detail = focus_detail;
            press(&mut a, KeyCode::Char('c'));
            assert!(matches!(&a.modal, Some(Modal::ConfirmClose { id, title })
                if id == "b0" && title == "issue 0"));
            a.selected = Some(2);
            press(&mut a, KeyCode::Char('y'));
            assert!(matches!(rx.try_recv(), Ok(Request::Close(id)) if id == "b0"));
            press(&mut a, KeyCode::Char('y'));
            press(&mut a, KeyCode::Esc);
            assert!(rx.try_recv().is_err());
            assert!(a.mutation_busy && a.modal.is_some());
            tx.send(Response::Closed(
                "b0".into(),
                Err(std::io::Error::other("blocked")),
            ))
            .unwrap();
            a.receive();
            assert!(!a.mutation_busy);
            assert!(a.notice.as_deref().unwrap().contains("blocked"));
            assert!(a.modal.is_some());
            assert!(matches!(rx.try_recv(), Ok(Request::List)));
            tx.send(Response::List(Ok(a.beads.clone()))).unwrap();
            a.receive();
            take_show(&rx, "b2");
            press(&mut a, KeyCode::Char('y'));
            assert!(matches!(rx.try_recv(), Ok(Request::Close(id)) if id == "b0"));
            tx.send(Response::Closed("b0".into(), Ok(()))).unwrap();
            a.receive();
            assert!(!a.mutation_busy && a.modal.is_none());
            assert!(a.notice.as_deref().unwrap().contains("b0 closed"));
            assert!(matches!(rx.try_recv(), Ok(Request::List)));
            let mut items = a.beads.clone();
            items
                .iter_mut()
                .find(|bead| bead.id == "b0")
                .unwrap()
                .status = "closed".into();
            tx.send(Response::List(Ok(items))).unwrap();
            a.receive();
            take_show(&rx, "b0");
            assert_eq!(a.focused_column, 5);
        }
    }

    #[test]
    fn close_cancel_guards_and_queue_failure_preserve_issue() {
        for cancel in [
            KeyCode::Char('n'),
            KeyCode::Char('N'),
            KeyCode::Enter,
            KeyCode::Esc,
        ] {
            let (mut a, rx, _) = connected_app();
            press(&mut a, KeyCode::Char('c'));
            press(&mut a, cancel);
            assert!(a.modal.is_none());
            assert!(rx.try_recv().is_err());
        }
        let (mut a, rx, _) = connected_app();
        a.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        a.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(rx.try_recv().is_err(), "must render before confirming");
        a.modal_ready = true;
        drop(rx);
        press(&mut a, KeyCode::Char('y'));
        assert!(!a.mutation_busy && a.modal.is_some());
        assert!(a.notice.as_deref().unwrap().contains("could not be queued"));
        a.modal = None;
        a.beads[0].status = "closed".into();
        press(&mut a, KeyCode::Char('c'));
        assert!(a.modal.is_none());
        a.selected = None;
        press(&mut a, KeyCode::Char('c'));
        assert!(a.modal.is_none());
        assert!(a.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        a.selected = Some(1);
        a.ready_pending = true;
        a.mutation_busy = true;
        press(&mut a, KeyCode::Char('c'));
        assert!(a.modal.is_none());
        a.ready_pending = false;
        a.mutation_busy = false;
        a.modal = Some(Modal::Create(CreateForm::default()));
        press(&mut a, KeyCode::Char('c'));
        assert!(matches!(&a.modal, Some(Modal::Create(form)) if form.title == "c"));
        a.modal = None;
        a.worktrees_view = true;
        press(&mut a, KeyCode::Char('c'));
        assert!(!matches!(a.modal, Some(Modal::ConfirmClose { .. })));
    }

    #[test]
    fn unreadable_delete_dialog_cannot_be_confirmed() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.modal = Some(Modal::ConfirmDelete {
            id: "b0".into(),
            title: "issue 0".into(),
        });
        a.modal_ready = false;
        press(&mut a, KeyCode::Char('y'));
        assert!(rx.try_recv().is_err());
        assert!(a.modal.is_some());
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
    }
    #[test]
    fn newly_opened_delete_is_unconfirmable_until_first_render() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        a.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(!a.modal_ready);
        a.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn unicode_cursor_and_multiline_vertical_movement_stay_on_boundaries() {
        let s = "界a\nxyz".to_string();
        let mut cursor = "界".len();
        assert_eq!(previous_boundary(&s, cursor), 0);
        cursor = next_boundary(&s, 0);
        assert_eq!(cursor, 3);
        let down = crate::ui::move_description_vertical(&s, 3, true, &mut None);
        assert_eq!(&s[down..down + 1], "z");
        let mut a = app();
        a.modal = Some(Modal::Create(CreateForm {
            title: s.clone(),
            cursor,
            focus: 0,
            ..Default::default()
        }));
        press(&mut a, KeyCode::Backspace);
        let Some(Modal::Create(form)) = a.modal else {
            panic!()
        };
        assert_eq!(form.title, "a\nxyz");
    }
    #[test]
    fn description_arrows_follow_soft_wrap_in_create_and_edit_forms() {
        let description = format!("{} {}", "a".repeat(60), "b".repeat(20));
        let first_line_cursor = 50;
        for editing in [false, true] {
            let mut a = app();
            let form = CreateForm {
                description: description.clone(),
                description_cursor: first_line_cursor,
                focus: 1,
                ..Default::default()
            };
            a.modal = Some(if editing {
                Modal::Edit {
                    id: "b0".into(),
                    form,
                }
            } else {
                Modal::Create(form)
            });
            press(&mut a, KeyCode::Up);
            let Some(Modal::Create(form) | Modal::Edit { form, .. }) = a.modal.as_ref() else {
                panic!("form closed")
            };
            assert_eq!(form.description_cursor, first_line_cursor);
            press(&mut a, KeyCode::Down);
            let Some(Modal::Create(form) | Modal::Edit { form, .. }) = a.modal.as_ref() else {
                panic!("form closed")
            };
            assert_eq!(form.description_cursor, description.len());
            assert_eq!(form.description, description);
            press(&mut a, KeyCode::Down);
            press(&mut a, KeyCode::Up);
            let Some(Modal::Create(form) | Modal::Edit { form, .. }) = a.modal.as_ref() else {
                panic!("form closed")
            };
            assert_eq!(form.description_cursor, first_line_cursor);
            assert_eq!(form.description, description);
        }
    }
    #[test]
    fn description_arrows_restore_column_across_short_explicit_line() {
        let description = format!("{}\nhi\n{}", "a".repeat(55), "界".repeat(28));
        let mut a = app();
        a.modal = Some(Modal::Create(CreateForm {
            description: description.clone(),
            description_cursor: 50,
            focus: 1,
            ..Default::default()
        }));
        press(&mut a, KeyCode::Down);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.description_cursor, 58);
        press(&mut a, KeyCode::Down);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.description_cursor, 59 + 25 * "界".len());
        assert!(form.description.is_char_boundary(form.description_cursor));
        press(&mut a, KeyCode::Up);
        press(&mut a, KeyCode::Up);
        press(&mut a, KeyCode::Up);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.description_cursor, 50);
        assert_eq!(form.description, description);
    }
    #[test]
    fn create_success_refreshes_and_selects_created_id() {
        let mut a = app();
        let (tx, rx) = mpsc::channel();
        a.requests = tx;
        let (out, responses) = mpsc::channel();
        a.responses = responses;
        out.send(Response::Created(Ok("new-id".into()))).unwrap();
        a.receive();
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
        out.send(Response::List(Ok(vec![Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: "new-id".into(),
            title: "fresh".into(),
            status: "open".into(),
            priority: 2,
            data: json!({}),
        }])))
        .unwrap();
        a.receive();
        assert_eq!(a.selected, Some(0));
        assert_eq!(a.beads[0].id, "new-id");
        match rx.try_recv() {
            Ok(Request::Show(id, _)) => assert_eq!(id, "new-id"),
            _ => panic!("missing show follow-up"),
        }
    }
    #[test]
    fn edit_loads_live_record_and_update_failure_keeps_draft_for_retry() {
        let mut a = app();
        let (requests, rx) = mpsc::channel();
        let (out, responses) = mpsc::channel();
        a.requests = requests;
        a.responses = responses;
        press(&mut a, KeyCode::Char('e'));
        let seq = match rx.try_recv() {
            Ok(Request::LoadEdit(id, seq)) if id == "b0" => seq,
            _ => panic!("missing edit load"),
        };
        assert!(matches!(a.modal, Some(Modal::EditLoading { .. })));
        out.send(Response::EditLoaded(
            "b0".into(),
            seq,
            Ok(json!({
                "id": "b0", "title": "Live title", "description": "Live\ntext",
                "issue_type": "bug", "priority": 3, "notes": "untouched"
            })),
        ))
        .unwrap();
        a.receive();
        let Some(Modal::Edit { id, form }) = a.modal.as_mut() else {
            panic!("edit form not opened")
        };
        assert_eq!(id, "b0");
        assert_eq!(
            (
                &*form.title,
                &*form.description,
                form.issue_type,
                form.priority
            ),
            ("Live title", "Live\ntext", 1, 3)
        );
        form.title = "Changed".into();
        form.focus = 4;
        press(&mut a, KeyCode::Enter);
        let Ok(Request::Update(input)) = rx.try_recv() else {
            panic!("missing update request")
        };
        assert_eq!(input.id, "b0");
        assert_eq!(input.fields.title, "Changed");
        assert_eq!(input.fields.description, "Live\ntext");
        assert_eq!(input.fields.issue_type, "bug");
        assert_eq!(input.fields.priority, 3);
        press(&mut a, KeyCode::Enter);
        assert!(rx.try_recv().is_err());
        out.send(Response::Updated(
            "b0".into(),
            Err(std::io::Error::other("offline")),
        ))
        .unwrap();
        a.receive();
        assert!(!a.mutation_busy);
        assert!(matches!(&a.modal, Some(Modal::Edit { form, .. }) if form.title == "Changed"));
        assert!(a.notice.as_deref().unwrap().contains("offline"));
        press(&mut a, KeyCode::Enter);
        assert!(matches!(rx.try_recv(), Ok(Request::Update(_))));
        out.send(Response::Updated("b0".into(), Ok(()))).unwrap();
        a.receive();
        assert!(a.modal.is_none());
        assert_eq!(a.preferred_id.as_deref(), Some("b0"));
        assert!(matches!(rx.try_recv(), Ok(Request::List)));
    }

    #[test]
    fn edit_cancellation_and_bad_live_record_do_not_write() {
        let mut a = app();
        let (requests, rx) = mpsc::channel();
        let (out, responses) = mpsc::channel();
        a.requests = requests;
        a.responses = responses;
        press(&mut a, KeyCode::Char('e'));
        let seq = match rx.try_recv() {
            Ok(Request::LoadEdit(_, seq)) => seq,
            _ => panic!(),
        };
        out.send(Response::EditLoaded(
            "b0".into(),
            seq.wrapping_sub(1),
            Ok(json!({"id": "b0", "title": "stale", "issue_type": "task", "priority": 2})),
        ))
        .unwrap();
        a.receive();
        assert!(matches!(a.modal, Some(Modal::EditLoading { .. })));
        out.send(Response::EditLoaded(
            "b0".into(),
            seq,
            Ok(json!({
                "id": "b0", "title": "Existing", "issue_type": "custom", "priority": 2
            })),
        ))
        .unwrap();
        a.receive();
        assert!(matches!(
            &a.modal,
            Some(Modal::EditLoading { error: Some(_), .. })
        ));
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Char('e'));
        let seq = match rx.try_recv() {
            Ok(Request::LoadEdit(_, seq)) => seq,
            _ => panic!(),
        };
        out.send(Response::EditLoaded(
            "b0".into(),
            seq,
            Ok(json!({
                "id": "b0", "title": "Existing", "issue_type": "task", "priority": 2
            })),
        ))
        .unwrap();
        a.receive();
        assert!(matches!(a.modal, Some(Modal::Edit { .. })));
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn dropdown_confirm_commits_previewed_option() {
        let mut a = app();
        press(&mut a, KeyCode::Char('n'));
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Down);
        press(&mut a, KeyCode::Enter);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.issue_type, 1);
        assert!(form.dropdown.is_none());
    }

    #[test]
    fn create_type_and_priority_cycle_with_left_right_and_enter_picker_remains_available() {
        let mut a = app();
        press(&mut a, KeyCode::Char('n'));
        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Left); // Type wraps from Task to the last option.
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.issue_type, beads::BEAD_TYPES.len() - 1);
        assert!(form.dropdown.is_none());
        press(&mut a, KeyCode::Right);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.issue_type, 0);
        press(&mut a, KeyCode::Enter);
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.dropdown, Some(2));
        press(&mut a, KeyCode::Left);
        assert!(
            matches!(a.modal, Some(Modal::Create(ref f)) if f.issue_type == beads::BEAD_TYPES.len() - 1)
        );
        press(&mut a, KeyCode::Esc);
        assert!(matches!(a.modal, Some(Modal::Create(ref f)) if f.issue_type == 0));

        press(&mut a, KeyCode::Tab);
        press(&mut a, KeyCode::Right); // P2 -> P3
        let Some(Modal::Create(form)) = a.modal.as_ref() else {
            panic!()
        };
        assert_eq!(form.priority, 3);
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Right);
        assert!(matches!(a.modal, Some(Modal::Create(ref f)) if f.priority == 0));
        press(&mut a, KeyCode::Left);
        assert!(matches!(a.modal, Some(Modal::Create(ref f)) if f.priority == 4));
        press(&mut a, KeyCode::Enter);
        assert!(matches!(a.modal, Some(Modal::Create(ref f)) if f.dropdown == Some(3)));
        press(&mut a, KeyCode::Right);
        assert!(matches!(a.modal, Some(Modal::Create(ref f)) if f.priority == 0));
        press(&mut a, KeyCode::Enter);
        assert!(
            matches!(a.modal, Some(Modal::Create(ref f)) if f.priority == 0 && f.dropdown.is_none())
        );
    }

    #[test]
    fn choice_cycling_wraps_both_directions() {
        assert_eq!(cycle_choice(0, 3, false), 2);
        assert_eq!(cycle_choice(2, 3, true), 0);
    }

    #[test]
    fn settings_autosave_failure_preserves_commit_and_same_choice_can_retry() {
        use std::fs;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ontrack-app-settings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let mut a = app();
        a.settings_path = Some(path.clone());
        settings::save_to(
            &path,
            &Settings {
                theme: Theme::Dark,
                ..Settings::default()
            },
        )
        .unwrap();
        press(&mut a, KeyCode::Char('s'));
        // A regular file where the parent directory should be makes saving fail.
        let blocker = root.join("blocked");
        fs::write(&blocker, "preserve").unwrap();
        a.settings_path = Some(blocker.join("settings.json"));
        press(&mut a, KeyCode::Right);
        assert_eq!(
            a.settings.theme,
            Theme::Dark,
            "failed save keeps committed value"
        );
        assert_eq!(
            a.theme,
            Theme::Dark,
            "failed save leaves the active theme unchanged"
        );
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                draft: Settings {
                    theme: Theme::Light,
                    ..
                },
                error: Some(_),
                ..
            })
        ));
        assert_eq!(settings::load_from(&path).unwrap().theme, Theme::Dark);
        assert_eq!(fs::read_to_string(blocker).unwrap(), "preserve");

        press(&mut a, KeyCode::Enter);
        assert!(matches!(
            a.modal,
            Some(Modal::Settings {
                dropdown_open: true,
                error: Some(_),
                ..
            })
        ));
        assert_eq!(a.theme, Theme::Dark);
        assert_eq!(settings::load_from(&path).unwrap().theme, Theme::Dark);
        press(&mut a, KeyCode::Enter);
        assert!(
            matches!(a.modal, Some(Modal::Settings { error: Some(_), .. })),
            "retry on the same candidate still reports failure"
        );

        a.settings_path = Some(path.clone());
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.theme, Theme::Light);
        assert_eq!(a.settings.theme, Theme::Light);
        assert!(matches!(a.modal, Some(Modal::Settings { error: None, .. })));
        assert_eq!(settings::load_from(&path).unwrap().theme, Theme::Light);
        press(&mut a, KeyCode::Esc);
        assert!(a.modal.is_none());
        assert_eq!(
            a.theme,
            Theme::Light,
            "closing retains the successful choice"
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn enter_focuses_selected_worktree_without_repeating_pending_request() {
        let (mut a, rx, tx) = connected_app();
        a.worktrees_view = true;
        a.worktrees = ["w1", "w2"]
            .into_iter()
            .map(|id| ManagedWorktree {
                id: id.into(),
                bead_id: "b1".into(),
                parent_bead_id: None,
                handoff_accepted: true,
                bead_title: None,
                bead_description: None,
                branch: "ontrack/b1".into(),
                target_branch: "main".into(),
                workspace_id: id.into(),
                status: worktrees::WorktreeStatus::Unmerged,
                detail: None,
            })
            .collect();
        a.selected_worktree = 1;
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Enter);
        assert!(matches!(rx.try_recv(), Ok(Request::FocusWorkspace(id)) if id == "w2"));
        assert!(rx.try_recv().is_err());
        tx.send(Response::WorkspaceFocused("w2".into(), Ok(())))
            .unwrap();
        a.receive_at(Instant::now());
        assert!(matches!(
            a.worktrees_focus_feedback,
            Some(WorktreeFocusFeedback::Success(ref message)) if message.contains("w2")
        ));
        assert!(!a.worktrees_focus_pending);
    }
    #[test]
    fn enter_without_open_workspace_reports_error_without_request() {
        let (mut a, rx, tx) = connected_app();
        a.worktrees_view = true;
        press(&mut a, KeyCode::Enter);
        assert!(matches!(
            a.worktrees_focus_feedback,
            Some(WorktreeFocusFeedback::Error(ref message)) if message.contains("Select")
        ));
        a.worktrees.push(ManagedWorktree {
            id: "wt-1".into(),
            bead_id: "b1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/b1".into(),
            target_branch: "main".into(),
            workspace_id: "w1".into(),
            status: worktrees::WorktreeStatus::WorkspaceClosed,
            detail: None,
        });
        press(&mut a, KeyCode::Enter);
        assert!(matches!(
            a.worktrees_focus_feedback,
            Some(WorktreeFocusFeedback::Error(ref message)) if message.contains("closed")
        ));
        assert!(rx.try_recv().is_err());

        a.worktrees[0].status = worktrees::WorktreeStatus::CheckoutRemoved;
        press(&mut a, KeyCode::Enter);
        assert!(matches!(
            a.worktrees_focus_feedback,
            Some(WorktreeFocusFeedback::Error(ref message)) if message.contains("closed")
        ));
        assert!(rx.try_recv().is_err());

        a.worktrees[0].status = worktrees::WorktreeStatus::Integrated;
        press(&mut a, KeyCode::Enter);
        assert!(matches!(rx.try_recv(), Ok(Request::FocusWorkspace(id)) if id == "w1"));
        tx.send(Response::WorkspaceFocused(
            "w1".into(),
            Err(std::io::Error::other("workspace missing")),
        ))
        .unwrap();
        a.receive_at(Instant::now());
        assert!(matches!(
            a.worktrees_focus_feedback,
            Some(WorktreeFocusFeedback::Error(ref message)) if message.contains("workspace missing")
        ));
    }
    #[test]
    fn worktree_merge_requires_confirmation_and_cancel_has_no_side_effect() {
        let (mut a, rx, _) = connected_app();
        a.worktrees_view = true;
        a.worktrees.push(ManagedWorktree {
            id: "wt-1".into(),
            bead_id: "b1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/b1".into(),
            target_branch: "main".into(),
            workspace_id: "ws1".into(),
            status: worktrees::WorktreeStatus::InProgress,
            detail: None,
        });
        press(&mut a, KeyCode::Char('m'));
        assert!(matches!(
            a.modal,
            Some(Modal::ConfirmWorktree { action: 0, .. })
        ));
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Esc);
        assert!(rx.try_recv().is_err());
        press(&mut a, KeyCode::Char('m'));
        press(&mut a, KeyCode::Char('y'));
        assert!(
            matches!(rx.try_recv(), Ok(Request::WorktreeAction(id, WorktreeAction::RequestMerge)) if id == "wt-1")
        );
    }
    #[test]
    fn restart_requires_explicit_confirmation_and_queues_selected_handoff() {
        let (mut app, requests, _) = connected_app();
        app.worktrees_view = true;
        app.worktrees.push(ManagedWorktree {
            id: "wt-1".into(),
            bead_id: "b1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: Some("Task".into()),
            bead_description: None,
            branch: "ontrack/b1".into(),
            target_branch: "main".into(),
            workspace_id: "ws1".into(),
            status: worktrees::WorktreeStatus::NeedsAttention,
            detail: None,
        });
        press(&mut app, KeyCode::Char('R'));
        assert!(matches!(
            app.modal,
            Some(Modal::ConfirmWorktree { action: 3, .. })
        ));
        assert!(requests.try_recv().is_err());
        press(&mut app, KeyCode::Esc);
        assert!(requests.try_recv().is_err());
        press(&mut app, KeyCode::Char('R'));
        press(&mut app, KeyCode::Char('y'));
        assert!(matches!(requests.try_recv(), Ok(Request::RestartWorktree(id)) if id == "wt-1"));
        assert!(app.mutation_busy);
    }
    #[test]
    fn force_remove_requires_explicit_confirmation_and_captures_selected_handoff() {
        let (mut app, requests, _) = connected_app();
        app.worktrees_view = true;
        app.worktrees.push(ManagedWorktree {
            id: "wt-1".into(),
            bead_id: "b1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: Some("Task".into()),
            bead_description: None,
            branch: "ontrack/b1".into(),
            target_branch: "main".into(),
            workspace_id: "ws1".into(),
            status: worktrees::WorktreeStatus::NeedsAttention,
            detail: None,
        });
        press(&mut app, KeyCode::Char('X'));
        assert!(matches!(
            app.modal,
            Some(Modal::ConfirmWorktree { action: 5, .. })
        ));
        assert!(requests.try_recv().is_err());
        press(&mut app, KeyCode::Esc);
        assert!(requests.try_recv().is_err());
        press(&mut app, KeyCode::Char('X'));
        app.worktrees[0].id = "changed-selection".into();
        press(&mut app, KeyCode::Char('y'));
        assert!(
            matches!(requests.try_recv(), Ok(Request::WorktreeAction(id, WorktreeAction::ForceRemoveCheckout)) if id == "wt-1")
        );
        assert!(app.mutation_busy);
    }
    #[test]
    fn continue_requires_confirmation_and_queues_active_handoff() {
        let (mut app, requests, _) = connected_app();
        app.worktrees_view = true;
        app.worktrees.push(ManagedWorktree {
            id: "wt-1".into(),
            bead_id: "b1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: Some("Task".into()),
            bead_description: None,
            branch: "ontrack/b1".into(),
            target_branch: "main".into(),
            workspace_id: "ws1".into(),
            status: worktrees::WorktreeStatus::DirtyCheckout,
            detail: None,
        });
        press(&mut app, KeyCode::Char('a'));
        assert!(matches!(
            app.modal,
            Some(Modal::ConfirmWorktree { action: 4, .. })
        ));
        assert!(requests.try_recv().is_err());
        press(&mut app, KeyCode::Char('y'));
        assert!(matches!(requests.try_recv(), Ok(Request::ContinueWorktree(id)) if id == "wt-1"));
    }

    #[test]
    fn empty_worktree_view_stays_rendered_through_refresh_and_error_retry() {
        use ratatui::{Terminal, backend::TestBackend};

        for (width, height) in [(120, 24), (64, 16)] {
            let (mut app, requests, responses) = connected_app();
            app.worktrees_view = true;
            app.worktrees_discovered = false;
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut frame = |app: &mut App| {
                terminal
                    .draw(|f| crate::ui::render(f, Duration::from_secs(2), app))
                    .unwrap();
                terminal.backend().buffer().clone()
            };
            let now = Instant::now();
            app.poll_worktrees(now);
            assert!(matches!(requests.try_recv(), Ok(Request::ListWorktrees)));
            assert!(app.worktrees_loading, "initial discovery stays quiet");
            let initial = frame(&mut app);
            responses.send(Response::Worktrees(Ok(vec![]))).unwrap();
            app.receive_at(now);
            let settled = frame(&mut app);
            assert_ne!(initial, settled, "completed discovery shows guidance");

            for cycle in 1..=3 {
                let now = now + REFRESH_INTERVAL * cycle;
                app.poll_worktrees(now);
                assert!(matches!(requests.try_recv(), Ok(Request::ListWorktrees)));
                assert_eq!(frame(&mut app), settled, "background scan hides no text");
                app.poll_worktrees(now + REFRESH_INTERVAL * 10);
                assert!(requests.try_recv().is_err(), "requests remain bounded");
                responses.send(Response::Worktrees(Ok(vec![]))).unwrap();
                app.receive_at(now);
                assert_eq!(frame(&mut app), settled);
            }

            press(&mut app, KeyCode::Char('r'));
            assert!(matches!(requests.try_recv(), Ok(Request::ListWorktrees)));
            assert_eq!(frame(&mut app), settled, "manual refresh hides no text");
            responses
                .send(Response::Worktrees(Err(std::io::Error::other(
                    "registry unavailable",
                ))))
                .unwrap();
            app.receive_at(now);
            assert_eq!(app.worktrees_error.as_deref(), Some("registry unavailable"));
            let failed = frame(&mut app);
            assert_ne!(failed, settled, "scan errors render in the header");
            app.poll_worktrees(now + REFRESH_INTERVAL);
            assert!(matches!(requests.try_recv(), Ok(Request::ListWorktrees)));
            assert_eq!(frame(&mut app), failed, "retry preserves empty guidance");
            responses.send(Response::Worktrees(Ok(vec![]))).unwrap();
            app.receive_at(now);
            assert_eq!(frame(&mut app), settled, "success clears the error");

            app.poll_worktrees(now + REFRESH_INTERVAL);
            assert!(matches!(requests.try_recv(), Ok(Request::ListWorktrees)));
            responses
                .send(Response::Worktrees(Ok(vec![observed_row(
                    "new-handoff",
                    worktrees::WorktreeStatus::InProgress,
                )])))
                .unwrap();
            app.receive_at(now);
            assert_eq!(app.worktrees.len(), 1);
            assert_ne!(frame(&mut app), settled, "new work replaces empty guidance");
        }
    }
    #[test]
    fn worktree_worker_polls_without_queue_growth_and_surfaces_errors() {
        let (mut a, rx, tx) = connected_app();
        a.worktrees_view = true;
        let now = Instant::now();
        a.poll_worktrees(now);
        assert!(matches!(rx.try_recv(), Ok(Request::ListWorktrees)));
        a.poll_worktrees(now + REFRESH_INTERVAL * 10);
        assert!(rx.try_recv().is_err(), "one in-flight request is enough");
        tx.send(Response::Worktrees(Err(std::io::Error::other(
            "registry unavailable",
        ))))
        .unwrap();
        a.receive_at(now);
        assert_eq!(a.worktrees_error.as_deref(), Some("registry unavailable"));
        assert!(!a.worktrees_loading);
        a.poll_worktrees(now + REFRESH_INTERVAL);
        assert!(matches!(rx.try_recv(), Ok(Request::ListWorktrees)));
    }

    #[test]
    fn worktree_view_r_still_refreshes_worktrees() {
        let (mut a, rx, _) = connected_app();
        a.worktrees_view = true;
        press(&mut a, KeyCode::Char('r'));
        assert!(matches!(rx.try_recv(), Ok(Request::ListWorktrees)));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn worktree_detail_scroll_is_separate_and_resets_on_selection() {
        let (mut app, _, _) = connected_app();
        app.worktrees_view = true;
        app.detail_rect = Rect::new(40, 2, 40, 12);
        app.worktrees = (0..2)
            .map(|index| ManagedWorktree {
                id: format!("wt-{index}"),
                bead_id: format!("bead-{index}"),
                parent_bead_id: None,
                handoff_accepted: true,
                bead_title: Some(format!("Title {index}")),
                bead_description: Some("A long description".into()),
                branch: format!("ontrack/bead-{index}"),
                target_branch: "main".into(),
                workspace_id: format!("ws-{index}"),
                status: worktrees::WorktreeStatus::Unmerged,
                detail: None,
            })
            .collect();
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.worktree_detail_offset, 10);
        assert_eq!(app.detail_offset, 0);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected_worktree, 1);
        assert_eq!(app.worktree_detail_offset, 0);
    }
    #[test]
    fn worktree_policy_reconciles_outside_worktrees_view() {
        let (mut a, rx, _) = connected_app();
        a.settings.auto_merge = true;
        a.poll_worktrees(Instant::now());
        assert!(matches!(
            rx.try_recv(),
            Ok(Request::ReconcileWorktrees(CleanupPolicy {
                auto_merge: true,
                ..
            }))
        ));
        assert!(!a.worktrees_view);
    }
    #[test]
    fn child_automation_discovers_worktrees_after_an_empty_scan() {
        let (mut app, requests, responses) = connected_app();
        assert!(!app.settings.auto_merge);
        assert!(app.settings.auto_merge_children);
        assert!(!app.settings.auto_close_workspace);
        assert!(!app.settings.auto_remove_checkout);
        assert!(!app.worktrees_view);
        app.worktrees_discovered = false;
        let now = Instant::now();
        app.poll_worktrees(now);
        assert!(matches!(requests.try_recv(), Ok(Request::ListWorktrees)));
        responses.send(Response::Worktrees(Ok(vec![]))).unwrap();
        app.receive_at(now);

        app.poll_worktrees(now + REFRESH_INTERVAL);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::ReconcileWorktrees(CleanupPolicy {
                auto_merge: false,
                auto_merge_children: true,
                auto_close_workspace: false,
                auto_remove_checkout: false,
            }))
        ));
        let mut child = observed_row("child", worktrees::WorktreeStatus::Unmerged);
        child.parent_bead_id = Some("epic".into());
        responses
            .send(Response::Worktrees(Ok(vec![child])))
            .unwrap();
        app.receive_at(now + REFRESH_INTERVAL);
        assert_eq!(app.worktrees[0].parent_bead_id.as_deref(), Some("epic"));
        app.poll_worktrees(now + REFRESH_INTERVAL * 2);
        assert!(matches!(
            requests.try_recv(),
            Ok(Request::ReconcileWorktrees(_))
        ));
    }
    #[test]
    fn empty_settled_worktrees_do_not_poll_when_all_automation_is_disabled() {
        let (mut app, requests, _) = connected_app();
        app.settings.auto_merge_children = false;
        app.poll_worktrees(Instant::now());
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn tracked_worktree_reconciles_with_cleanup_preferences_disabled() {
        let (mut app, rx, _) = connected_app();
        app.worktrees_discovered = true;
        app.worktrees.push(ManagedWorktree {
            id: "tracked".into(),
            bead_id: "bead".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/task".into(),
            target_branch: "main".into(),
            workspace_id: "ws".into(),
            status: worktrees::WorktreeStatus::Unmerged,
            detail: None,
        });
        app.poll_worktrees(Instant::now());
        assert!(matches!(
            rx.try_recv(),
            Ok(Request::ReconcileWorktrees(CleanupPolicy {
                auto_merge: false,
                auto_merge_children: true,
                auto_close_workspace: false,
                auto_remove_checkout: false,
            }))
        ));
    }
    #[test]
    fn closed_bead_does_not_remove_its_managed_worktree() {
        let (mut a, _, tx) = connected_app();
        a.beads[0].status = "closed".into();
        let item = ManagedWorktree {
            id: "tracked-closed".into(),
            bead_id: a.beads[0].id.clone(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/b0".into(),
            target_branch: "main".into(),
            workspace_id: "ws".into(),
            status: worktrees::WorktreeStatus::Unmerged,
            detail: None,
        };
        tx.send(Response::Worktrees(Ok(vec![item.clone()])))
            .unwrap();
        a.receive_at(Instant::now());
        assert_eq!(a.worktrees.len(), 1);
        assert_eq!(a.worktrees[0].id, item.id);
        assert_eq!(a.beads[0].status, "closed");
    }
    #[test]
    fn removed_checkout_is_hidden_on_refresh_without_hiding_attention_or_closed_workspace() {
        let (mut app, _, tx) = connected_app();
        let item = |id: &str, status| ManagedWorktree {
            id: id.into(),
            bead_id: format!("bead-{id}"),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: format!("ontrack/{id}"),
            target_branch: "main".into(),
            workspace_id: format!("ws-{id}"),
            status,
            detail: None,
        };
        let removed = item("removed", worktrees::WorktreeStatus::CheckoutRemoved);
        let closed = item("closed", worktrees::WorktreeStatus::WorkspaceClosed);
        let attention = item("attention", worktrees::WorktreeStatus::NeedsAttention);
        app.worktrees = vec![removed.clone(), closed.clone()];
        app.selected_worktree = 1;
        app.worktree_detail_offset = 7;
        tx.send(Response::Worktrees(Ok(vec![
            removed.clone(),
            closed.clone(),
            attention.clone(),
        ])))
        .unwrap();
        app.receive();
        assert_eq!(
            app.worktrees
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["closed", "attention"]
        );
        assert_eq!(app.selected_worktree, 0);
        assert_eq!(app.worktree_detail_offset, 7);
        assert!(app.accepted_handoffs.contains(&removed.bead_id));

        tx.send(Response::Worktrees(Ok(vec![
            removed.clone(),
            attention.clone(),
        ])))
        .unwrap();
        app.receive();
        assert_eq!(app.worktrees[0].id, attention.id);
        assert_eq!(app.selected_worktree, 0);
        assert_eq!(app.worktree_detail_offset, 0);

        tx.send(Response::Worktrees(Ok(vec![removed]))).unwrap();
        app.receive();
        assert!(app.worktrees.is_empty());
        assert_eq!(app.selected_worktree, 0);
    }
    #[test]
    fn successful_checkout_removal_hides_selected_row_immediately() {
        let (mut app, _, tx) = connected_app();
        let item = |id: &str| ManagedWorktree {
            id: id.into(),
            bead_id: format!("bead-{id}"),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: format!("ontrack/{id}"),
            target_branch: "main".into(),
            workspace_id: format!("ws-{id}"),
            status: worktrees::WorktreeStatus::Integrated,
            detail: None,
        };
        let first = item("first");
        let selected = item("selected");
        let last = item("last");
        app.worktrees = vec![first, selected.clone(), last.clone()];
        app.selected_worktree = 1;
        app.worktree_detail_offset = 9;
        tx.send(Response::WorktreeChanged(Ok(ManagedWorktree {
            status: worktrees::WorktreeStatus::CheckoutRemoved,
            ..selected
        })))
        .unwrap();
        app.receive();
        assert_eq!(app.worktrees.len(), 2);
        assert_eq!(app.worktrees[app.selected_worktree].id, last.id);
        assert_eq!(app.worktree_detail_offset, 0);
        assert!(app.accepted_handoffs.contains("bead-selected"));
        assert!(app.notice.as_deref().unwrap().contains("CHECKOUT REMOVED"));
    }
    #[test]
    fn older_worktree_snapshot_cannot_overwrite_action_result() {
        let (mut a, _, tx) = connected_app();
        let before = ManagedWorktree {
            id: "wt".into(),
            bead_id: "b0".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/b0".into(),
            target_branch: "main".into(),
            workspace_id: "ws".into(),
            status: worktrees::WorktreeStatus::Unmerged,
            detail: None,
        };
        a.worktrees = vec![before.clone()];
        a.worktrees_inflight = true;
        a.worktree_action_pending = true;
        let after = ManagedWorktree {
            status: worktrees::WorktreeStatus::Integrated,
            detail: None,
            ..before
        };
        tx.send(Response::Worktrees(Ok(vec![a.worktrees[0].clone()])))
            .unwrap();
        tx.send(Response::WorktreeChanged(Ok(after.clone())))
            .unwrap();
        a.receive_at(Instant::now());
        assert_eq!(a.worktrees.len(), 1);
        assert_eq!(a.worktrees[0].status, worktrees::WorktreeStatus::Integrated);
    }
}
