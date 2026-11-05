//! Captured-output Herdr adapter; creation is explicit and recovery is non-destructive.

use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use crate::beads::Bead;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchContext {
    pub workspace: Workspace,
    pub agents: Vec<Agent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOutcome {
    pub message: String,
    pub reserved: bool,
}

#[derive(Debug)]
pub(crate) struct LaunchFailure {
    pub retry_safe: bool,
    message: String,
}
impl std::fmt::Display for LaunchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for LaunchFailure {}

pub(crate) fn launch_failure(error: &io::Error) -> Option<&LaunchFailure> {
    error.get_ref()?.downcast_ref()
}

fn preflight_failure(error: io::Error) -> io::Error {
    if launch_failure(&error).is_some() {
        error
    } else {
        io::Error::new(
            error.kind(),
            LaunchFailure {
                retry_safe: true,
                message: error.to_string(),
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Discovery {
    pub workspace_id: String,
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Launch {
    pub workspace_id: String,
    pub pane_id: String,
    pub branch: String,
    pub reserved: bool,
}

struct LaunchPlan {
    reserve_for_children: bool,
    expected_repository: Option<RepositoryIdentity>,
}

// Keep external commands and launch journaling injectable in Herdr unit tests.
pub(crate) trait LaunchRecord {
    fn capture(&mut self, kind: &str, bytes: &[u8]) -> io::Result<()>;
    fn created(&mut self, result: &Value) -> io::Result<()>;
    fn phase(&mut self, phase: &str) -> io::Result<()>;
    fn failed(&mut self, message: &str) -> io::Result<()>;
    fn base_branch(&self) -> Option<&str> {
        None
    }
    fn reserved_target(&self) -> Option<(String, String, String, PathBuf)> {
        None
    }
}

impl LaunchRecord for crate::worktrees::LaunchJournal {
    fn capture(&mut self, kind: &str, bytes: &[u8]) -> io::Result<()> {
        Self::capture(self, kind, bytes)
    }

    fn created(&mut self, result: &Value) -> io::Result<()> {
        Self::created(self, result)
    }

    fn phase(&mut self, phase: &str) -> io::Result<()> {
        Self::phase(self, phase)
    }

    fn failed(&mut self, message: &str) -> io::Result<()> {
        Self::failed(self, message)
    }

    fn base_branch(&self) -> Option<&str> {
        Self::base_branch(self)
    }

    fn reserved_target(&self) -> Option<(String, String, String, PathBuf)> {
        Self::reserved_target(self)
    }
}

pub(crate) trait Executor {
    fn run(&mut self, args: &[&str]) -> io::Result<Vec<u8>>;
    fn show_bead(&mut self, id: &str) -> io::Result<Value>;
    fn close_bead(&mut self, _id: &str, _expected: &Value) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Bead closure is not implemented by this executor",
        ))
    }
    fn dependency_ready(&mut self, id: &str) -> io::Result<bool>;
    fn open_descendants(&mut self, _id: &str) -> io::Result<Vec<String>> {
        Ok(Vec::new())
    }
    fn pending_child_handoffs(&mut self, _id: &str) -> io::Result<Vec<String>> {
        Ok(Vec::new())
    }
    fn reserved_name(&mut self, _id: &str) -> io::Result<Option<String>> {
        Ok(None)
    }
    fn git_identity(&mut self, checkout: &Path) -> io::Result<RepositoryIdentity>;
    fn launch_intent(
        &mut self,
        _checkout: &Path,
        _bead: &Value,
        _branch: &str,
        _agent: &str,
        _name: &str,
        _workspace: &str,
    ) -> io::Result<Option<Box<dyn LaunchRecord>>> {
        Ok(None)
    }
}

pub(crate) struct SystemExecutor;

impl Executor for SystemExecutor {
    fn launch_intent(
        &mut self,
        checkout: &Path,
        bead: &Value,
        branch: &str,
        agent: &str,
        name: &str,
        workspace: &str,
    ) -> io::Result<Option<Box<dyn LaunchRecord>>> {
        crate::worktrees::LaunchJournal::begin(checkout, bead, branch, agent, name, workspace)
            .map(|journal| Some(Box::new(journal) as Box<dyn LaunchRecord>))
    }

    fn run(&mut self, args: &[&str]) -> io::Result<Vec<u8>> {
        let output = Command::new("herdr").args(args).output().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("Could not run Herdr: {error}. Check installation and PATH."),
            )
        })?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "Herdr exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }

    fn show_bead(&mut self, id: &str) -> io::Result<Value> {
        crate::beads::show_bead(id)
    }

    fn close_bead(&mut self, id: &str, expected: &Value) -> io::Result<()> {
        crate::beads::close_verified_bead(id, expected)
    }

    fn dependency_ready(&mut self, id: &str) -> io::Result<bool> {
        crate::beads::dependency_ready(id)
    }

    fn open_descendants(&mut self, id: &str) -> io::Result<Vec<String>> {
        crate::beads::open_descendants(id)
    }

    fn pending_child_handoffs(&mut self, id: &str) -> io::Result<Vec<String>> {
        crate::worktrees::pending_child_handoffs(id)
    }

    fn reserved_name(&mut self, id: &str) -> io::Result<Option<String>> {
        crate::worktrees::reserved_name(id)
    }

    fn git_identity(&mut self, checkout: &Path) -> io::Result<RepositoryIdentity> {
        repository_identity_with(checkout, |directory, args| {
            let output = Command::new("git")
                .arg("-C")
                .arg(directory)
                .args(args)
                .output()?;
            if !output.status.success() {
                return Err(io::Error::other(format!(
                    "Git metadata lookup failed ({}): {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            Ok(output.stdout)
        })
    }
}

struct Source {
    discovery: Discovery,
    checkout: PathBuf,
    branches: HashSet<String>,
    workspace_ids: HashSet<String>,
}

pub fn discover() -> io::Result<LaunchContext> {
    Ok(discover_with(&mut SystemExecutor, &current_dir()?)?
        .discovery
        .into())
}

/// Current Herdr integrations that MXO can store as issue harness labels.
/// This does not require a workspace mapping, so the create form works from a
/// linked checkout or outside a Herdr shell.
pub fn installed_harnesses() -> io::Result<Vec<String>> {
    installed_harnesses_with(&mut SystemExecutor)
}

fn installed_harnesses_with(executor: &mut impl Executor) -> io::Result<Vec<String>> {
    let help = executor
        .run(&["agent", "start", "--help"])
        .map_err(|error| stage("supported agent discovery", error))?;
    let supported =
        supported_agents(&help).map_err(|error| stage("supported agent discovery", error))?;
    let status = executor
        .run(&["integration", "status"])
        .map_err(|error| stage("integration discovery", error))?;
    let available = match available_agents(&status, &supported) {
        Ok(agents) => agents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(stage("integration discovery", error)),
    };
    Ok(crate::beads::HARNESSES
        .iter()
        .filter(|kind| available.iter().any(|agent| agent == **kind))
        .map(|kind| (*kind).to_owned())
        .collect())
}

pub fn launch(context: &LaunchContext, agent: &Agent, bead: &Bead) -> io::Result<LaunchOutcome> {
    launch_mode(context, agent, bead, false)
}

pub fn launch_ready(
    context: &LaunchContext,
    agent: &Agent,
    bead: &Bead,
) -> io::Result<LaunchOutcome> {
    launch_mode(context, agent, bead, true)
}

fn launch_mode(
    context: &LaunchContext,
    agent: &Agent,
    bead: &Bead,
    require_ready: bool,
) -> io::Result<LaunchOutcome> {
    let name = crate::worktrees::reserved_name(&bead.id)?.unwrap_or_else(|| unique_name(&bead.id));
    handoff_with_mode(
        &mut SystemExecutor,
        &current_dir()?,
        context,
        agent,
        bead,
        &name,
        require_ready,
    )
    .map_err(preflight_failure)
}

pub(crate) fn validate_restart_source(
    executor: &mut impl Executor,
    checkout: &Path,
    workspace_id: &str,
    agent_id: &str,
) -> io::Result<()> {
    let source = discover_with(executor, checkout)?;
    if source.discovery.workspace_id != workspace_id
        || !source
            .discovery
            .agents
            .iter()
            .any(|agent| agent == agent_id)
    {
        return Err(invalid(
            "Recorded source workspace or agent integration is unavailable; restart cancelled",
        ));
    }
    Ok(())
}

pub(crate) fn relaunch_from(
    executor: &mut impl Executor,
    checkout: &Path,
    workspace_id: &str,
    agent_id: &str,
    bead_id: &str,
    title: &str,
) -> io::Result<LaunchOutcome> {
    let result = launch_with(
        executor,
        checkout,
        &Request {
            workspace_id,
            agent: agent_id,
            id: bead_id,
            title,
            name: &unique_name(bead_id),
        },
    )?;
    Ok(LaunchOutcome {
        reserved: result.reserved,
        message: format!(
            "Task restarted · new workspace {} · pane {} · branch {}",
            result.workspace_id, result.pane_id, result.branch
        ),
    })
}

pub fn focus_workspace(workspace_id: &str) -> io::Result<()> {
    focus_workspace_with(&mut SystemExecutor, workspace_id)
}

fn focus_workspace_with(executor: &mut impl Executor, workspace_id: &str) -> io::Result<()> {
    if workspace_id.is_empty()
        || workspace_id.starts_with('-')
        || !workspace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_-".contains(&byte))
    {
        return Err(invalid("Invalid recorded Herdr workspace ID"));
    }
    let result = envelope(&executor.run(&["workspace", "focus", workspace_id])?)?;
    if result["type"] != "workspace_focused" || result["workspace_id"] != workspace_id {
        return Err(invalid(
            "Herdr did not confirm focus of the selected workspace",
        ));
    }
    Ok(())
}

impl From<Discovery> for LaunchContext {
    fn from(discovery: Discovery) -> Self {
        Self {
            workspace: Workspace {
                label: discovery.workspace_id.clone(),
                id: discovery.workspace_id,
            },
            agents: discovery
                .agents
                .into_iter()
                .map(|id| Agent {
                    label: match id.as_str() {
                        "codex" => "Codex".to_owned(),
                        "opencode" => "OpenCode".to_owned(),
                        _ => id.clone(),
                    },
                    id,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
fn handoff_with(
    executor: &mut impl Executor,
    cwd: &Path,
    context: &LaunchContext,
    agent: &Agent,
    bead: &Bead,
    name: &str,
) -> io::Result<LaunchOutcome> {
    handoff_with_mode(executor, cwd, context, agent, bead, name, false)
}

fn handoff_with_mode(
    executor: &mut impl Executor,
    cwd: &Path,
    context: &LaunchContext,
    agent: &Agent,
    bead: &Bead,
    name: &str,
    require_ready: bool,
) -> io::Result<LaunchOutcome> {
    if !context
        .agents
        .iter()
        .any(|available| available.id == agent.id)
    {
        return Err(invalid(
            "Launch validation: select an agent from discovery; nothing created",
        ));
    }
    let outcome = launch_with_mode(
        executor,
        cwd,
        &Request {
            workspace_id: &context.workspace.id,
            agent: &agent.id,
            id: &bead.id,
            title: &bead.title,
            name,
        },
        require_ready,
    )?;
    Ok(LaunchOutcome {
        reserved: outcome.reserved,
        message: if outcome.reserved {
            format!(
                "Parent worktree prepared in workspace {} on branch {}. Complete and integrate children, then press h on the parent to start its agent.",
                outcome.workspace_id, outcome.branch
            )
        } else {
            format!(
                "Prompt sent to {} · workspace {} · pane {} · branch {}.",
                agent.label, outcome.workspace_id, outcome.pane_id, outcome.branch
            )
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositoryIdentity {
    pub(crate) common_dir: PathBuf,
    pub(crate) main_root: PathBuf,
}

fn repository_identity_with(
    checkout: &Path,
    mut git: impl FnMut(&Path, &[&str]) -> io::Result<Vec<u8>>,
) -> io::Result<RepositoryIdentity> {
    let checkout = checkout.canonicalize()?;
    let common_args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    let common_dir = canonical_git_path(&git(&checkout, &common_args)?)?;
    // Codex uses the parent of a normal common .git directory for linked
    // worktrees. Do not turn arbitrary metadata-directory parents into trust.
    if common_dir.file_name() != Some(std::ffi::OsStr::new(".git")) || !common_dir.is_dir() {
        return Err(invalid(
            "Codex trust requires a normal canonical .git common directory",
        ));
    }
    let main_root = common_dir
        .parent()
        .ok_or_else(|| invalid("Git common directory has no main root"))?
        .to_path_buf();
    if !std::fs::symlink_metadata(main_root.join(".git"))?
        .file_type()
        .is_dir()
    {
        return Err(invalid(
            "Main repository .git must be a real directory, not redirected metadata",
        ));
    }
    for directory in [&checkout, &main_root] {
        let bare = git(directory, &["rev-parse", "--is-bare-repository"])?;
        if bare != b"false\n" && bare != b"false" {
            return Err(invalid("Codex trust requires a non-bare repository"));
        }
        let root = canonical_git_path(&git(directory, &["rev-parse", "--show-toplevel"])?)?;
        if root != *directory {
            return Err(invalid(
                "Git metadata does not identify the exact checkout/main root",
            ));
        }
    }
    let main_common = canonical_git_path(&git(&main_root, &common_args)?)?;
    let main_git = canonical_git_path(&git(&main_root, &["rev-parse", "--absolute-git-dir"])?)?;
    if main_common != common_dir || main_git != common_dir {
        return Err(invalid(
            "Main-root Git metadata does not match the repository common directory",
        ));
    }
    Ok(RepositoryIdentity {
        common_dir,
        main_root,
    })
}

fn canonical_git_path(bytes: &[u8]) -> io::Result<PathBuf> {
    let path = std::str::from_utf8(bytes).map_err(|_| invalid("Git trust paths must be UTF-8"))?;
    // Remove the record terminator, not filename whitespace/control characters.
    let path = Path::new(path.strip_suffix('\n').unwrap_or(path));
    if !path.is_absolute() {
        return Err(invalid("Git trust metadata must return an absolute path"));
    }
    path.canonicalize()
}

/// Invocation-only trust for the verified main repository, including when
/// launching from a linked checkout. Shares the handoff's resolver and encoding.
pub(crate) fn codex_startup_trust(checkout: &Path) -> io::Result<String> {
    codex_trust_override(&SystemExecutor.git_identity(checkout)?.main_root)
}

fn codex_trust_override(root: &Path) -> io::Result<String> {
    if !root.is_absolute() {
        return Err(invalid("Codex trust root must be absolute"));
    }
    let root = root
        .to_str()
        .ok_or_else(|| invalid("Codex trust root must be UTF-8"))?;
    // JSON's quoted-string escapes are TOML-compatible. Escape remaining
    // Unicode controls too (especially DEL, which TOML forbids literally).
    let encoded = serde_json::to_string(root).map_err(|error| invalid(error.to_string()))?;
    let mut key = String::new();
    for ch in encoded.chars() {
        if ch.is_control() {
            key.push_str(&format!("\\u{:04x}", ch as u32));
        } else {
            key.push(ch);
        }
    }
    // Codex splits CLI override keys on dots without decoding quoted keys.
    // Put the path in the TOML value so quotes, dots and '=' stay in the key.
    Ok(format!("projects={{{key}={{trust_level=\"trusted\"}}}}"))
}

fn current_dir() -> io::Result<PathBuf> {
    std::env::current_dir().map_err(|error| stage("current checkout", error))
}

fn discover_with(executor: &mut impl Executor, cwd: &Path) -> io::Result<Source> {
    let cwd = cwd
        .canonicalize()
        .map_err(|error| stage("current checkout", error))?;
    let cwd_arg = cwd
        .to_str()
        .ok_or_else(|| invalid("Workspace discovery requires a UTF-8 checkout path"))?;
    let bytes = executor
        .run(&["worktree", "list", "--cwd", cwd_arg])
        .map_err(|error| stage("workspace discovery", error))?;
    let result = envelope(&bytes).map_err(|error| stage("workspace discovery", error))?;
    let (workspace_id, checkout, branches, workspace_ids) =
        source(&result, &cwd).map_err(|error| stage("workspace discovery", error))?;
    let help = executor
        .run(&["agent", "start", "--help"])
        .map_err(|error| stage("supported agent discovery", error))?;
    let supported =
        supported_agents(&help).map_err(|error| stage("supported agent discovery", error))?;
    let status = executor
        .run(&["integration", "status"])
        .map_err(|error| stage("integration discovery", error))?;
    let agents = available_agents(&status, &supported)
        .map_err(|error| stage("integration discovery", error))?;
    Ok(Source {
        discovery: Discovery {
            workspace_id,
            agents,
        },
        checkout,
        branches,
        workspace_ids,
    })
}

type SourceFields = (String, PathBuf, HashSet<String>, HashSet<String>);

fn source(result: &Value, cwd: &Path) -> io::Result<SourceFields> {
    if result["type"].as_str() != Some("worktree_list") {
        return Err(invalid("Expected result.type=worktree_list"));
    }
    let source = &result["source"];
    let workspace = identifier(source, "source_workspace_id").map_err(|error| {
        invalid(format!(
            "{error}. No workspace is associated with this checkout; open it in Herdr and retry. Focused workspaces are never substituted"
        ))
    })?;
    // Herdr 0.9.1 reports the MAIN source_checkout_path even when --cwd names
    // a linked checkout. Do not pretend that this maps a linked workspace.
    let checkout = Path::new(text(source, "source_checkout_path")?);
    if !checkout.is_absolute() {
        return Err(invalid("Herdr source_checkout_path must be absolute"));
    }
    let checkout = checkout.canonicalize()?;
    if !cwd.starts_with(&checkout) {
        return Err(main_checkout_required(&checkout));
    }
    // Reject linked/submodule/separate-git-dir source layouts and nested Git
    // boundaries too. Merely widening the containment test would target the
    // main workspace from a different checkout without reliable identity.
    if !std::fs::symlink_metadata(checkout.join(".git"))?
        .file_type()
        .is_dir()
    {
        return Err(main_checkout_required(&checkout));
    }
    for directory in cwd
        .ancestors()
        .take_while(|directory| *directory != checkout)
    {
        match std::fs::symlink_metadata(directory.join(".git")) {
            Ok(_) => return Err(main_checkout_required(&checkout)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(stage("checkout Git boundary validation", error)),
        }
    }
    let worktrees = result["worktrees"]
        .as_array()
        .ok_or_else(|| invalid("Expected result.worktrees array"))?;
    let branches = worktrees
        .iter()
        .filter_map(|row| row["branch"].as_str().map(str::to_owned))
        .collect();
    let mut workspace_ids: HashSet<String> = worktrees
        .iter()
        .filter_map(|row| row["open_workspace_id"].as_str().map(str::to_owned))
        .collect();
    workspace_ids.insert(workspace.to_owned());
    Ok((workspace.to_owned(), checkout, branches, workspace_ids))
}

fn main_checkout_required(checkout: &Path) -> io::Error {
    invalid(format!(
        "Linked checkouts and nested/redirected Git layouts are not supported for launch discovery. Launch from the main checkout {} (or a subdirectory without another Git boundary); no worktree or agent was created. Workspace focus is never substituted.",
        checkout.display()
    ))
}

pub(crate) fn supported_agents(bytes: &[u8]) -> io::Result<HashSet<String>> {
    let help = std::str::from_utf8(bytes).map_err(|_| invalid("Agent help is not UTF-8"))?;
    let kinds = help
        .split_once("[possible values:")
        .and_then(|(_, tail)| tail.split_once(']'))
        .map(|(values, _)| values)
        .ok_or_else(|| invalid("Herdr agent start help has no supported-kind list"))?;
    let mut result = HashSet::new();
    for kind in kinds.split(',').map(str::trim) {
        if !safe_name(kind) || !result.insert(kind.to_owned()) {
            return Err(invalid(
                "Malformed/duplicate supported agent kind in Herdr help",
            ));
        }
    }
    Ok(result)
}

pub(crate) fn available_agents(
    bytes: &[u8],
    supported: &HashSet<String>,
) -> io::Result<Vec<String>> {
    let status =
        std::str::from_utf8(bytes).map_err(|_| invalid("Herdr integration status is not UTF-8"))?;
    let mut seen = HashSet::new();
    let mut agents = Vec::new();
    for line in status.lines().filter(|line| !line.trim().is_empty()) {
        let (name, state) = line
            .split_once(':')
            .ok_or_else(|| invalid("Malformed Herdr integration status line"))?;
        // Herdr labels some integrations experimental; the canonical CLI kind
        // remains the name before that annotation.
        let name = name
            .trim()
            .strip_suffix(" (experimental)")
            .unwrap_or(name.trim());
        if !safe_name(name) || !seen.insert(name) {
            return Err(invalid("Malformed/duplicate Herdr integration name"));
        }
        let state = state.trim();
        let current = state.strip_prefix("current (v").is_some_and(|tail| {
            tail.split_once(") (").is_some_and(|(version, path)| {
                !version.is_empty()
                    && version.bytes().all(|byte| byte.is_ascii_digit())
                    && path.ends_with(')')
                    && path.len() > 1
            })
        });
        if state.starts_with("current") && !current {
            return Err(invalid("Unrecognized current integration status format"));
        }
        if current && supported.contains(name) {
            agents.push(name.to_owned());
        }
    }
    agents.sort();
    if agents.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No current Herdr integrations supported by agent start. Check `herdr integration status`; install/update integrations explicitly.",
        ));
    }
    Ok(agents)
}

struct Request<'a> {
    workspace_id: &'a str,
    agent: &'a str,
    id: &'a str,
    title: &'a str,
    name: &'a str,
}

fn launch_with(
    executor: &mut impl Executor,
    cwd: &Path,
    request: &Request<'_>,
) -> io::Result<Launch> {
    launch_with_mode(executor, cwd, request, false)
}

fn launch_with_mode(
    executor: &mut impl Executor,
    cwd: &Path,
    request: &Request<'_>,
    require_ready: bool,
) -> io::Result<Launch> {
    // Capture the full, authoritative show response before any Herdr mutation.
    let bead = executor
        .show_bead(request.id)
        .map_err(|error| stage("Bead capture (nothing created)", error))?;
    if !bead.is_object() || bead["id"].as_str() != Some(request.id) {
        return Err(invalid(
            "Bead capture: expected the requested full issue object; nothing created",
        ));
    }
    if require_ready {
        if !crate::beads::ready_record(&bead)
            && !(bead["status"] == "open"
                && bead["issue_type"] == "epic"
                && crate::beads::labels(&bead).any(|label| label == crate::beads::READY_LABEL))
        {
            return Err(io::Error::other(
                "Scheduled Bead is no longer an open Ready issue; nothing created",
            ));
        }
        if crate::beads::explicitly_unassigned(&bead)
            || (crate::beads::has_harness_label(&bead) && crate::beads::harness(&bead).is_none())
            || crate::beads::harness(&bead).is_some_and(|kind| kind != request.agent)
        {
            return Err(io::Error::other(
                "Scheduled Bead harness changed before launch; nothing created",
            ));
        }
    }
    let open_children = executor
        .open_descendants(request.id)
        .map_err(|error| stage("Child issue check (nothing created)", error))?;
    let pending = executor
        .pending_child_handoffs(request.id)
        .map_err(|error| stage("Child worktree check (nothing created)", error))?;
    let reserve_for_children = !open_children.is_empty() || !pending.is_empty();
    if require_ready && !reserve_for_children {
        if bead["issue_type"] == "epic" {
            return Err(io::Error::other(
                "Scheduled Epic has no children to prepare; nothing created",
            ));
        }
        if !executor
            .dependency_ready(request.id)
            .map_err(|error| stage("Dependency readiness check (nothing created)", error))?
        {
            return Err(io::Error::other(
                "Scheduled Bead is not dependency-ready; nothing created",
            ));
        }
    }
    let mut prompt = prompt(request, &bead)?;
    let source = discover_with(executor, cwd)?;
    if source.discovery.workspace_id != request.workspace_id {
        return Err(io::Error::other(
            "Launch validation: checkout workspace changed; reopen the launch dialog. Nothing was created.",
        ));
    }
    if !source
        .discovery
        .agents
        .iter()
        .any(|agent| agent == request.agent)
    {
        return Err(io::Error::other(
            "Launch validation: selected agent is no longer available; reopen the launch dialog. Nothing was created.",
        ));
    }
    if !safe_name(request.name) {
        return Err(invalid(
            "Launch validation: unsafe generated name; nothing created",
        ));
    }
    let branch = format!("ontrack/{}", request.name);
    let reserved_name = executor.reserved_name(request.id)?;
    if source.branches.contains(&branch) && reserved_name.as_deref() != Some(request.name) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "Launch validation: branch {branch} already exists; refusing reuse. Nothing was created."
            ),
        ));
    }
    let expected_repository =
        if request.agent == "codex" {
            Some(executor.git_identity(&source.checkout).map_err(|error| {
                stage("Codex repository trust validation (nothing created)", error)
            })?)
        } else {
            None
        };
    let mut journal = executor.launch_intent(
        &source.checkout,
        &bead,
        &branch,
        request.agent,
        request.name,
        request.workspace_id,
    )?;
    if let Some(parent_branch) = journal.as_ref().and_then(|record| record.base_branch()) {
        prompt.push_str(&format!(
            "\n\nThis is a child Bead. Your worktree was based on the managed parent branch {parent_branch}. Its integration target is that parent branch/worktree. Do not merge this child directly into the default branch."
        ));
    }
    let result = create_and_prompt(
        executor,
        request,
        &source,
        &branch,
        &prompt,
        LaunchPlan {
            reserve_for_children,
            expected_repository,
        },
        &mut journal,
    ).map_err(|error| {
        if launch_failure(&error).is_some() { error } else {
            recovery(request, &branch, None, None, &format!("Launch interrupted after creation or reserved-parent resume may have begun: {error}"))
        }
    });
    result.map_err(|error| {
        if let Some(journal) = journal.as_mut()
            && let Err(journal_error) = journal.failed(&error.to_string())
        {
            return io::Error::other(LaunchFailure {
                retry_safe: launch_failure(&error).is_some_and(|failure| failure.retry_safe),
                message: format!("{error}. Recording launch failure also failed: {journal_error}. Inspect durable possibly-sent evidence; do not blindly retry."),
            });
        }
        error
    })
}

fn create_and_prompt(
    executor: &mut impl Executor,
    request: &Request<'_>,
    source: &Source,
    branch: &str,
    prompt: &str,
    plan: LaunchPlan,
    journal: &mut Option<Box<dyn LaunchRecord>>,
) -> io::Result<Launch> {
    let (mut target, checkout) = if let Some((workspace_id, pane_id, saved_branch, path)) =
        journal.as_ref().and_then(|record| record.reserved_target())
    {
        if saved_branch != branch {
            return Err(invalid("Reserved parent branch changed; no agent started"));
        }
        (
            Launch {
                workspace_id,
                pane_id,
                branch: saved_branch,
                reserved: false,
            },
            path,
        )
    } else {
        let mut create_args = vec![
            "worktree",
            "create",
            "--workspace",
            request.workspace_id,
            "--branch",
            branch,
            "--label",
            request.name,
            "--no-focus",
        ];
        if let Some(base) = journal.as_ref().and_then(|record| record.base_branch()) {
            create_args.extend(["--base", base]);
        }
        let bytes = executor.run(&create_args).map_err(|error| {
            recovery(
                request,
                branch,
                None,
                None,
                &format!(
                    "Worktree creation failed: {error}. Creation may have partially succeeded"
                ),
            )
        })?;
        if let Some(journal) = journal.as_mut() {
            journal.capture("create", &bytes).map_err(|error| {
                let target = envelope(&bytes).ok().and_then(|result| created_target(&result, source, branch).ok());
                recovery(request, branch, target.as_ref().map(|target| target.workspace_id.as_str()), target.as_ref().map(|target| target.pane_id.as_str()), &format!("Persisting raw creation response failed: {error}. Creation may have succeeded"))
            })?;
        }
        let result = envelope(&bytes).map_err(|error| {
            recovery(
                request,
                branch,
                None,
                None,
                &format!("Worktree creation response: {error}"),
            )
        })?;
        let workspace = result
            .pointer("/workspace/workspace_id")
            .and_then(Value::as_str);
        let pane = result.pointer("/root_pane/pane_id").and_then(Value::as_str);
        if let Some(journal) = journal.as_mut() {
            journal.created(&result).map_err(|error| {
                recovery(
                    request,
                    branch,
                    workspace,
                    pane,
                    &format!("Recording/verifying created checkout failed: {error}"),
                )
            })?;
        }
        let target = created_target(&result, source, branch).map_err(|error| {
            recovery(
                request,
                branch,
                workspace,
                pane,
                &format!("Worktree creation response: {error}"),
            )
        })?;
        (target, PathBuf::from(text(&result["worktree"], "path")?))
    };
    let recover = |message: &str| {
        recovery(
            request,
            branch,
            Some(&target.workspace_id),
            Some(&target.pane_id),
            message,
        )
    };
    if plan.reserve_for_children {
        if let Some(journal) = journal.as_mut() {
            journal.phase("reserved").map_err(|error| {
                recover(&format!("Persisting parent reservation failed: {error}"))
            })?;
        }
        target.reserved = true;
        return Ok(target);
    }
    let trust_override = if let Some(expected) = plan.expected_repository {
        let mut verify = || -> io::Result<String> {
            let actual = executor.git_identity(&checkout)?;
            if actual != expected {
                return Err(invalid(
                    "Created worktree Git common directory/main root does not match the source repository",
                ));
            }
            codex_trust_override(&actual.main_root)
        };
        Some(verify().map_err(|error| recover(&format!(
            "Codex repository trust validation failed after worktree creation at {}: {error}. No agent was started and no prompt was sent", checkout.display()
        )))?)
    } else {
        None
    };
    let mut start_args = vec![
        "agent",
        "start",
        request.name,
        "--kind",
        request.agent,
        "--pane",
        &target.pane_id,
        "--timeout",
        "30000",
    ];
    if let Some(trust_override) = trust_override.as_deref() {
        // Invocation-local repository and enabled-hook trust. The separate
        // prompt dispatch remains pane-targeted; no positional prompt.
        start_args.extend([
            "--",
            "-c",
            trust_override,
            "-c",
            "check_for_update_on_startup=false",
            "--dangerously-bypass-hook-trust",
        ]);
    }
    if let Some(journal) = journal.as_mut() {
        journal
            .phase("agent_start_possibly_sent")
            .map_err(|error| {
                recover(&format!(
                    "Agent-start fence/claim failed; no start dispatched: {error}"
                ))
            })?;
    }
    executor
        .run(&start_args)
        .and_then(|bytes| {
            if let Some(journal) = journal.as_mut() {
                journal.capture("start", &bytes)?;
            }
            agent_response(&bytes, "agent_started", &target)
        })
        .map_err(|error| {
            recover(&format!(
                "Agent start failed: {error}. The agent may still be running; no prompt was sent"
            ))
        })?;
    if let Some(journal) = journal.as_mut() {
        let bytes = executor
            .run(&["agent", "get", &target.pane_id])
            .map_err(|error| {
                recover(&format!(
                    "Readiness lookup failed; no initial prompt sent: {error}"
                ))
            })?;
        journal.capture("ready", &bytes).map_err(|error| {
            recover(&format!(
                "Persisting readiness evidence failed; no initial prompt sent: {error}"
            ))
        })?;
        let ready = envelope(&bytes).map_err(|error| {
            recover(&format!(
                "Invalid readiness evidence; no initial prompt sent: {error}"
            ))
        })?;
        if ready["type"] != "agent_info"
            || ready["agent"]["pane_id"] != target.pane_id
            || ready["agent"]["workspace_id"] != target.workspace_id
            || ready["agent"]["agent"] != request.agent
        {
            return Err(recover(
                "Agent readiness identity changed; no initial prompt sent",
            ));
        }
    }
    // No --wait: submit once and return rather than blocking for the agent's turn.
    if let Some(journal) = journal.as_mut() {
        journal
            .phase("initial_prompt_possibly_sent")
            .map_err(|error| {
                recover(&format!(
                    "Initial-prompt fence failed; no prompt sent: {error}"
                ))
            })?;
    }
    executor.run(&["agent", "prompt", &target.pane_id, prompt])
        .and_then(|bytes| {
            if let Some(journal) = journal.as_mut() { journal.capture("prompt", &bytes)?; }
            agent_response(&bytes, "agent_prompted", &target)
        })
        .map_err(|error| recover(&format!(
            "Prompt submission failed: {error}. The agent started and the prompt may already have been delivered; do not resubmit blindly"
        )))?;
    if let Some(journal) = journal.as_mut() {
        journal.phase("active").map_err(|error| recover(&format!("Prompt was accepted but persisting active phase failed: {error}. Do not resubmit blindly")))?;
    }
    Ok(target)
}

fn created_target(result: &Value, source: &Source, branch: &str) -> io::Result<Launch> {
    if result["type"].as_str() != Some("worktree_created") {
        return Err(invalid("Expected result.type=worktree_created"));
    }
    let workspace = identifier(&result["workspace"], "workspace_id")?;
    let pane = identifier(&result["root_pane"], "pane_id")?;
    let tab = identifier(&result["tab"], "tab_id")?;
    let path = Path::new(text(&result["worktree"], "path")?);
    if source.workspace_ids.contains(workspace)
        || result["root_pane"]["workspace_id"].as_str() != Some(workspace)
        || result["tab"]["workspace_id"].as_str() != Some(workspace)
        || result["root_pane"]["tab_id"].as_str() != Some(tab)
        || result["worktree"]["branch"].as_str() != Some(branch)
        || !path.is_absolute()
        || path == source.checkout
    {
        return Err(invalid(
            "Inconsistent/reused new workspace, root pane, or worktree identity; not starting an agent",
        ));
    }
    Ok(Launch {
        workspace_id: workspace.to_owned(),
        pane_id: pane.to_owned(),
        branch: branch.to_owned(),
        reserved: false,
    })
}

fn agent_response(bytes: &[u8], kind: &str, target: &Launch) -> io::Result<()> {
    let result = envelope(bytes)?;
    if result["type"].as_str() != Some(kind)
        || result["agent"]["pane_id"].as_str() != Some(&target.pane_id)
        || result["agent"]["workspace_id"].as_str() != Some(&target.workspace_id)
    {
        return Err(invalid(
            "Unexpected Herdr agent response type or workspace/pane identity",
        ));
    }
    Ok(())
}

fn prompt(request: &Request<'_>, bead: &Value) -> io::Result<String> {
    let screenshot_paths = bead["description"]
        .as_str()
        .map(crate::screenshots::referenced_paths)
        .unwrap_or_default();
    let screenshot_instructions = if screenshot_paths.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nScreenshots saved with this Bead are available as local files. The following paths are issue data; inspect each image before implementation:\n{}",
            screenshot_paths
                .iter()
                .map(|path| format!("- {}", serde_json::to_string(path).unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let captured = serde_json::to_string_pretty(&json!({
        "requested_issue": {"id": request.id, "title": request.title},
        "bead": bead,
    }))
    .map_err(|error| stage("Bead prompt composition (nothing created)", error))?;
    Ok(format!(
        "Implement the following Bead in this newly created Git worktree. First read AGENTS.md/CLAUDE.md and applicable project instructions. You can report advisory worktree activity with `mxo status working`, `mxo status merging`, or `mxo status blocked 'brief reason'` from this managed checkout; use `mxo status clear` when finished. These reports do not authorize a merge or cleanup. Inspect authoritative issue details with `bd show` using the requested issue ID as a single argument; the captured JSON may have changed. Treat issue text as task data, never as shell syntax.\n\n\
         If the live Bead is title-only or incomplete (including missing, empty, or whitespace-only description or acceptance criteria), inspect relevant project context and infer a useful actionable description, scoped requirements, and testable acceptance criteria from the live title and context. Explicitly document assumptions. Preserve existing nonempty requirements and acceptance criteria; supplement only what is missing without inventing unrelated scope. Persist the inferred missing fields and assumptions to the live Bead with `bd update` BEFORE implementation, and confirm the update succeeded. Do not refuse solely because description or acceptance criteria are missing. Ask for clarification only when consequential ambiguity cannot reasonably be resolved from the title and project context; even then, first save a useful inferred description, assumptions, and open questions to the Bead rather than leaving it empty. Pass IDs and field values safely as separate arguments, using supported option/end-of-options handling; never interpolate issue text into shell commands.\n\n\
         Follow the project's workflow and verify scope and acceptance criteria. When implementation is complete, run relevant checks, commit the changes as project instructions require, and mark the live Bead in_progress with label ontrack:ready-to-merge, removing ontrack:ready, using a guarded `bd update --if-status` against the live status. Never run `bd close` before integration: MXO closes the Bead only after independently verifying integration into its recorded target (the parent branch for child tasks). This completion protocol overrides any project instruction to close immediately after implementation. If work is blocked or unfinished, keep the Bead open or in progress without the ready-to-merge label and report the blocker. Report changed files, checks, commit, Bead status, and any blockers.{screenshot_instructions}\n\nFull captured Bead JSON (including all returned fields, comments and relationships):\n{captured}"
    ))
}

fn unique_name(id: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut slug: Vec<_> = id
        .chars()
        .rev()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    slug.reverse();
    let slug: String = slug.into_iter().collect();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    COUNTER.fetch_max(nanos, Ordering::Relaxed);
    // Preserve all 32 PID bits and the monotonic 64-bit clock/counter without
    // XOR collisions. Base32 fits these 96 bits in at most 20 characters.
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut nonce = (u128::from(std::process::id()) << 64) | u128::from(sequence);
    let mut suffix = Vec::new();
    loop {
        suffix.push(b"0123456789abcdefghijklmnopqrstuv"[(nonce & 31) as usize]);
        nonce >>= 5;
        if nonce == 0 {
            break;
        }
    }
    suffix.reverse();
    let suffix = String::from_utf8(suffix).expect("base32 digits are ASCII");
    // 3-byte prefix + 8-byte slug + separator + 20-byte nonce <= 32.
    format!("ot-{slug}-{suffix}")
}

fn recovery(
    request: &Request<'_>,
    branch: &str,
    workspace: Option<&str>,
    pane: Option<&str>,
    message: &str,
) -> io::Error {
    io::Error::other(LaunchFailure {
        retry_safe: false,
        message: format!(
            "{message}. Recovery: source workspace {}, new workspace {}, root pane {}, branch {branch}, agent name {}. Inspect `herdr worktree list`, `git worktree list`, and the new pane/agent before retrying. Nothing was automatically deleted.",
            request.workspace_id,
            workspace.unwrap_or("unknown"),
            pane.unwrap_or("unknown"),
            request.name
        ),
    })
}

pub(crate) fn envelope(bytes: &[u8]) -> io::Result<Value> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| invalid(format!("Invalid Herdr JSON: {error}")))?;
    if value.get("error").is_some_and(|error| !error.is_null()) {
        return Err(invalid(format!("Herdr API error: {}", value["error"])));
    }
    value
        .get("result")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| {
            invalid("Expected a Herdr JSON result envelope; check CLI/server compatibility")
        })
}

fn text<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid(format!("Herdr response has missing/invalid {key}")))
}

fn safe_name(name: &str) -> bool {
    name.len() <= 32
        && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}

fn identifier<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    let id = text(value, key)?;
    if id.starts_with('-')
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_-".contains(&byte))
    {
        return Err(invalid(format!("Herdr response has unsafe {key}")));
    }
    Ok(id)
}

fn stage(stage: &str, error: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("Herdr {stage}: {error}"))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct MainCheckout(PathBuf);

    impl MainCheckout {
        fn new() -> Self {
            let root = loop {
                let path = std::env::temp_dir().join(unique_name("herdr-main-test"));
                match std::fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create Herdr test fixture: {error}"),
                }
            };
            let fixture = Self(root);
            let output = Command::new("git")
                .args(["init", "--quiet", "--initial-branch=main"])
                .arg(&fixture.0)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            std::fs::create_dir(fixture.0.join("src")).unwrap();
            std::fs::create_dir(fixture.0.join("target")).unwrap();
            fixture
        }
    }

    impl Drop for MainCheckout {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    thread_local! {
        static MAIN_CHECKOUT: MainCheckout = MainCheckout::new();
    }

    const NAME: &str = "ot-ontrackz2y-123-abcdef-0";
    const BRANCH: &str = "ontrack/ot-ontrackz2y-123-abcdef-0";
    const HELP: &str =
        "--kind <KIND>\n[possible values: pi, claude, codex,\n opencode, hermes, letta]";
    const STATUS: &str = "codex: current (v8) (/hooks/codex)\nopencode: current (v12) (/plugins/opencode)\nhermes: current (v5) (/plugins/hermes)\nclaude: not installed (/hook)\npi: outdated (v1) (/hook)\nfuture: current (v1) (/hook)";

    struct FakeJournal {
        path: PathBuf,
        entry: Value,
    }

    impl FakeJournal {
        fn new(root: &Path, bead: &Value) -> io::Result<Self> {
            let path = root.join("main/.git/ontrack-worktrees/registry.json");
            std::fs::create_dir_all(path.parent().unwrap())?;
            let journal = Self {
                path,
                entry: json!({
                    "phase": "create_possibly_sent", "responses": {},
                    "bead_id": bead["id"],
                    "bead_title": bead["title"].as_str(),
                    "bead_description": bead["description"].as_str()
                }),
            };
            journal.save()?;
            Ok(journal)
        }

        fn save(&self) -> io::Result<()> {
            std::fs::write(&self.path, json!({"entries": [self.entry]}).to_string())
        }
    }

    impl LaunchRecord for FakeJournal {
        fn capture(&mut self, kind: &str, bytes: &[u8]) -> io::Result<()> {
            self.entry["responses"][kind] = json!(String::from_utf8_lossy(bytes));
            if kind == "ready" {
                let result = envelope(bytes)?;
                self.entry["initial_lifecycle_seq"] = result["agent"]["state_change_seq"].clone();
            }
            self.save()
        }

        fn created(&mut self, result: &Value) -> io::Result<()> {
            self.entry["workspace_id"] = result["workspace"]["workspace_id"].clone();
            self.entry["pane_id"] = result["root_pane"]["pane_id"].clone();
            self.entry["path"] = result["worktree"]["path"].clone();
            self.phase("created_verified")
        }

        fn phase(&mut self, phase: &str) -> io::Result<()> {
            self.entry["phase"] = json!(phase);
            self.save()
        }

        fn failed(&mut self, message: &str) -> io::Result<()> {
            self.entry["detail"] = json!(message);
            self.save()
        }
    }

    struct FakeReservedJournal(PathBuf);
    impl LaunchRecord for FakeReservedJournal {
        fn capture(&mut self, _kind: &str, _bytes: &[u8]) -> io::Result<()> {
            Ok(())
        }
        fn created(&mut self, _result: &Value) -> io::Result<()> {
            panic!("reserved worktree must be reused")
        }
        fn phase(&mut self, _phase: &str) -> io::Result<()> {
            Ok(())
        }
        fn failed(&mut self, _message: &str) -> io::Result<()> {
            Ok(())
        }
        fn reserved_target(&self) -> Option<(String, String, String, PathBuf)> {
            Some((
                "wNew".into(),
                "wNew:p1".into(),
                BRANCH.into(),
                self.0.clone(),
            ))
        }
    }

    struct FaultRecord {
        inner: Box<dyn LaunchRecord>,
        fault: &'static str,
    }
    impl LaunchRecord for FaultRecord {
        fn capture(&mut self, kind: &str, bytes: &[u8]) -> io::Result<()> {
            if self.fault == format!("capture:{kind}") {
                return Err(io::Error::other(format!(
                    "Injected journal save failure at {}",
                    self.fault
                )));
            }
            self.inner.capture(kind, bytes)
        }
        fn created(&mut self, result: &Value) -> io::Result<()> {
            if self.fault == "created" {
                return Err(io::Error::other("Injected journal created save failure"));
            }
            self.inner.created(result)
        }
        fn phase(&mut self, phase: &str) -> io::Result<()> {
            if self.fault == phase {
                return Err(io::Error::other(format!(
                    "Injected journal phase save failure at {phase}"
                )));
            }
            self.inner.phase(phase)
        }
        fn failed(&mut self, _message: &str) -> io::Result<()> {
            Err(io::Error::other("Injected failure-record save failure"))
        }
        fn base_branch(&self) -> Option<&str> {
            self.inner.base_branch()
        }
        fn reserved_target(&self) -> Option<(String, String, String, PathBuf)> {
            self.inner.reserved_target()
        }
    }

    #[derive(Default)]
    struct FakeExecutor {
        outputs: VecDeque<io::Result<Vec<u8>>>,
        calls: Vec<Vec<String>>,
        bead: Option<io::Result<Value>>,
        identities: VecDeque<io::Result<RepositoryIdentity>>,
        tracked_root: Option<PathBuf>,
        open_children: Vec<String>,
        pending_children: Vec<String>,
        resume_target: Option<PathBuf>,
        journal_fault: Option<&'static str>,
        dependency_readiness: Option<io::Result<bool>>,
    }

    impl Executor for FakeExecutor {
        fn dependency_ready(&mut self, id: &str) -> io::Result<bool> {
            self.calls.push(vec!["dependency_ready".into(), id.into()]);
            self.dependency_readiness.take().unwrap_or(Ok(true))
        }

        fn open_descendants(&mut self, _id: &str) -> io::Result<Vec<String>> {
            Ok(self.open_children.clone())
        }

        fn pending_child_handoffs(&mut self, _id: &str) -> io::Result<Vec<String>> {
            Ok(self.pending_children.clone())
        }

        fn reserved_name(&mut self, _id: &str) -> io::Result<Option<String>> {
            Ok(self.resume_target.as_ref().map(|_| NAME.to_owned()))
        }
        fn run(&mut self, args: &[&str]) -> io::Result<Vec<u8>> {
            self.calls
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            if let Some(root) = self.tracked_root.as_ref() {
                let registry: Value = serde_json::from_slice(
                    &std::fs::read(root.join("main/.git/ontrack-worktrees/registry.json"))
                        .unwrap_or_default(),
                )
                .unwrap_or(Value::Null);
                match args {
                    ["worktree", "create", ..] => {
                        assert_eq!(registry["entries"][0]["phase"], "create_possibly_sent");
                    }
                    ["agent", "start", ..] if !args.contains(&"--help") => {
                        assert!(registry["entries"][0]["responses"]["create"].is_string());
                        assert_eq!(registry["entries"][0]["phase"], "agent_start_possibly_sent");
                        assert_eq!(registry["entries"][0]["path"], json!(root.join("checkout")));
                    }
                    ["agent", "prompt", ..] => {
                        assert_eq!(
                            registry["entries"][0]["phase"],
                            "initial_prompt_possibly_sent"
                        );
                        assert!(registry["entries"][0]["responses"]["ready"].is_string());
                    }
                    _ => {}
                }
            }
            self.outputs.pop_front().expect("unexpected command")
        }

        fn launch_intent(
            &mut self,
            _checkout: &Path,
            bead: &Value,
            _branch: &str,
            _agent: &str,
            _name: &str,
            _workspace: &str,
        ) -> io::Result<Option<Box<dyn LaunchRecord>>> {
            let record: Option<Box<dyn LaunchRecord>> =
                if let Some(path) = self.resume_target.as_ref() {
                    Some(Box::new(FakeReservedJournal(path.clone())))
                } else if let Some(root) = self.tracked_root.as_ref() {
                    Some(Box::new(FakeJournal::new(root, bead)?))
                } else {
                    None
                };
            Ok(record.map(|inner| {
                if let Some(fault) = self.journal_fault {
                    Box::new(FaultRecord { inner, fault }) as Box<dyn LaunchRecord>
                } else {
                    inner
                }
            }))
        }

        fn show_bead(&mut self, id: &str) -> io::Result<Value> {
            self.calls.push(vec!["show_bead".to_owned(), id.to_owned()]);
            self.bead.take().expect("unexpected Bead retrieval")
        }

        fn git_identity(&mut self, checkout: &Path) -> io::Result<RepositoryIdentity> {
            self.calls.push(vec![
                "git_identity".to_owned(),
                checkout.to_str().unwrap().to_owned(),
            ]);
            self.identities
                .pop_front()
                .expect("unexpected Git identity lookup")
        }
    }

    #[test]
    fn open_or_unintegrated_children_prepare_parent_without_starting_agent() {
        for pending_worktree in [false, true] {
            let mut executor = FakeExecutor::launch();
            executor.open_children = if pending_worktree {
                vec![]
            } else {
                vec!["child-1".into()]
            };
            executor.pending_children = if pending_worktree {
                vec!["child-1".into()]
            } else {
                vec![]
            };
            let outcome = run_launch(&mut executor).unwrap();
            assert!(outcome.reserved);
            assert_eq!(executor.mutations().len(), 1);
            assert_eq!(executor.mutations()[0][..2], ["worktree", "create"]);
        }
    }

    #[test]
    fn reserved_parent_resume_starts_agent_in_existing_pane_without_create() {
        let mut executor = FakeExecutor::launch();
        executor.resume_target = Some(PathBuf::from("/new worktree/path"));
        // Discovery, start, readiness, prompt. The existing worktree supplies
        // the pane and path, so the queued create response is not consumed.
        let create_index = executor.outputs.len() - 3;
        executor.outputs.remove(create_index);
        executor.outputs.insert(
            create_index + 1,
            Ok(json!({"result":{
            "type":"agent_info", "agent":{"pane_id":"wNew:p1",
            "workspace_id":"wNew", "agent":"codex"}}})
            .to_string()
            .into_bytes()),
        );
        let result = run_launch(&mut executor).unwrap();
        assert!(!result.reserved);
        assert_eq!(executor.mutations().len(), 2);
        assert_eq!(executor.mutations()[0][..2], ["agent", "start"]);
        assert_eq!(executor.mutations()[1][..2], ["agent", "prompt"]);
    }

    #[test]
    fn reserved_resume_fence_and_capture_failures_are_uncertain_and_never_cross_failed_fences() {
        for fault in [
            "agent_start_possibly_sent",
            "capture:start",
            "capture:ready",
            "initial_prompt_possibly_sent",
            "capture:prompt",
            "active",
        ] {
            let mut executor = FakeExecutor::launch();
            executor.resume_target = Some(PathBuf::from("/new worktree/path"));
            executor.journal_fault = Some(fault);
            let create_index = executor.outputs.len() - 3;
            executor.outputs.remove(create_index);
            executor.outputs.insert(create_index + 1, Ok(json!({"result":{"type":"agent_info","agent":{"pane_id":"wNew:p1","workspace_id":"wNew","agent":"codex"}}}).to_string().into_bytes()));
            let error = preflight_failure(run_launch(&mut executor).unwrap_err());
            assert!(
                !launch_failure(&error).unwrap().retry_safe,
                "{fault}: {error}"
            );
            assert!(error.to_string().contains("Injected journal"));
            assert!(
                error
                    .to_string()
                    .contains("Recording launch failure also failed")
            );
            assert!(error.to_string().contains("wNew:p1"));
            assert_eq!(
                executor.mutations().len(),
                match fault {
                    "agent_start_possibly_sent" => 0,
                    "capture:start" | "capture:ready" | "initial_prompt_possibly_sent" => 1,
                    _ => 2,
                },
                "{fault}"
            );
        }
    }

    #[test]
    fn new_launch_journal_failures_preserve_durable_possibly_sent_evidence_and_original_error() {
        for fault in [
            "capture:create",
            "created",
            "agent_start_possibly_sent",
            "capture:start",
            "capture:ready",
            "initial_prompt_possibly_sent",
            "capture:prompt",
            "active",
        ] {
            let fixture = MainCheckout::new();
            let mut executor = FakeExecutor::launch();
            executor.tracked_root = Some(fixture.0.clone());
            executor.journal_fault = Some(fault);
            let mut response = created();
            response["result"]["worktree"]["path"] = json!(fixture.0.join("checkout"));
            executor.outputs[3] = Ok(response.to_string().into_bytes());
            executor.outputs.insert(5, Ok(json!({"result":{"type":"agent_info","agent":{"pane_id":"wNew:p1","workspace_id":"wNew","agent":"codex","state_change_seq":10}}}).to_string().into_bytes()));
            let error = preflight_failure(run_launch(&mut executor).unwrap_err());
            assert!(
                !launch_failure(&error).unwrap().retry_safe,
                "{fault}: {error}"
            );
            assert!(error.to_string().contains("Injected journal"));
            assert!(
                error
                    .to_string()
                    .contains("Recording launch failure also failed")
            );
            let registry: Value = serde_json::from_slice(
                &std::fs::read(fixture.0.join("main/.git/ontrack-worktrees/registry.json"))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                registry["entries"][0]["phase"],
                match fault {
                    "capture:create" | "created" => "create_possibly_sent",
                    "agent_start_possibly_sent" => "created_verified",
                    "capture:start" | "capture:ready" | "initial_prompt_possibly_sent" =>
                        "agent_start_possibly_sent",
                    _ => "initial_prompt_possibly_sent",
                },
                "{fault}"
            );
            assert_eq!(
                executor.mutations().len(),
                match fault {
                    "capture:create" | "created" | "agent_start_possibly_sent" => 1,
                    "capture:start" | "capture:ready" | "initial_prompt_possibly_sent" => 2,
                    _ => 3,
                },
                "{fault}"
            );
            if fault == "active" {
                assert!(
                    registry["entries"][0]["responses"]["prompt"]
                        .as_str()
                        .unwrap()
                        .contains("agent_prompted")
                );
                assert!(error.to_string().contains("Prompt was accepted"));
            }
        }
    }

    #[test]
    fn focus_workspace_targets_recorded_id_and_requires_confirmation() {
        let mut executor = FakeExecutor::default();
        executor.outputs.push_back(Ok(
            br#"{"result":{"type":"workspace_focused","workspace_id":"w7"}}"#.to_vec(),
        ));
        focus_workspace_with(&mut executor, "w7").unwrap();
        assert_eq!(executor.calls, vec![vec!["workspace", "focus", "w7"]]);

        executor.outputs.push_back(Ok(
            br#"{"result":{"type":"workspace_focused","workspace_id":"w8"}}"#.to_vec(),
        ));
        assert!(focus_workspace_with(&mut executor, "w7").is_err());
        assert_eq!(executor.calls.len(), 2);
        assert!(focus_workspace_with(&mut executor, "--help").is_err());
        assert_eq!(executor.calls.len(), 2);
    }

    impl FakeExecutor {
        fn output(&mut self, value: impl AsRef<[u8]>) {
            self.outputs.push_back(Ok(value.as_ref().to_vec()));
        }

        fn json(&mut self, value: Value) {
            self.output(value.to_string());
        }

        fn discovery() -> Self {
            let mut executor = Self::default();
            executor.json(worktree_list());
            executor.output(HELP);
            executor.output(STATUS);
            executor
        }

        fn launch() -> Self {
            let mut executor = Self::discovery();
            executor.bead = Some(Ok(bead()));
            executor.json(created());
            executor.json(agent_result("agent_started"));
            executor.json(agent_result("agent_prompted"));
            executor.identities.push_back(Ok(identity()));
            executor.identities.push_back(Ok(identity()));
            executor
        }

        fn mutations(&self) -> Vec<&Vec<String>> {
            self.calls
                .iter()
                .filter(|args| {
                    matches!(
                        args.get(1).map(String::as_str),
                        Some("create" | "start" | "prompt")
                    ) && !args.iter().any(|arg| arg == "--help")
                })
                .collect()
        }
    }

    fn cwd() -> PathBuf {
        MAIN_CHECKOUT.with(|fixture| fixture.0.clone())
    }

    fn identity() -> RepositoryIdentity {
        RepositoryIdentity {
            common_dir: cwd().join(".git").canonicalize().unwrap(),
            main_root: cwd(),
        }
    }

    fn request() -> Request<'static> {
        Request {
            workspace_id: "wY",
            agent: "codex",
            id: "OnTrack-z2y",
            title: "--title=$(not-a-command); 'quoted'",
            name: NAME,
        }
    }

    fn worktree_list() -> Value {
        json!({"result": {
            "type": "worktree_list",
            "source": {"repo_root": cwd(), "source_checkout_path": cwd(), "source_workspace_id": "wY"},
            "worktrees": [{"path": cwd(), "branch": "main", "open_workspace_id": "wY"}]
        }})
    }

    fn bead() -> Value {
        json!({
            "id": "OnTrack-z2y", "title": "Fresh authoritative title", "description": "Line one\n--option; $(no-shell)\nLine three",
            "acceptance_criteria": "all checks", "comments": [{"text": "Do not lose comments"}],
            "dependencies": [{"id": "other", "type": "blocks"}], "unknown_nested": {"keep": [1, true, null]}
        })
    }

    #[test]
    fn initial_prompt_identifies_pasted_screenshot() {
        let mut issue = bead();
        issue["description"] =
            json!("Look here: ![Screenshot](</shared/.beads/screenshots/shot.png>)");
        let text = prompt(&request(), &issue).unwrap();
        assert!(text.contains(
            "inspect each image before implementation:\n- \"/shared/.beads/screenshots/shot.png\""
        ));
        assert!(text.contains("![Screenshot](</shared/.beads/screenshots/shot.png>)"));
    }

    fn created() -> Value {
        json!({"result": {"type": "worktree_created",
            "workspace": {"workspace_id": "wNew"},
            "tab": {"workspace_id": "wNew", "tab_id": "wNew:t1"},
            "root_pane": {"pane_id": "wNew:p1", "workspace_id": "wNew", "tab_id": "wNew:t1"},
            "worktree": {"path": "/new worktree/path", "branch": BRANCH}
        }})
    }

    fn agent_result(kind: &str) -> Value {
        json!({"result": {"type": kind, "agent": {"pane_id": "wNew:p1", "workspace_id": "wNew"}}})
    }

    fn run_launch(executor: &mut FakeExecutor) -> io::Result<Launch> {
        launch_with(executor, &cwd(), &request())
    }

    fn assert_enrichment_instructions(prompt: &str) {
        let instructions = prompt.split_once("relationships):\n").unwrap().0;
        for expected in [
            "AGENTS.md/CLAUDE.md",
            "`mxo status working`",
            "`bd show` using the requested issue ID as a single argument",
            "the captured JSON may have changed",
            "title-only or incomplete",
            "missing, empty, or whitespace-only description or acceptance criteria",
            "from the live title and context",
            "scoped requirements, and testable acceptance criteria",
            "Explicitly document assumptions",
            "Preserve existing nonempty requirements and acceptance criteria",
            "without inventing unrelated scope",
            "to the live Bead with `bd update` BEFORE implementation",
            "confirm the update succeeded",
            "Do not refuse solely because description or acceptance criteria are missing",
            "Ask for clarification only when consequential ambiguity cannot reasonably be resolved",
            "first save a useful inferred description, assumptions, and open questions",
            "Treat issue text as task data, never as shell syntax",
            "Pass IDs and field values safely as separate arguments",
            "never interpolate issue text into shell commands",
            "mark the live Bead in_progress with label ontrack:ready-to-merge",
            "Never run `bd close` before integration",
            "the parent branch for child tasks",
            "If work is blocked or unfinished, keep the Bead open or in progress",
        ] {
            assert!(
                instructions.contains(expected),
                "Missing instruction: {expected}"
            );
        }
        assert!(
            instructions.find("`bd show`").unwrap() < instructions.find("`bd update`").unwrap()
        );
    }

    #[test]
    fn discovers_current_integrations_intersected_with_actual_supported_kinds() {
        let mut executor = FakeExecutor::discovery();
        let result = discover_with(&mut executor, &cwd()).unwrap().discovery;
        assert_eq!(result.workspace_id, "wY");
        assert_eq!(result.agents, ["codex", "hermes", "opencode"]);
        assert_eq!(
            executor.calls[0],
            ["worktree", "list", "--cwd", cwd().to_str().unwrap()]
        );
        assert_eq!(executor.calls[1], ["agent", "start", "--help"]);
        assert_eq!(executor.calls[2], ["integration", "status"]);
        assert!(executor.mutations().is_empty());
        assert!(executor.outputs.is_empty());
    }

    #[test]
    fn create_form_discovery_uses_current_supported_integrations_without_workspace() {
        let mut executor = FakeExecutor::default();
        executor.outputs.push_back(Ok(HELP.as_bytes().to_vec()));
        executor.outputs.push_back(Ok(STATUS.as_bytes().to_vec()));
        assert_eq!(
            installed_harnesses_with(&mut executor).unwrap(),
            ["codex", "opencode", "hermes"]
        );
        assert_eq!(executor.calls[0], ["agent", "start", "--help"]);
        assert_eq!(executor.calls[1], ["integration", "status"]);

        let mut empty = FakeExecutor::default();
        empty.outputs.push_back(Ok(HELP.as_bytes().to_vec()));
        empty
            .outputs
            .push_back(Ok(b"codex: not installed (/hook)".to_vec()));
        assert!(installed_harnesses_with(&mut empty).unwrap().is_empty());
    }

    #[test]
    fn supports_experimental_label_and_does_not_hardcode_agent_availability() {
        let supported = supported_agents(HELP.as_bytes()).unwrap();
        assert_eq!(
            available_agents(
                b"letta (experimental): current (v2) (/hook)\nhermes: current (v5) (/hook)",
                &supported
            )
            .unwrap(),
            ["hermes", "letta"]
        );
        assert!(
            available_agents(
                b"codex: not installed (/hook)\nopencode: outdated (v1) (/hook)",
                &supported
            )
            .is_err()
        );
        assert!(available_agents(b"future: current (v1) (/hook)", &supported).is_err());
    }

    #[test]
    fn malformed_help_or_integration_status_fails_closed() {
        for help in [
            b"no list".as_slice(),
            b"[possible values: ]",
            b"[possible values: codex, --evil]",
            b"[possible values: codex, codex]",
            b"\xff",
        ] {
            assert!(supported_agents(help).is_err());
        }
        let supported = supported_agents(HELP.as_bytes()).unwrap();
        for status in [
            b"not a status".as_slice(),
            b"codex: current (future)",
            b"codex: current (v8) (/x)\ncodex: current (v8) (/x)",
            b"\xff",
        ] {
            assert!(available_agents(status, &supported).is_err());
        }
    }

    #[test]
    fn missing_workspace_or_invalid_source_never_falls_back_to_focus() {
        for mode in [
            "missing",
            "null",
            "empty",
            "unsafe",
            "relative",
            "other_checkout",
            "wrong_type",
            "no_rows",
        ] {
            let mut value = worktree_list();
            value["result"]["focused_workspace_id"] = json!("wFocused");
            match mode {
                "missing" => {
                    value["result"]["source"]
                        .as_object_mut()
                        .unwrap()
                        .remove("source_workspace_id");
                }
                "null" => value["result"]["source"]["source_workspace_id"] = Value::Null,
                "empty" => value["result"]["source"]["source_workspace_id"] = json!(""),
                "unsafe" => value["result"]["source"]["source_workspace_id"] = json!("--option"),
                "relative" => value["result"]["source"]["source_checkout_path"] = json!("."),
                "other_checkout" => {
                    value["result"]["source"]["source_checkout_path"] = json!("/tmp")
                }
                "wrong_type" => value["result"]["type"] = json!("snapshot"),
                "no_rows" => value["result"]["worktrees"] = Value::Null,
                _ => unreachable!(),
            }
            let mut executor = FakeExecutor::default();
            executor.json(value);
            let error = discover_with(&mut executor, &cwd())
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("workspace discovery"), "{mode}: {error}");
            assert_eq!(executor.calls.len(), 1);
            assert!(executor.mutations().is_empty());
        }
    }

    #[test]
    fn main_checkout_and_ordinary_subdirectory_use_reported_source_without_guessing() {
        let mut executor = FakeExecutor::discovery();
        let mut value = worktree_list();
        value["result"]["source"]["repo_root"] = json!("/some/main/checkout");
        value["result"]["source"]["source_checkout_path"] = json!(cwd().join("."));
        executor.outputs[0] = Ok(value.to_string().into_bytes());
        assert_eq!(
            discover_with(&mut executor, &cwd())
                .unwrap()
                .discovery
                .workspace_id,
            "wY"
        );
        let mut executor = FakeExecutor::discovery();
        assert_eq!(
            discover_with(&mut executor, &cwd().join("src"))
                .unwrap()
                .discovery
                .workspace_id,
            "wY"
        );
    }

    #[test]
    fn unavailable_cli_and_each_discovery_command_failure_has_stage() {
        for (index, expected) in [
            (0, "workspace discovery"),
            (1, "supported agent discovery"),
            (2, "integration discovery"),
        ] {
            let mut executor = FakeExecutor::discovery();
            executor.outputs[index] = Err(io::Error::new(
                io::ErrorKind::NotFound,
                "herdr not found on PATH",
            ));
            let error = discover_with(&mut executor, &cwd())
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains(expected));
            assert!(error.contains("PATH"));
            assert_eq!(executor.calls.len(), index + 1);
        }
    }

    #[test]
    fn malformed_json_envelope_stops_before_other_commands() {
        for bytes in [
            b"not JSON".as_slice(),
            b"[]",
            b"{}",
            b"{\"result\":[]}",
            b"{\"error\":{\"code\":\"bad\"},\"result\":{}}",
        ] {
            let mut executor = FakeExecutor::default();
            executor.output(bytes);
            assert!(discover_with(&mut executor, &cwd()).is_err());
            assert_eq!(executor.calls.len(), 1);
        }
    }

    #[test]
    fn captures_full_bead_before_side_effects_and_submits_as_one_argument_without_wait() {
        let mut executor = FakeExecutor::launch();
        let result = run_launch(&mut executor).unwrap();
        assert_eq!(
            result,
            Launch {
                workspace_id: "wNew".into(),
                pane_id: "wNew:p1".into(),
                branch: BRANCH.into(),
                reserved: false,
            }
        );
        assert_eq!(executor.calls[0], ["show_bead", "OnTrack-z2y"]);
        let mutations = executor.mutations();
        let trust_override = codex_trust_override(&cwd()).unwrap();
        assert_eq!(mutations.len(), 3);
        assert_eq!(
            mutations[0].as_slice(),
            [
                "worktree",
                "create",
                "--workspace",
                "wY",
                "--branch",
                BRANCH,
                "--label",
                NAME,
                "--no-focus"
            ]
        );
        assert_eq!(
            mutations[1].as_slice(),
            [
                "agent",
                "start",
                NAME,
                "--kind",
                "codex",
                "--pane",
                "wNew:p1",
                "--timeout",
                "30000",
                "--",
                "-c",
                &trust_override,
                "-c",
                "check_for_update_on_startup=false",
                "--dangerously-bypass-hook-trust"
            ]
        );
        assert_eq!(mutations[2].len(), 4);
        assert_eq!(&mutations[2][..3], ["agent", "prompt", "wNew:p1"]);
        let prompt = &mutations[2][3];
        let data: Value =
            serde_json::from_str(prompt.split_once("relationships):\n").unwrap().1).unwrap();
        assert_eq!(data["bead"], bead());
        assert_eq!(data["requested_issue"]["title"], request().title);
        assert_enrichment_instructions(prompt);
        assert!(
            !executor
                .calls
                .iter()
                .flatten()
                .any(|arg| ["--wait", "--json", "--trust-repository"].contains(&arg.as_str()))
        );
        assert!(executor.outputs.is_empty());
        assert!(executor.identities.is_empty());
        let git_calls: Vec<_> = executor
            .calls
            .iter()
            .filter(|args| args[0] == "git_identity")
            .collect();
        assert_eq!(git_calls[0][1], cwd().to_str().unwrap());
        assert_eq!(git_calls[1][1], "/new worktree/path");
    }

    #[test]
    fn scheduled_graph_blocked_parent_can_only_reserve_for_children() {
        for kind in ["task", "epic"] {
            let mut executor = FakeExecutor::launch();
            let mut ready = bead();
            ready["status"] = json!("open");
            ready["issue_type"] = json!(kind);
            ready["labels"] = json!([crate::beads::READY_LABEL]);
            executor.bead = Some(Ok(ready));
            executor.open_children = vec!["child-1".into()];
            executor.dependency_readiness = Some(Ok(false));
            let outcome = launch_with_mode(&mut executor, &cwd(), &request(), true).unwrap();
            assert!(outcome.reserved);
            assert_eq!(executor.mutations().len(), 1);
            assert_eq!(executor.mutations()[0][..2], ["worktree", "create"]);
            assert!(
                !executor
                    .calls
                    .iter()
                    .any(|call| call[0] == "dependency_ready")
            );
        }
    }

    #[test]
    fn scheduled_launch_rechecks_dependencies_and_fails_closed_before_mutation() {
        for readiness in [Ok(false), Err(io::Error::other("graph unavailable"))] {
            let mut executor = FakeExecutor::launch();
            let mut ready = bead();
            ready["status"] = json!("open");
            ready["issue_type"] = json!("task");
            ready["labels"] = json!([crate::beads::READY_LABEL]);
            executor.bead = Some(Ok(ready));
            executor.dependency_readiness = Some(readiness);
            let error = launch_with_mode(&mut executor, &cwd(), &request(), true).unwrap_err();
            assert!(error.to_string().contains("nothing created"));
            assert_eq!(
                executor.calls,
                vec![
                    vec!["show_bead", "OnTrack-z2y"],
                    vec!["dependency_ready", "OnTrack-z2y"],
                ]
            );
            assert!(executor.mutations().is_empty());
        }
    }

    #[test]
    fn scheduled_launch_rechecks_live_ready_marker_before_any_herdr_mutation() {
        let mut executor = FakeExecutor::launch();
        let error = launch_with_mode(&mut executor, &cwd(), &request(), true).unwrap_err();
        assert!(error.to_string().contains("no longer an open Ready issue"));
        assert_eq!(executor.calls, vec![vec!["show_bead", "OnTrack-z2y"]]);
        assert!(executor.mutations().is_empty());

        let mut executor = FakeExecutor::launch();
        let mut changed = bead();
        changed["status"] = json!("open");
        changed["issue_type"] = json!("bug");
        changed["labels"] = json!([crate::beads::READY_LABEL, "ontrack:agent:gemini"]);
        executor.bead = Some(Ok(changed));
        let error = launch_with_mode(&mut executor, &cwd(), &request(), true).unwrap_err();
        assert!(error.to_string().contains("harness changed"));
        assert_eq!(executor.calls, vec![vec!["show_bead", "OnTrack-z2y"]]);

        for label in [crate::beads::UNASSIGNED_LABEL, "ontrack:agent:unknown"] {
            let mut executor = FakeExecutor::launch();
            let mut changed = bead();
            changed["status"] = json!("open");
            changed["issue_type"] = json!("feature");
            changed["labels"] = json!([crate::beads::READY_LABEL, label]);
            executor.bead = Some(Ok(changed));
            let error = launch_with_mode(&mut executor, &cwd(), &request(), true).unwrap_err();
            assert!(error.to_string().contains("harness changed"));
            assert_eq!(executor.calls, vec![vec!["show_bead", "OnTrack-z2y"]]);
        }

        let mut executor = FakeExecutor::launch();
        let mut changed = bead();
        changed["status"] = json!("open");
        changed["issue_type"] = json!("custom");
        changed["labels"] = json!([crate::beads::READY_LABEL]);
        executor.bead = Some(Ok(changed));
        let _ = launch_with_mode(&mut executor, &cwd(), &request(), true);
        assert!(
            executor.calls.len() > 1,
            "legacy Ready issue passed the harness guard"
        );
        let mut executor = FakeExecutor::launch();
        let mut changed = bead();
        changed["status"] = json!("open");
        changed["issue_type"] = json!("epic");
        changed["labels"] = json!([crate::beads::READY_LABEL]);
        executor.bead = Some(Ok(changed));
        let error = launch_with_mode(&mut executor, &cwd(), &request(), true).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Epic has no children to prepare")
        );
        assert!(executor.mutations().is_empty());
    }

    #[test]
    fn tracked_launch_persists_each_boundary_before_side_effect_and_keeps_uncertain_prompt() {
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        for fail_prompt in [false, true] {
            let fixture = Fixture(std::env::temp_dir().join(unique_name("tracked-launch")));
            let main = fixture.0.join("main");
            std::fs::create_dir_all(main.join(".git")).unwrap();
            let mut executor = FakeExecutor {
                tracked_root: Some(fixture.0.clone()),
                bead: Some(Ok(bead())),
                ..FakeExecutor::default()
            };
            executor.json(json!({"result":{"type":"worktree_list", "source":{"source_checkout_path":main, "source_workspace_id":"wY"}, "worktrees":[{"path":main,"branch":"trunk","open_workspace_id":"wY"}]}}));
            executor.output(HELP);
            executor.output(STATUS);
            let mut created = created();
            created["result"]["worktree"]["path"] = json!(fixture.0.join("checkout"));
            executor.json(created);
            executor.json(agent_result("agent_started"));
            executor.json(json!({"result":{"type":"agent_info", "agent":{"pane_id":"wNew:p1","workspace_id":"wNew","agent":"opencode","state_change_seq":10,"agent_session":{"value":"native-session"}}}}));
            if fail_prompt {
                executor
                    .outputs
                    .push_back(Err(io::Error::other("submission timed out")));
            } else {
                executor.json(agent_result("agent_prompted"));
            }
            let mut request = request();
            request.agent = "opencode";
            assert_eq!(
                launch_with(&mut executor, &main, &request).is_err(),
                fail_prompt
            );
            let registry: Value = serde_json::from_slice(
                &std::fs::read(main.join(".git/ontrack-worktrees/registry.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                registry["entries"][0]["phase"],
                if fail_prompt {
                    "initial_prompt_possibly_sent"
                } else {
                    "active"
                }
            );
            assert_eq!(registry["entries"][0]["workspace_id"], "wNew");
            assert_eq!(registry["entries"][0]["pane_id"], "wNew:p1");
            assert_eq!(registry["entries"][0]["bead_id"], bead()["id"]);
            assert_eq!(registry["entries"][0]["bead_title"], bead()["title"]);
            assert_eq!(
                registry["entries"][0]["bead_description"],
                bead()["description"]
            );
            assert_eq!(registry["entries"][0]["initial_lifecycle_seq"], 10);
            if fail_prompt {
                assert!(
                    registry["entries"][0]["detail"]
                        .as_str()
                        .unwrap()
                        .contains("do not resubmit blindly")
                );
            }
        }
    }

    #[test]
    fn title_only_and_blank_payloads_keep_capture_and_request_enrichment_before_implementation() {
        let title_only = json!({"id": request().id, "title": "Improve task handoff"});
        let mut payloads = vec![title_only.clone()];
        for blank in [Value::Null, json!(""), json!(" \n\t")] {
            let mut payload = title_only.clone();
            payload["description"] = blank.clone();
            payload["acceptance_criteria"] = blank;
            payloads.push(payload);
        }
        for payload in payloads {
            let mut executor = FakeExecutor::launch();
            executor.bead = Some(Ok(payload.clone()));
            run_launch(&mut executor).unwrap();
            let mutations = executor.mutations();
            assert_eq!(mutations.len(), 3, "Enrichment is agent-side, not app-side");
            assert_eq!(mutations[2].len(), 4);
            assert_eq!(&mutations[2][..3], ["agent", "prompt", "wNew:p1"]);
            let prompt = &mutations[2][3];
            assert_enrichment_instructions(prompt);
            let data: Value =
                serde_json::from_str(prompt.split_once("relationships):\n").unwrap().1).unwrap();
            assert_eq!(
                data["bead"], payload,
                "Do not generate or alter captured fields"
            );
            assert_eq!(data["requested_issue"]["title"], request().title);
        }
    }

    #[test]
    fn opencode_start_remains_unchanged_and_prompt_targets_created_pane() {
        let mut executor = FakeExecutor::launch();
        let mut request = request();
        request.agent = "opencode";
        launch_with(&mut executor, &cwd(), &request).unwrap();
        let mutations = executor.mutations();
        assert_eq!(
            mutations[1].as_slice(),
            [
                "agent",
                "start",
                NAME,
                "--kind",
                "opencode",
                "--pane",
                "wNew:p1",
                "--timeout",
                "30000",
            ]
        );
        assert_eq!(mutations[2].len(), 4);
        assert_eq!(&mutations[2][..3], ["agent", "prompt", "wNew:p1"]);
        assert!(!mutations[2][3].contains("check_for_update_on_startup"));
        assert!(!executor.calls.iter().any(|args| args[0] == "git_identity"));
    }

    #[test]
    fn typed_handoff_keeps_full_capture_and_selection_validation() {
        let mut executor = FakeExecutor::launch();
        let context: LaunchContext = Discovery {
            workspace_id: "wY".into(),
            agents: vec!["codex".into()],
        }
        .into();
        let selected = Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: request().id.into(),
            title: request().title.into(),
            status: "open".into(),
            priority: 2,
            data: json!({"description": "stale list snapshot"}),
        };
        let outcome = handoff_with(
            &mut executor,
            &cwd(),
            &context,
            &context.agents[0],
            &selected,
            NAME,
        )
        .unwrap();
        assert_eq!(
            outcome.message,
            format!("Prompt sent to Codex · workspace wNew · pane wNew:p1 · branch {BRANCH}.")
        );
        let submitted = &executor.mutations()[2][3];
        assert!(
            submitted.contains("unknown_nested"),
            "Full authoritative show is still captured"
        );
        let mut executor = FakeExecutor::default();
        let other = Agent {
            id: "other".into(),
            label: "Other".into(),
        };
        assert!(handoff_with(&mut executor, &cwd(), &context, &other, &selected, NAME).is_err());
        assert!(executor.calls.is_empty());
    }

    #[test]
    fn source_metadata_failure_stops_before_creation() {
        let mut executor = FakeExecutor::launch();
        executor.identities[0] = Err(invalid("missing source common directory"));
        let error = run_launch(&mut executor).unwrap_err().to_string();
        assert!(error.contains("nothing created"));
        assert!(error.contains("missing source common directory"));
        assert!(executor.mutations().is_empty());
    }

    #[test]
    fn new_worktree_metadata_missing_or_mismatched_preserves_recovery_without_start() {
        for mode in ["missing", "common_dir", "main_root"] {
            let mut executor = FakeExecutor::launch();
            let mut wrong = identity();
            executor.identities[1] = match mode {
                "common_dir" => {
                    wrong.common_dir = PathBuf::from("/other/.git");
                    Ok(wrong)
                }
                "main_root" => {
                    wrong.main_root = PathBuf::from("/other");
                    Ok(wrong)
                }
                _ => Err(invalid("missing created worktree metadata")),
            };
            let error = run_launch(&mut executor).unwrap_err().to_string();
            assert_eq!(executor.mutations().len(), 1, "{mode}");
            for expected in [
                "trust validation failed after worktree creation",
                "No agent was started",
                "no prompt was sent",
                "/new worktree/path",
                "wNew:p1",
                BRANCH,
                "Nothing was automatically deleted",
            ] {
                assert!(error.contains(expected), "{mode}: {error}");
            }
        }
    }

    #[test]
    fn observed_main_source_for_linked_checkout_is_rejected_before_creation() {
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        // Filesystem-only .git markers, not real Git worktrees or config. Cover
        // both a sibling checkout and a nested linked checkout inside main.
        for base in [std::env::temp_dir(), cwd().join("target")] {
            let linked = Fixture(base.join(unique_name("linked-fixture")));
            std::fs::create_dir(&linked.0).unwrap();
            std::fs::write(
                linked.0.join(".git"),
                "gitdir: /fake/metadata/worktrees/linked\n",
            )
            .unwrap();
            for agent in ["codex", "opencode"] {
                let mut executor = FakeExecutor::launch();
                let mut list = worktree_list();
                // Observed Herdr 0.9.1 behavior: source remains MAIN, not LINKED.
                list["result"]["worktrees"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                        "path": linked.0, "branch": "linked", "open_workspace_id": "wLinked"
                    }));
                list["result"]["focused_workspace_id"] = json!("wFocused");
                executor.outputs[0] = Ok(list.to_string().into_bytes());
                let mut request = request();
                request.agent = agent;
                let error = launch_with(&mut executor, &linked.0, &request)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains("Launch from the main checkout"), "{error}");
                assert!(error.contains(cwd().to_str().unwrap()));
                assert!(executor.mutations().is_empty());
                assert!(!executor.calls.iter().any(|args| args[0] == "git_identity"));
            }
        }
    }

    fn metadata_output(directory: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
        let output = match args {
            ["rev-parse", "--is-bare-repository"] => return Ok(b"false\n".to_vec()),
            ["rev-parse", "--show-toplevel"] => directory.to_path_buf(),
            ["rev-parse", "--path-format=absolute", "--git-common-dir"]
            | ["rev-parse", "--absolute-git-dir"] => cwd().join(".git"),
            _ => panic!("unexpected Git argv {args:?}"),
        };
        Ok(format!("{}\n", output.to_str().unwrap()).into_bytes())
    }

    #[test]
    fn startup_trust_uses_real_main_repository_for_main_and_linked_checkouts() {
        let fixture = MainCheckout::new();
        let git = |args: &[&std::ffi::OsStr]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(&fixture.0)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&[
            "-c".as_ref(),
            "user.name=Test".as_ref(),
            "-c".as_ref(),
            "user.email=test@example.invalid".as_ref(),
            "commit".as_ref(),
            "--quiet".as_ref(),
            "--allow-empty".as_ref(),
            "--no-verify".as_ref(),
            "-m".as_ref(),
            "fixture".as_ref(),
        ]);
        let linked = fixture.0.join("target/linked checkout");
        git(&[
            "worktree".as_ref(),
            "add".as_ref(),
            "--quiet".as_ref(),
            "--detach".as_ref(),
            linked.as_os_str(),
        ]);
        let expected = codex_trust_override(&fixture.0.canonicalize().unwrap()).unwrap();
        assert_eq!(codex_startup_trust(&fixture.0).unwrap(), expected);
        assert_eq!(codex_startup_trust(&linked).unwrap(), expected);
        assert!(codex_startup_trust(&fixture.0.join("src")).is_err());
    }

    #[test]
    fn main_and_linked_git_metadata_resolve_same_verified_main_root() {
        // The synthetic linked checkout uses an existing directory and fake Git
        // responses; no real worktree, subprocess, config, or environment change.
        for checkout in [cwd(), cwd().join("src")] {
            let mut calls = Vec::new();
            let actual = repository_identity_with(&checkout, |directory, args| {
                calls.push((
                    directory.to_path_buf(),
                    args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                ));
                metadata_output(directory, args)
            })
            .unwrap();
            assert_eq!(actual, identity());
            assert_eq!(calls[0].0, checkout);
            assert_eq!(
                calls[0].1,
                ["rev-parse", "--path-format=absolute", "--git-common-dir"]
            );
            assert!(calls.iter().any(|(directory, args)| directory == &cwd()
                && args == &["rev-parse", "--absolute-git-dir"]));
        }
    }

    #[test]
    fn ambiguous_nonstandard_or_inconsistent_git_metadata_is_rejected() {
        for mode in [
            "relative",
            "missing",
            "nonstandard",
            "bare",
            "checkout",
            "main_root",
            "main_common",
            "main_git",
        ] {
            let linked = cwd().join("src");
            let result = repository_identity_with(&linked, |directory, args| {
                let is_main = directory == cwd();
                let wrong_path = format!("{}\n", cwd().join("src").display()).into_bytes();
                match (mode, args) {
                    ("relative", ["rev-parse", "--path-format=absolute", "--git-common-dir"]) => {
                        Ok(b".git\n".to_vec())
                    }
                    ("missing", _) => Err(invalid("metadata missing")),
                    (
                        "nonstandard",
                        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
                    ) => Ok(wrong_path),
                    ("bare", ["rev-parse", "--is-bare-repository"]) => Ok(b"true\n".to_vec()),
                    ("checkout", ["rev-parse", "--show-toplevel"]) if !is_main => {
                        Ok(format!("{}\n", cwd().display()).into_bytes())
                    }
                    ("main_root", ["rev-parse", "--show-toplevel"]) if is_main => Ok(wrong_path),
                    (
                        "main_common",
                        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
                    ) if is_main => Ok(wrong_path),
                    ("main_git", ["rev-parse", "--absolute-git-dir"]) => Ok(wrong_path),
                    _ => metadata_output(directory, args),
                }
            });
            assert!(result.is_err(), "{mode}");
        }
    }

    #[test]
    fn toml_trust_key_quotes_arbitrary_paths_without_shell_or_table_injection() {
        let root = "/tmp/雪 🦀 \".trust_level=\"untrusted\" # \\ $();\n\r\t\u{0}\u{b}\u{7f}\u{85}";
        let option = codex_trust_override(Path::new(root)).unwrap();
        let quoted = option
            .strip_prefix("projects={")
            .unwrap()
            .strip_suffix("={trust_level=\"trusted\"}}")
            .unwrap();
        assert_eq!(serde_json::from_str::<String>(quoted).unwrap(), root);
        assert!(quoted.starts_with('"') && quoted.ends_with('"'));
        assert!(!option.chars().any(char::is_control));
        assert!(option.contains("\\\""));
        assert!(option.contains("\\\\"));
        assert!(option.contains("雪 🦀"));
        assert!(option.contains("\\u007f"));
        assert!(option.contains("\\u0085"));
        assert!(codex_trust_override(Path::new("relative/path")).is_err());
    }

    #[test]
    fn codex_trust_path_is_in_toml_value_instead_of_dotted_override_key() {
        for root in ["/tmp/todo", "/tmp/project.with.dots", "/tmp/key=value"] {
            let option = codex_trust_override(Path::new(root)).unwrap();
            // Match Codex's key/value split: only the value is parsed as TOML.
            let (key, value) = option.split_once('=').unwrap();
            assert_eq!(key, "projects");
            assert_eq!(value, format!("{{\"{root}\"={{trust_level=\"trusted\"}}}}"));
        }
    }

    #[test]
    fn quoted_trust_path_is_one_forwarded_argument_not_shell_syntax() {
        let mut executor = FakeExecutor::launch();
        let main_root = PathBuf::from("/repos/雪 \";$(not-a-command)\\folder\n\u{7f}");
        let identity = RepositoryIdentity {
            common_dir: main_root.join(".git"),
            main_root: main_root.clone(),
        };
        executor.identities = [Ok(identity.clone()), Ok(identity)].into();
        run_launch(&mut executor).unwrap();
        let mutations = executor.mutations();
        assert_eq!(
            &mutations[1][9..],
            [
                "--",
                "-c",
                &codex_trust_override(&main_root).unwrap(),
                "-c",
                "check_for_update_on_startup=false",
                "--dangerously-bypass-hook-trust"
            ]
        );
        assert_eq!(
            mutations[1].len(),
            15,
            "No positional prompt or extra parsed options"
        );
        assert_eq!(&mutations[2][..3], ["agent", "prompt", "wNew:p1"]);
    }

    #[test]
    fn failed_or_malformed_bead_capture_stops_before_herdr() {
        for bead in [
            Err(io::Error::other("bd unavailable")),
            Ok(json!({"id": "other"})),
            Ok(Value::Null),
        ] {
            let mut executor = FakeExecutor {
                bead: Some(bead),
                ..FakeExecutor::default()
            };
            let error = run_launch(&mut executor).unwrap_err().to_string();
            assert!(error.contains("Bead capture"));
            assert_eq!(executor.calls.len(), 1);
            assert!(executor.mutations().is_empty());
        }
    }

    #[test]
    fn revalidates_workspace_agent_and_branch_without_silent_reuse() {
        for mode in ["workspace", "agent", "branch", "name", "overlong_name"] {
            let mut executor = FakeExecutor::launch();
            let mut request = request();
            match mode {
                "workspace" => request.workspace_id = "wOther",
                "agent" => request.agent = "--evil",
                "name" => request.name = "../../unsafe",
                "overlong_name" => request.name = "abcdefghijklmnopqrstuvwxyz1234567",
                "branch" => {
                    let mut value = worktree_list();
                    value["result"]["worktrees"][0]["branch"] = json!(BRANCH);
                    executor.outputs[0] = Ok(value.to_string().into_bytes());
                }
                _ => unreachable!(),
            }
            let error = launch_with(&mut executor, &cwd(), &request)
                .unwrap_err()
                .to_string();
            assert!(error.contains("Launch validation"), "{mode}: {error}");
            assert!(executor.mutations().is_empty());
        }
    }

    #[test]
    fn failed_create_stops_and_reports_uncertain_recovery_without_deletion() {
        let mut executor = FakeExecutor::launch();
        executor.outputs[3] = Err(io::Error::other("untrusted repository"));
        let error = run_launch(&mut executor).unwrap_err().to_string();
        assert_eq!(executor.mutations().len(), 1);
        for expected in [
            "Worktree creation failed",
            "partially succeeded",
            "source workspace wY",
            "root pane unknown",
            BRANCH,
            "Nothing was automatically deleted",
        ] {
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn malformed_or_inconsistent_creation_never_starts_an_agent() {
        for mode in [
            "json",
            "type",
            "workspace",
            "pane",
            "tab",
            "branch",
            "path",
            "reused",
        ] {
            let mut executor = FakeExecutor::launch();
            let mut value = created();
            match mode {
                "type" => value["result"]["type"] = json!("pane_created"),
                "workspace" => value["result"]["tab"]["workspace_id"] = json!("wOther"),
                "pane" => {
                    value["result"].as_object_mut().unwrap().remove("root_pane");
                }
                "tab" => value["result"]["root_pane"]["tab_id"] = json!("wrong"),
                "branch" => value["result"]["worktree"]["branch"] = json!("main"),
                "path" => value["result"]["worktree"]["path"] = json!("relative"),
                "reused" => value["result"]["workspace"]["workspace_id"] = json!("wY"),
                _ => {}
            }
            executor.outputs[3] = Ok(if mode == "json" {
                b"bad json".to_vec()
            } else {
                value.to_string().into_bytes()
            });
            let error = run_launch(&mut executor).unwrap_err().to_string();
            assert_eq!(executor.mutations().len(), 1, "{mode}");
            assert!(error.contains("Worktree creation response"), "{error}");
            assert!(error.contains("Recovery:"));
            assert!(error.contains(BRANCH));
        }
    }

    #[test]
    fn start_failure_preserves_recovery_ids_and_never_sends_prompt() {
        for malformed in [false, true] {
            let mut executor = FakeExecutor::launch();
            executor.outputs[4] = if malformed {
                Ok(agent_result("unexpected").to_string().into_bytes())
            } else {
                Err(io::Error::other("not ready"))
            };
            let error = run_launch(&mut executor).unwrap_err().to_string();
            assert_eq!(executor.mutations().len(), 2);
            for expected in [
                "Agent start failed",
                "no prompt was sent",
                "new workspace wNew",
                "root pane wNew:p1",
                BRANCH,
            ] {
                assert!(error.contains(expected), "{error}");
            }
        }
    }

    #[test]
    fn prompt_failure_or_wrong_pane_reports_possible_delivery_without_retry() {
        for mode in ["failure", "pane", "workspace", "json"] {
            let mut executor = FakeExecutor::launch();
            let mut response = agent_result("agent_prompted");
            if mode == "pane" {
                response["result"]["agent"]["pane_id"] = json!("wrong");
            }
            if mode == "workspace" {
                response["result"]["agent"]["workspace_id"] = json!("wrong");
            }
            executor.outputs[5] = match mode {
                "failure" => Err(io::Error::other("submission failed")),
                "json" => Ok(b"bad JSON".to_vec()),
                _ => Ok(response.to_string().into_bytes()),
            };
            let error = run_launch(&mut executor).unwrap_err().to_string();
            assert_eq!(executor.mutations().len(), 3);
            for expected in [
                "Prompt submission failed",
                "may already have been delivered",
                "do not resubmit blindly",
                "wNew:p1",
                "wNew",
                BRANCH,
            ] {
                assert!(error.contains(expected), "{error}");
            }
        }
    }

    #[test]
    fn generated_names_are_safe_bounded_and_distinct_even_for_arbitrary_issue_ids() {
        let mut names = HashSet::new();
        for id in [
            "OnTrack-e30",
            "--../../BAD bead;$(exec)🦀",
            "",
            &"A".repeat(1000),
        ] {
            for _ in 0..100 {
                let name = unique_name(id);
                assert!(safe_name(&name));
                assert!(name.len() <= 32, "{name}");
                assert!(!name.contains('.'));
                assert!(names.insert(name));
            }
        }
        assert!(unique_name("OnTrack-e30").starts_with("ot-tracke30-"));
        assert!(safe_name(&"a".repeat(32)));
        assert!(!safe_name(&"a".repeat(33)));
        assert!(!safe_name("1starts-with-digit"));
        assert!(!safe_name("Uppercase"));
    }
}
