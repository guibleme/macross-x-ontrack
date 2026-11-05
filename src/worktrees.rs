//! Only explicitly journaled OnTrack handoffs are managed here. Beads remain independent.
//!
//! The advisory lock serializes OnTrack processes, not arbitrary external Git clients.
//! Every mutation rechecks Git and uses ff-only; no reset, force, or branch deletion.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use crate::herdr::{self, Executor, RepositoryIdentity, SystemExecutor, envelope};

#[derive(Debug, Clone)]
pub struct ManagedWorktree {
    pub id: String,
    pub bead_id: String,
    pub parent_bead_id: Option<String>,
    /// Herdr accepted the initial prompt and the active phase was persisted.
    pub handoff_accepted: bool,
    pub bead_title: Option<String>,
    pub bead_description: Option<String>,
    pub branch: String,
    pub target_branch: String,
    pub workspace_id: String,
    pub status: WorktreeStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentDisplay {
    pub id: String,
    pub workspace_id: String,
    pub detail: String,
    /// Verified Herdr state only; never used by merge or cleanup decisions.
    pub blocked: bool,
}

/// Read-only presentation evidence, never passed to merge/cleanup eligibility.
pub(crate) fn observe_agents() -> io::Result<Vec<AgentDisplay>> {
    observe_agents_with(&std::env::current_dir()?, &mut SystemExecutor)
}

fn observe_agents_with(cwd: &Path, executor: &mut impl Executor) -> io::Result<Vec<AgentDisplay>> {
    let registry = LaunchJournal::open(cwd)?;
    let entries = registry.entries.clone();
    drop(registry);
    Ok(entries.iter().filter(|entry| entry["workspace_closed"] != true && entry["checkout_removed"] != true && entry["phase"] != "reserved").map(|entry| {
        let identity = format!("workspace {} · pane {} · branch {}", entry["workspace_id"].as_str().unwrap_or("unknown"), entry["pane_id"].as_str().unwrap_or("unknown"), entry["branch"].as_str().unwrap_or("unknown"));
        let status = observe_entry(entry, executor).unwrap_or_else(|error| format!("unknown — {error}"));
        AgentDisplay {
            id: entry["id"].as_str().unwrap_or_default().into(),
            workspace_id: entry["workspace_id"].as_str().unwrap_or_default().into(),
            detail: format!("Agent observation (display only): {status} · {identity}. Not completion, retry, merge, or cleanup authorization."),
            blocked: matches!(status.as_str(), "blocked" | "error"),
        }
    }).collect())
}

fn observe_entry(entry: &Value, executor: &mut impl Executor) -> io::Result<String> {
    let pane = field(entry, "pane_id")?;
    if pane.starts_with('-')
        || !pane
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_-".contains(&byte))
    {
        return Err(error("Unsafe recorded pane identity"));
    }
    let result = envelope(&executor.run(&["agent", "get", pane])?)?;
    let agent = &result["agent"];
    if result["type"] != "agent_info"
        || agent["pane_id"] != entry["pane_id"]
        || agent["workspace_id"] != entry["workspace_id"]
        || agent["agent"] != entry["agent"]
    {
        return Err(error("Agent pane/workspace/kind identity changed"));
    }
    if !entry["agent_session"].is_object() || agent["agent_session"] != entry["agent_session"] {
        return Err(error("Recorded native agent session is missing or changed"));
    }
    if entry["initial_terminal_id"].is_string()
        && agent["terminal_id"] != entry["initial_terminal_id"]
    {
        return Err(error("Recorded terminal identity changed"));
    }
    Ok(match agent["agent_status"].as_str() {
        Some(status @ ("working" | "blocked" | "idle" | "done" | "error")) => status.to_owned(),
        _ => "unknown".into(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeStatus {
    Reserved,
    InProgress,
    Blocked,
    Unmerged,
    MergeQueued,
    MergeRequested,
    MergeInProgress,
    NeedsResolution,
    Integrated,
    WorkspaceClosed,
    CheckoutRemoved,
    DirtyCheckout,
    NeedsAttention,
}

/// An agent's advisory activity report is scoped to the current managed checkout.
/// It never participates in merge, cleanup, or Bead decisions.
pub fn report_activity(cwd: &Path, state: &str, detail: Option<&str>) -> io::Result<()> {
    if !matches!(state, "working" | "merging" | "blocked" | "clear") {
        return Err(error("Status must be working, merging, blocked, or clear"));
    }
    if state == "clear" && detail.is_some() {
        return Err(error("A cleared status cannot have a detail"));
    }
    if state == "blocked" && detail.is_none() {
        return Err(error("A blocked status requires a brief reason"));
    }
    if detail.is_some_and(|text| {
        text.len() > 200 || text.trim().is_empty() || text.chars().any(char::is_control)
    }) {
        return Err(error("Status detail must be 1-200 characters on one line"));
    }
    let root = PathBuf::from(git(cwd, &["rev-parse", "--show-toplevel"])?).canonicalize()?;
    let repo = SystemExecutor.git_identity(&root)?;
    if !repo
        .common_dir
        .join("ontrack-worktrees/registry.json")
        .is_file()
    {
        return Err(error("Current checkout has no managed OnTrack handoff"));
    }
    let mut registry = LaunchJournal::open(cwd)?;
    let matching: Vec<_> = registry
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry["path"].as_str() == root.to_str())
        .map(|(index, _)| index)
        .collect();
    if matching.len() != 1 {
        return Err(error(
            "Current checkout is not exactly one managed OnTrack handoff",
        ));
    }
    let index = matching[0];
    let entry = &registry.entries[index];
    if entry["phase"] != "active"
        || entry["checkout_removed"] == true
        || entry["workspace_closed"] == true
        || entry["restarted"] == true
    {
        return Err(error("Managed handoff is not active"));
    }
    verify_identity(&registry.repo, entry)?;
    registry.entries[index]["agent_report"] = if state == "clear" {
        Value::Null
    } else {
        json!({
            "state": state,
            "detail": detail,
            "reported_at": SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_secs(),
        })
    };
    registry.save()
}

#[derive(Debug, Clone, Copy)]
pub enum Action {
    RequestMerge,
    CloseWorkspace,
    RemoveCheckout,
    ForceRemoveCheckout,
}

#[derive(Debug, Clone, Copy)]
pub struct CleanupPolicy {
    pub auto_merge: bool,
    pub auto_merge_children: bool,
    pub auto_close_workspace: bool,
    pub auto_remove_checkout: bool,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            auto_merge: false,
            auto_merge_children: true,
            auto_close_workspace: false,
            auto_remove_checkout: false,
        }
    }
}

fn error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

fn git_output(path: &Path, args: &[&str]) -> io::Result<Output> {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
}

fn git(path: &Path, args: &[&str]) -> io::Result<String> {
    let output = git_output(path, args)?;
    if !output.status.success() {
        return Err(error(format!(
            "Git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.strip_suffix('\n').unwrap_or(&s).to_owned())
        .map_err(|_| error("Git output must be UTF-8"))
}

fn branch(path: &Path) -> io::Result<String> {
    git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])
}

fn tip(path: &Path, reference: &str) -> io::Result<String> {
    git(
        path,
        &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
    )
}

fn ancestor(path: &Path, source: &str, target: &str) -> io::Result<bool> {
    let output = git_output(path, &["merge-base", "--is-ancestor", source, target])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(error(format!(
            "Cannot verify ancestry: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

fn dirty(path: &Path) -> io::Result<bool> {
    Ok(!git(path, &["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty())
}

fn unfinished_operation(path: &Path) -> io::Result<()> {
    // A clean-looking index can still be in the middle of an operation.
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let metadata = git(
            path,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        )?;
        if Path::new(&metadata).exists() {
            return Err(error(format!(
                "Unfinished Git operation {name} in {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn merge_head_exists(path: &Path) -> io::Result<bool> {
    let metadata = git(
        path,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "MERGE_HEAD",
        ],
    )?;
    Ok(Path::new(&metadata).exists())
}

fn clean(path: &Path) -> io::Result<()> {
    if dirty(path)? {
        return Err(error(format!(
            "Dirty checkout {}; commit/resolve changes first",
            path.display()
        )));
    }
    unfinished_operation(path)
}

/// Remote symbolic HEADs must agree. Without one, only the verified primary
/// checkout's actual branch is evidence; init.defaultBranch is never consulted.
fn default_branch(repo: &RepositoryIdentity) -> io::Result<String> {
    let refs = git(
        &repo.main_root,
        &[
            "for-each-ref",
            "--format=%(refname) %(symref)",
            "refs/remotes/",
        ],
    )?;
    let mut defaults = std::collections::BTreeSet::new();
    // for-each-ref omits dangling symbolic HEADs: do not silently fall back
    // when a configured remote has a broken default ref.
    for remote in git(&repo.main_root, &["remote"])?.lines() {
        let reference = format!("refs/remotes/{remote}/HEAD");
        let output = git_output(&repo.main_root, &["symbolic-ref", "--quiet", &reference])?;
        if output.status.success() {
            let target =
                String::from_utf8(output.stdout).map_err(|_| error("Non UTF-8 remote HEAD"))?;
            let target = target.trim_end_matches('\n');
            let prefix = format!("refs/remotes/{remote}/");
            let candidate = target
                .strip_prefix(&prefix)
                .ok_or_else(|| error("Remote HEAD points outside its remote"))?;
            tip(&repo.main_root, target)?;
            defaults.insert(candidate.to_owned());
        } else if output.status.code() != Some(1) {
            return Err(error(format!(
                "Cannot inspect remote symbolic HEAD {reference}"
            )));
        }
    }
    for line in refs.lines() {
        let Some((name, target)) = line.split_once(' ') else {
            continue;
        };
        if name.ends_with("/HEAD") && !target.is_empty() {
            let prefix = name
                .strip_suffix("HEAD")
                .ok_or_else(|| error("Invalid remote HEAD"))?;
            let candidate = target
                .strip_prefix(prefix)
                .ok_or_else(|| error("Remote HEAD points outside its remote"))?;
            tip(&repo.main_root, target)?;
            defaults.insert(candidate.to_owned());
        }
    }
    if defaults.len() > 1 {
        return Err(error(
            "Remote symbolic HEADs disagree; default branch is ambiguous",
        ));
    }
    let target = match defaults.into_iter().next() {
        Some(target) => target,
        None => branch(&repo.main_root).map_err(|_| error("No remote symbolic HEAD and primary checkout is detached; cannot determine default branch"))?,
    };
    if target.starts_with("ontrack/") || target.is_empty() {
        return Err(error(
            "Primary checkout is a task branch; cannot establish a safe default branch",
        ));
    }
    tip(&repo.main_root, &format!("refs/heads/{target}"))?;
    if branch(&repo.main_root)? != target {
        return Err(error(format!(
            "Verified primary checkout must be on default branch {target}"
        )));
    }
    Ok(target)
}

/// Registry entries deliberately retain raw responses, including malformed or
/// mismatched create results, so partial launches can be recovered by a human.
struct Registry {
    repo: RepositoryIdentity,
    directory: PathBuf,
    _lock: File,
    entries: Vec<Value>,
}

pub(crate) fn with_existing_registry_lock<T>(
    cwd: &Path,
    action: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    let output = match git_output(
        cwd,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    ) {
        Ok(output) if output.status.success() => output,
        Ok(_) => return action(), // Beads-only workspaces need not be Git checkouts.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return action(),
        Err(error) => return Err(error),
    };
    let common = String::from_utf8(output.stdout).map_err(|_| error("Git output must be UTF-8"))?;
    let directory =
        Path::new(common.strip_suffix('\n').unwrap_or(&common)).join("ontrack-worktrees");
    match fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return action(),
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    // Hold only the shared lock, never a registry snapshot that could overwrite a journal update.
    let _lock = registry_lock(&directory)?;
    action()
}

fn registry_lock(directory: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(directory)?.file_type().is_dir() {
        return Err(error("OnTrack registry directory must not be a symlink"));
    }
    let lock_path = directory.join("registry.lock");
    reject_symlink(&lock_path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    // std::fs::File::try_lock requires Rust 1.89; keep the project's 1.88 MSRV.
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|e| io::Error::new(e.kind(), format!("OnTrack worktree registry is busy: {e}")))?;
    Ok(lock)
}

impl Registry {
    fn open(cwd: &Path) -> io::Result<Self> {
        let root = git(cwd, &["rev-parse", "--show-toplevel"])?;
        let repo = SystemExecutor.git_identity(Path::new(&root))?;
        let directory = repo.common_dir.join("ontrack-worktrees");
        match fs::create_dir(&directory) {
            Ok(()) => {
                File::open(&repo.common_dir)?.sync_all()?;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let lock = registry_lock(&directory)?;
        let path = directory.join("registry.json");
        reject_symlink(&path)?;
        let entries = match fs::read(&path) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes).map_err(|e| {
                    error(format!("Invalid OnTrack registry (not overwritten): {e}"))
                })?;
                if value["version"] != 1 || value["common_dir"].as_str() != repo.common_dir.to_str()
                {
                    return Err(error(
                        "Unsupported/mismatched OnTrack registry; refusing changes",
                    ));
                }
                let entries = value["entries"]
                    .as_array()
                    .ok_or_else(|| error("Invalid registry entries"))?
                    .clone();
                let mut ids = std::collections::HashSet::new();
                for entry in &entries {
                    for key in [
                        "id",
                        "bead_id",
                        "branch",
                        "target_branch",
                        "common_dir",
                        "main_root",
                        "phase",
                        "base",
                        "agent",
                        "source_workspace",
                    ] {
                        field(entry, key)?;
                    }
                    if !ids.insert(field(entry, "id")?.to_owned())
                        || entry["common_dir"].as_str() != repo.common_dir.to_str()
                        || entry["main_root"].as_str() != repo.main_root.to_str()
                    {
                        return Err(error("Invalid/duplicate registry identity"));
                    }
                }
                entries
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            repo,
            directory,
            _lock: lock,
            entries,
        })
    }

    fn save(&self) -> io::Result<()> {
        let path = self.directory.join("registry.json");
        reject_symlink(&path)?;
        // A stable temporary name is safe under the separate, never-renamed lock.
        let temporary = self.directory.join("registry.json.tmp");
        reject_symlink(&temporary)?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        serde_json::to_writer_pretty(
            &mut file,
            &json!({"version":1, "common_dir":self.repo.common_dir, "entries":self.entries}),
        )?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(temporary, path)?;
        File::open(&self.directory)?.sync_all()
    }

    fn index(&self, id: &str) -> io::Result<usize> {
        self.entries
            .iter()
            .position(|e| e["id"] == id)
            .ok_or_else(|| {
                error("Unknown managed handoff; existing worktrees are never adopted automatically")
            })
    }
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(error(format!(
            "Refusing registry symlink {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn field<'a>(entry: &'a Value, key: &str) -> io::Result<&'a str> {
    entry[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            error(format!(
                "Managed handoff has missing {key}; inspect registry before recovery"
            ))
        })
}

pub(crate) struct LaunchJournal {
    checkout: PathBuf,
    repo: RepositoryIdentity,
    intent: Value,
    claim: String,
}

impl LaunchJournal {
    fn open(checkout: &Path) -> io::Result<Registry> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match Registry::open(checkout) {
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                result => return result,
            }
        }
    }

    fn handle(checkout: &Path, registry: &Registry, intent: Value) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self {
            checkout: checkout.to_path_buf(),
            repo: registry.repo.clone(),
            intent,
            claim: format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }

    fn transaction<T>(&self, update: impl FnOnce(&mut Value) -> io::Result<T>) -> io::Result<T> {
        let mut registry = Self::open(&self.checkout)?;
        if registry.repo != self.repo {
            return Err(error("Launch journal repository identity changed"));
        }
        let index = registry.index(field(&self.intent, "id")?)?;
        let entry = &mut registry.entries[index];
        for key in [
            "id",
            "bead_id",
            "branch",
            "target_branch",
            "target_path",
            "parent_bead_id",
            "common_dir",
            "main_root",
            "source_workspace",
            "agent",
            "base",
        ] {
            if entry[key] != self.intent[key] {
                return Err(error(format!("Launch journal intent changed: {key}")));
            }
        }
        if entry["launch_owner"] != self.intent["launch_owner"]
            && entry["launch_owner"].as_str() != Some(&self.claim)
        {
            return Err(error(
                "Reserved parent launch has been claimed by another handoff",
            ));
        }
        let result = update(entry)?;
        registry.save()?;
        Ok(result)
    }
    pub(crate) fn begin(
        checkout: &Path,
        bead: &Value,
        launch_branch: &str,
        agent: &str,
        name: &str,
        workspace: &str,
    ) -> io::Result<Self> {
        let bead_id = field(bead, "id")?;
        let checkout = checkout.canonicalize()?;
        let registry = Self::open(&checkout)?;
        if let Some(index) = registry
            .entries
            .iter()
            .position(|entry| entry["bead_id"] == bead_id && entry["phase"] == "reserved")
        {
            let entry = &registry.entries[index];
            if field(entry, "id")? != name
                || field(entry, "branch")? != launch_branch
                || field(entry, "source_workspace")? != workspace
            {
                return Err(error("Reserved parent handoff identity changed"));
            }
            let journal = Self::handle(&checkout, &registry, entry.clone());
            drop(registry);
            // A snapshot is not authority to start: phase() atomically claims
            // the still-reserved entry immediately before the start command.
            verify(&journal.repo, &journal.intent)?;
            return Ok(journal);
        }
        if registry
            .entries
            .iter()
            .any(|entry| entry["bead_id"] == bead_id && entry["checkout_removed"] != true)
        {
            return Err(error("An OnTrack worktree already exists for this task"));
        }
        let repo = registry.repo.clone();
        let entries = registry.entries.clone();
        drop(registry);
        let default = default_branch(&repo)?;
        clean(&repo.main_root)?;
        let parent_bead_id = bead["parent"].as_str();
        let (target, target_path, base) = if let Some(parent_id) = parent_bead_id {
            let parents: Vec<_> = entries
                .iter()
                .filter(|entry| {
                    entry["bead_id"] == parent_id
                        && matches!(entry["phase"].as_str(), Some("active" | "reserved"))
                        && entry["checkout_removed"] != true
                        && entry["workspace_closed"] != true
                })
                .collect();
            if parents.len() != 1 {
                return Err(error(
                    "Child handoff requires one active, open OnTrack-managed parent worktree",
                ));
            }
            let parent = parents[0];
            let path = Path::new(field(parent, "path")?).canonicalize()?;
            if path.to_str() != parent["path"].as_str()
                || SystemExecutor.git_identity(&path)? != repo
                || branch(&path)? != field(parent, "branch")?
            {
                return Err(error(
                    "Parent worktree identity or branch changed; child launch refused",
                ));
            }
            clean(&path)?;
            let base = tip(&path, "HEAD")?;
            (field(parent, "branch")?.to_owned(), Some(path), base)
        } else {
            (default, None, tip(&repo.main_root, "HEAD")?)
        };
        if entries
            .iter()
            .any(|e| e["id"] == name || e["branch"] == launch_branch)
        {
            return Err(error(
                "Launch intent already recorded; inspect it instead of retrying creation",
            ));
        }
        // Also catch branches without an open Herdr workspace.
        if git_output(
            &repo.main_root,
            &[
                "show-ref",
                "--verify",
                &format!("refs/heads/{launch_branch}"),
            ],
        )?
        .status
        .success()
        {
            return Err(error("Launch branch already exists; refusing reuse"));
        }
        // Reload after all Git verification; the short write transaction checks
        // competing per-Bead intents and the parent snapshot before saving.
        let mut registry = Self::open(&checkout)?;
        if registry.repo != repo
            || registry.entries.iter().any(|entry| {
                (entry["bead_id"] == bead_id && entry["checkout_removed"] != true)
                    || entry["id"] == name
                    || entry["branch"] == launch_branch
            })
        {
            return Err(error(
                "An OnTrack worktree/launch intent already exists for this task or branch",
            ));
        }
        if let Some(parent_id) = parent_bead_id {
            let previous = entries
                .iter()
                .find(|entry| entry["bead_id"] == parent_id && entry["checkout_removed"] != true);
            let current = registry
                .entries
                .iter()
                .find(|entry| entry["bead_id"] == parent_id && entry["checkout_removed"] != true);
            for key in [
                "id",
                "branch",
                "path",
                "phase",
                "workspace_closed",
                "checkout_removed",
            ] {
                if previous.map(|entry| &entry[key]) != current.map(|entry| &entry[key]) {
                    return Err(error(
                        "Parent handoff changed during child launch validation",
                    ));
                }
            }
        }
        let index = registry.entries.len();
        registry.entries.push(json!({
            "id":name, "bead_id":bead_id,
            "bead_title":bead["title"].as_str(),
            "bead_description":bead["description"].as_str(),
            "branch":launch_branch, "target_branch":target,
            "target_path":target_path, "parent_bead_id":parent_bead_id,
            "common_dir":registry.repo.common_dir, "main_root":registry.repo.main_root,
            "source_workspace":workspace, "agent":agent, "base":base,
            "phase":"create_possibly_sent", "responses":{},
            "workspace_closed":false, "checkout_removed":false,
            "detail":"Launch intent persisted before Herdr creation. If interrupted, inspect the raw response and Herdr/Git before recovery; never blindly relaunch."
        }));
        let journal = Self::handle(&checkout, &registry, registry.entries[index].clone());
        registry.entries[index]["launch_owner"] = json!(journal.claim);
        registry.save()?;
        Ok(Self {
            intent: registry.entries[index].clone(),
            ..journal
        })
    }

    pub(crate) fn capture(&mut self, kind: &str, bytes: &[u8]) -> io::Result<()> {
        self.transaction(|entry| {
            entry["responses"][kind] = json!(String::from_utf8_lossy(bytes));
            if let Ok(result) = envelope(bytes) {
                if result["agent"]["agent_session"].is_object() {
                    entry["agent_session"] = result["agent"]["agent_session"].clone();
                }
                if kind == "ready" {
                    if result["agent"]["state_change_seq"].is_u64() {
                        entry["initial_lifecycle_seq"] =
                            result["agent"]["state_change_seq"].clone();
                    }
                    if result["agent"]["terminal_id"]
                        .as_str()
                        .is_some_and(|id| !id.is_empty())
                    {
                        entry["initial_terminal_id"] = result["agent"]["terminal_id"].clone();
                    }
                }
            }
            Ok(())
        })
    }

    pub(crate) fn created(&mut self, result: &Value) -> io::Result<()> {
        // Persist received handles BEFORE filesystem identity checks or agent start.
        let entry = self.transaction(|entry| {
            entry["workspace_id"] = result["workspace"]["workspace_id"].clone();
            entry["pane_id"] = result["root_pane"]["pane_id"].clone();
            entry["path"] = result["worktree"]["path"].clone();
            entry["phase"] = json!("created");
            Ok(entry.clone())
        })?;
        let source = Path::new(field(&entry, "path")?).canonicalize()?;
        if source == self.repo.main_root || source.to_str() != entry["path"].as_str() {
            return Err(error(
                "Created checkout path is not a distinct canonical path",
            ));
        }
        let identity = SystemExecutor.git_identity(&source)?;
        if identity != self.repo
            || branch(&source)? != field(&entry, "branch")?
            || tip(&source, "HEAD")? != field(&entry, "base")?
        {
            return Err(error(
                "Created checkout identity, branch, or launch base does not match intent; no agent started",
            ));
        }
        clean(&source)?;
        self.transaction(|current| {
            for key in ["workspace_id", "pane_id", "path", "phase"] {
                if current[key] != entry[key] {
                    return Err(error("Created launch evidence changed during verification"));
                }
            }
            current["phase"] = json!("created_verified");
            Ok(())
        })
    }

    pub(crate) fn phase(&mut self, phase: &str) -> io::Result<()> {
        self.transaction(|entry| {
            if self.intent["phase"] == "reserved" && phase == "agent_start_possibly_sent" {
                if entry["phase"] != "reserved" {
                    return Err(error(
                        "Reserved parent is no longer available for agent start",
                    ));
                }
                for key in ["workspace_id", "pane_id", "path"] {
                    if entry[key] != self.intent[key] {
                        return Err(error("Reserved parent target changed before start"));
                    }
                }
                entry["launch_owner"] = json!(self.claim);
            }
            entry["phase"] = json!(phase);
            if phase == "active" {
                entry["detail"] = Value::Null;
            }
            Ok(())
        })
    }

    pub(crate) fn failed(&mut self, message: &str) -> io::Result<()> {
        self.transaction(|entry| {
            entry["detail"] = json!(message);
            Ok(())
        })
    }

    pub(crate) fn base_branch(&self) -> Option<&str> {
        self.intent["parent_bead_id"]
            .as_str()
            .and_then(|_| self.intent["target_branch"].as_str())
    }

    pub(crate) fn reserved_target(&self) -> Option<(String, String, String, PathBuf)> {
        let entry = &self.intent;
        if entry["phase"] != "reserved" {
            return None;
        }
        Some((
            entry["workspace_id"].as_str()?.to_owned(),
            entry["pane_id"].as_str()?.to_owned(),
            entry["branch"].as_str()?.to_owned(),
            PathBuf::from(entry["path"].as_str()?),
        ))
    }
}

pub fn reserved_name(bead_id: &str) -> io::Result<Option<String>> {
    let registry = LaunchJournal::open(&std::env::current_dir()?)?;
    registry
        .entries
        .iter()
        .find(|entry| entry["bead_id"] == bead_id && entry["phase"] == "reserved")
        .map(|entry| field(entry, "id").map(str::to_owned))
        .transpose()
}

struct Verified {
    source: PathBuf,
    target: PathBuf,
    source_tip: String,
    target_tip: String,
}

fn target_checkout_identity(repo: &RepositoryIdentity, entry: &Value) -> io::Result<PathBuf> {
    let default = default_branch(repo)?;
    let Some(recorded) = entry["target_path"].as_str() else {
        if field(entry, "target_branch")? != default {
            return Err(error(
                "Repository default branch changed; explicit registry recovery required",
            ));
        }
        return Ok(repo.main_root.clone());
    };
    let target = Path::new(recorded).canonicalize()?;
    if target == repo.main_root
        || target.to_str() != Some(recorded)
        || SystemExecutor.git_identity(&target)? != *repo
        || branch(&target)? != field(entry, "target_branch")?
    {
        return Err(error("Parent target checkout identity or branch changed"));
    }
    Ok(target)
}

fn target_checkout(repo: &RepositoryIdentity, entry: &Value) -> io::Result<PathBuf> {
    let target = target_checkout_identity(repo, entry)?;
    clean(&target)?;
    Ok(target)
}

/// A removed child's parent checkout may later be removed too. For historical
/// status only, the retained parent branch still proves integration by ancestry.
/// Live merge and cleanup paths continue to require the verified checkout.
fn removed_checkout_target_tip(repo: &RepositoryIdentity, entry: &Value) -> io::Result<String> {
    if entry["parent_bead_id"].is_string() {
        let recorded = Path::new(field(entry, "target_path")?);
        if !recorded.is_absolute() {
            return Err(error("Recorded parent target path is not absolute"));
        }
        if matches!(fs::symlink_metadata(recorded), Err(e) if e.kind() == io::ErrorKind::NotFound) {
            return tip(
                &repo.main_root,
                &format!("refs/heads/{}", field(entry, "target_branch")?),
            );
        }
    }
    let target = target_checkout_identity(repo, entry)?;
    tip(&target, "HEAD")
}

const NO_TASK_COMMITS: &str = "No task commits beyond the recorded launch base. No-op completion is not supported: inspect the task and keep the checkout; closing a Bead alone never authorizes integration or cleanup.";
const CLOSED_INTEGRATED_DETAIL: &str = "Integration is verified and the Herdr workspace is closed. Use x to remove the clean local checkout; Git keeps the branch.";

fn task_commits(repo: &RepositoryIdentity, entry: &Value, source_tip: &str) -> io::Result<()> {
    let base = field(entry, "base")?;
    if source_tip == base {
        return Err(error(NO_TASK_COMMITS));
    }
    if !ancestor(&repo.main_root, base, source_tip)? {
        return Err(error(
            "Source no longer contains its recorded launch base; inspect rewritten history before integration or cleanup",
        ));
    }
    Ok(())
}

fn requested_merge_tips_in_source(
    repo: &RepositoryIdentity,
    entry: &Value,
    source_tip: &str,
) -> io::Result<()> {
    let request = &entry["merge_request"];
    if !request.is_object() {
        return Ok(());
    }
    for key in ["source_tip", "target_tip"] {
        if !ancestor(&repo.main_root, field(request, key)?, source_tip)? {
            return Err(error(
                "Source no longer contains both commits recorded for the merge request",
            ));
        }
    }
    Ok(())
}

fn verify_identity(repo: &RepositoryIdentity, entry: &Value) -> io::Result<Verified> {
    let actual = SystemExecutor.git_identity(&repo.main_root)?;
    if actual != *repo
        || entry["common_dir"].as_str() != repo.common_dir.to_str()
        || entry["main_root"].as_str() != repo.main_root.to_str()
    {
        return Err(error("Repository identity changed"));
    }
    let target = target_checkout_identity(repo, entry)?;
    let source = Path::new(field(entry, "path")?).canonicalize()?;
    if source.to_str() != entry["path"].as_str()
        || source == repo.main_root
        || source == target
        || SystemExecutor.git_identity(&source)? != *repo
    {
        return Err(error(
            "Source checkout canonical/repository identity changed",
        ));
    }
    if branch(&source)? != field(entry, "branch")? {
        return Err(error("Source checkout branch changed"));
    }
    Ok(Verified {
        source_tip: tip(&source, "HEAD")?,
        target_tip: tip(&target, "HEAD")?,
        source,
        target,
    })
}

fn verify(repo: &RepositoryIdentity, entry: &Value) -> io::Result<Verified> {
    let verified = verify_identity(repo, entry)?;
    clean(&verified.target)?;
    clean(&verified.source)?;
    Ok(verified)
}

fn inspection_dirty(verified: &Verified) -> io::Result<bool> {
    unfinished_operation(&verified.target)?;
    unfinished_operation(&verified.source)?;
    Ok(dirty(&verified.target)? || dirty(&verified.source)?)
}

const DIRTY_CHECKOUT_DETAIL: &str =
    "Uncommitted checkout changes are present; commit or resolve them before merge or cleanup.";

fn view(entry: &Value, status: WorktreeStatus, detail: Option<String>) -> ManagedWorktree {
    let string = |key: &str| entry[key].as_str().unwrap_or_default().to_owned();
    ManagedWorktree {
        id: string("id"),
        bead_id: string("bead_id"),
        parent_bead_id: entry["parent_bead_id"].as_str().map(str::to_owned),
        handoff_accepted: entry["phase"] == "active",
        bead_title: entry["bead_title"].as_str().map(str::to_owned),
        bead_description: entry["bead_description"].as_str().map(str::to_owned),
        branch: string("branch"),
        target_branch: string("target_branch"),
        workspace_id: string("workspace_id"),
        status,
        detail,
    }
}

fn checkout_missing(entry: &Value) -> io::Result<bool> {
    let path = Path::new(field(entry, "path")?);
    if !path.is_absolute() {
        return Err(error("Recorded checkout path is not absolute"));
    }
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}

fn inspect(repo: &RepositoryIdentity, entry: &Value) -> ManagedWorktree {
    use WorktreeStatus::*;
    if entry["continue_prompt_pending"] == true {
        return view(entry, NeedsAttention, Some("Continue prompt may have been delivered. Inspect the recorded agent before any further recovery; OnTrack will not resend it.".into()));
    }
    if entry["restart_pending"] == true {
        return view(entry, NeedsAttention, Some(entry["detail"].as_str().unwrap_or(
            "Restart removal may have succeeded; inspect Git and Herdr before recovery."
        ).to_owned()));
    }
    if entry["restarted"] == true {
        if entry["restart_launch_pending"] == true || entry["restart_launch_error"].is_string() {
            return view(entry, NeedsAttention, Some(entry["restart_launch_error"].as_str().unwrap_or(
                "Old checkout removed; fresh launch may not have completed. Inspect tracked handoffs before recovery."
            ).to_owned()));
        }
        if entry["path"]
            .as_str()
            .is_some_and(|path| Path::new(path).exists())
        {
            return view(
                entry,
                NeedsAttention,
                Some("Restarted checkout path exists again; inspect identity".into()),
            );
        }
        return view(entry, CheckoutRemoved, None);
    }
    if entry["phase"] == "reserved" {
        return match verify_identity(repo, entry).and_then(|verified| inspection_dirty(&verified)) {
            Ok(true) => view(entry, DirtyCheckout, Some(DIRTY_CHECKOUT_DETAIL.into())),
            Ok(false) => view(entry, Reserved, Some("Parent worktree prepared; press h on the parent after its children close and integrate".into())),
            Err(error) => view(entry, NeedsAttention, Some(error.to_string())),
        };
    }
    if entry["phase"] != "active" {
        return view(
            entry,
            NeedsAttention,
            Some(
                entry["detail"]
                    .as_str()
                    .unwrap_or(
                        "Incomplete launch; inspect recorded response/handles. No automatic retry.",
                    )
                    .to_owned(),
            ),
        );
    }
    if entry["checkout_removed"] == true {
        if entry["checkout_removed_externally"] == true || entry["force_removed"] == true {
            return match checkout_missing(entry) {
                Ok(true) => view(entry, CheckoutRemoved, None),
                Ok(false) => view(
                    entry,
                    NeedsAttention,
                    Some("Removed checkout path exists again; identity requires inspection".into()),
                ),
                Err(e) => view(entry, NeedsAttention, Some(e.to_string())),
            };
        }
        let check = (|| {
            // This is historical, read-only status inspection. A dirty target
            // must block new merges and cleanup, but it cannot invalidate an
            // already removed checkout whose branch is contained in the
            // target history.
            let source = tip(
                &repo.main_root,
                &format!("refs/heads/{}", field(entry, "branch")?),
            )?;
            if Path::new(field(entry, "path")?).exists() {
                return Err(error(
                    "Removed checkout path exists again; identity requires inspection",
                ));
            }
            if !ancestor(
                &repo.main_root,
                &source,
                &removed_checkout_target_tip(repo, entry)?,
            )? {
                return Err(error(
                    "Preserved source branch advanced after checkout removal; new work is outstanding",
                ));
            }
            Ok(())
        })();
        return match check {
            Ok(()) => view(
                entry,
                CheckoutRemoved,
                entry["detail"].as_str().map(str::to_owned),
            ),
            Err(e) => view(entry, NeedsAttention, Some(e.to_string())),
        };
    }
    if entry["cleanup_pending"].is_string() {
        return view(entry, NeedsAttention, Some(entry["detail"].as_str().unwrap_or("Cleanup possibly sent; reconciling read-only evidence, never blindly retrying.").to_owned()));
    }
    let verified = match verify_identity(repo, entry) {
        Ok(verified) => verified,
        Err(e) => {
            let conflicts = entry["merge_request"].is_object()
                && entry["path"].as_str().is_some_and(|path| {
                    git(Path::new(path), &["ls-files", "--unmerged"])
                        .is_ok_and(|files| !files.is_empty())
                });
            return view(
                entry,
                if conflicts {
                    NeedsResolution
                } else {
                    NeedsAttention
                },
                Some(e.to_string()),
            );
        }
    };
    if !entry["merge_request"].is_object()
        && clean(&verified.target).is_ok()
        && merge_head_exists(&verified.source).unwrap_or(false)
        && git(&verified.source, &["rev-parse", "MERGE_HEAD"])
            .is_ok_and(|head| head == verified.target_tip)
    {
        let conflicts =
            git(&verified.source, &["ls-files", "--unmerged"]).is_ok_and(|files| !files.is_empty());
        return view(
            entry,
            MergeInProgress,
            Some(if conflicts {
                "Git merge in the source checkout has unresolved conflicts. Resolve and commit them before integration; agent activity is not proof of completion.".into()
            } else {
                "Git merge in the source checkout is unfinished. Commit or abort it before integration; agent activity is not proof of completion.".into()
            }),
        );
    }
    let conflicts = entry["merge_request"].is_object()
        && git(&verified.source, &["ls-files", "--unmerged"]).is_ok_and(|files| !files.is_empty());
    if conflicts {
        return view(
            entry,
            NeedsResolution,
            Some("Unresolved merge conflicts in the source checkout".into()),
        );
    }
    let checkout_dirty = match inspection_dirty(&verified) {
        Ok(dirty) => dirty,
        Err(e) => return view(entry, NeedsAttention, Some(e.to_string())),
    };
    if let Err(e) = task_commits(repo, entry, &verified.source_tip) {
        return view(
            entry,
            if verified.source_tip == entry["base"] {
                if checkout_dirty {
                    DirtyCheckout
                } else {
                    InProgress
                }
            } else {
                NeedsAttention
            },
            Some(if checkout_dirty && verified.source_tip == entry["base"] {
                DIRTY_CHECKOUT_DETAIL.into()
            } else {
                e.to_string()
            }),
        );
    }
    match ancestor(&repo.main_root, &verified.source_tip, &verified.target_tip) {
        Ok(true) => {
            if checkout_dirty {
                return view(entry, DirtyCheckout, Some(DIRTY_CHECKOUT_DETAIL.into()));
            }
            return view(
                entry,
                if entry["workspace_closed"] == true {
                    WorkspaceClosed
                } else {
                    Integrated
                },
                if entry["workspace_closed"] == true {
                    Some(CLOSED_INTEGRATED_DETAIL.to_owned())
                } else {
                    entry["detail"].as_str().map(str::to_owned)
                },
            );
        }
        Err(e) => return view(entry, NeedsAttention, Some(e.to_string())),
        Ok(false) => {}
    }
    if entry["workspace_closed"] == true {
        return view(entry, NeedsAttention, Some("Source advanced after workspace closure; integration is outstanding, reopen the workspace explicitly".into()));
    }
    if entry["merge_request"].is_object() {
        if entry["merge_request"]["target_tip"] != verified.target_tip {
            return view(entry, NeedsResolution, Some("Target advanced since the request; explicitly request another agent resolution. Never automatically resend.".into()));
        }
        let detail = entry["detail"].as_str().unwrap_or("Merge prompt possibly sent; waiting for verified current Git ancestry. Native idle/sequence changes are not proof that this particular prompt completed. Never blindly resend.");
        return view(
            entry,
            MergeRequested,
            Some(if checkout_dirty {
                format!("{detail} {DIRTY_CHECKOUT_DETAIL}")
            } else {
                detail.to_owned()
            }),
        );
    }
    if checkout_dirty {
        return view(entry, DirtyCheckout, Some(DIRTY_CHECKOUT_DETAIL.into()));
    }
    if entry["merge_queue"].is_object() {
        return view(
            entry,
            MergeQueued,
            Some(
                entry["detail"]
                    .as_str()
                    .unwrap_or("Merge queued; waiting for the active integration to finish.")
                    .to_owned(),
            ),
        );
    }
    view(entry, Unmerged, entry["detail"].as_str().map(str::to_owned))
}

/// Ask the original, idle agent to continue in its existing checkout. The
/// durable fence deliberately remains set after any uncertain send result.
pub fn continue_task(id: &str) -> io::Result<String> {
    continue_task_with(&std::env::current_dir()?, id, &mut SystemExecutor)
}

fn continue_task_with(cwd: &Path, id: &str, executor: &mut impl Executor) -> io::Result<String> {
    let mut registry = Registry::open(cwd)?;
    let index = registry.index(id)?;
    let entry = &registry.entries[index];
    if entry["phase"] != "active"
        || entry["checkout_removed"] == true
        || entry["workspace_closed"] == true
        || entry["restarted"] == true
        || entry["restart_pending"] == true
        || entry["cleanup_pending"].is_string()
        || entry["merge_request"].is_object()
        || entry["merge_queue"].is_object()
        || entry["continue_prompt_pending"] == true
    {
        return Err(error(
            "Handoff is not available for a continue prompt; inspect its recovery state",
        ));
    }
    let verified = verify_identity(&registry.repo, entry)?;
    unfinished_operation(&verified.source)?;
    verify_open_workspace(&registry.repo, entry, executor)?;
    let bead_id = field(entry, "bead_id")?.to_owned();
    let bead = executor.show_bead(&bead_id)?;
    if bead["id"] != bead_id || !matches!(bead["status"].as_str(), Some("open" | "in_progress")) {
        return Err(error(
            "Live Bead is missing or no longer open; continue cancelled",
        ));
    }
    // A matching native session is required. A missing or replaced agent can
    // instead use the existing confirmed fresh-start action.
    if !entry["agent_session"].is_object() {
        return Err(error(
            "Original native agent session was not recorded; use fresh start or inspect in Herdr",
        ));
    }
    let pane = field(entry, "pane_id")?.to_owned();
    let workspace = field(entry, "workspace_id")?.to_owned();
    let kind = field(entry, "agent")?.to_owned();
    let ready = envelope(&executor.run(&["agent", "get", &pane])?)?;
    let agent = &ready["agent"];
    if ready["type"] != "agent_info"
        || agent["pane_id"] != pane
        || agent["workspace_id"] != workspace
        || agent["agent"] != kind
        || agent["agent_session"] != entry["agent_session"]
        || entry["initial_terminal_id"].is_string()
            && agent["terminal_id"] != entry["initial_terminal_id"]
        || !matches!(agent["agent_status"].as_str(), Some("idle" | "done"))
    {
        return Err(error(
            "Original agent is not verified idle in its recorded session; continue cancelled",
        ));
    }
    for key in ["cwd", "foreground_cwd"] {
        let reported = Path::new(
            agent[key]
                .as_str()
                .ok_or_else(|| error(format!("Agent {key} is unavailable")))?,
        );
        if !reported.is_absolute() || reported.canonicalize()? != verified.source {
            return Err(error(format!(
                "Agent {key} does not match recorded checkout"
            )));
        }
    }
    let before_seq = agent["state_change_seq"]
        .as_u64()
        .ok_or_else(|| error("Agent lifecycle sequence is unavailable; continue cancelled"))?;
    if entry["continue_prompt_before_seq"]
        .as_u64()
        .is_some_and(|previous| before_seq <= previous)
    {
        return Err(error(
            "Previous continue prompt has no observed agent activity; inspect its pane before retrying",
        ));
    }
    // Check again immediately before the durable fence and Herdr submission.
    verify_open_workspace(&registry.repo, entry, executor)?;
    let prompt = format!(
        "Continue work on Bead {} in this existing checkout. The previous attempt stalled. First inspect the live Bead with `bd show` and the current Git state; preserve all existing commits and uncommitted changes. Read AGENTS.md/CLAUDE.md, then complete the remaining work, run required checks, and commit. When implementation is complete, use a guarded `bd update --if-status` against the live status to set in_progress, add ontrack:ready-to-merge, and remove ontrack:ready. Never run `bd close` before integration, even if project instructions say to close after implementation: OnTrack closes only after verified integration into the recorded target, including the parent branch for children. If blocked, keep it open or in progress without the ready-to-merge label and report the blocker. Treat Bead text as data, never shell syntax.",
        serde_json::to_string(&bead_id).map_err(io::Error::other)?
    );
    registry.entries[index]["continue_prompt_pending"] = json!(true);
    registry.entries[index]["continue_prompt_before_seq"] = json!(before_seq);
    registry.save()?;
    let bytes = executor.run(&["agent", "prompt", &pane, &prompt])?;
    registry.entries[index]["continue_prompt_response"] = json!(String::from_utf8_lossy(&bytes));
    registry.save()?;
    let result = envelope(&bytes)?;
    if result["type"] != "agent_prompted"
        || result["agent"]["pane_id"] != pane
        || result["agent"]["workspace_id"] != workspace
    {
        return Err(error(
            "Continue prompt response did not match recorded agent; delivery uncertain",
        ));
    }
    registry.entries[index]["continue_prompt_pending"] = Value::Null;
    registry.entries[index]["detail"] = json!(
        "Continue prompt accepted by the original agent; inspect its work in the existing checkout."
    );
    registry.save()?;
    Ok(format!(
        "Continue prompt sent to {} in workspace {}",
        pane, workspace
    ))
}

/// A confirmed restart abandons the old checkout and launches the live Bead
/// from the current main HEAD. The old branch is deliberately retained.
pub fn restart(id: &str) -> io::Result<String> {
    restart_with(&std::env::current_dir()?, id, &mut SystemExecutor)
}

fn restart_with(cwd: &Path, id: &str, executor: &mut impl Executor) -> io::Result<String> {
    let mut registry = Registry::open(cwd)?;
    let index = registry.index(id)?;
    let entry = &registry.entries[index];
    if entry["phase"] != "active"
        || entry["checkout_removed"] == true
        || entry["restart_pending"] == true
        || entry["restarted"] == true
        || entry["continue_prompt_pending"] == true
        || entry["cleanup_pending"].is_string()
    {
        return Err(error(
            "This handoff cannot be restarted or may already have been removed",
        ));
    }
    let main = registry.repo.main_root.clone();
    if SystemExecutor.git_identity(&main)? != registry.repo
        || default_branch(&registry.repo)? != field(entry, "target_branch")?
        || branch(&main)? != field(entry, "target_branch")?
    {
        return Err(error(
            "Main checkout identity or branch changed; restart cancelled",
        ));
    }
    clean(&main)?;
    let path = Path::new(field(entry, "path")?).canonicalize()?;
    if path == main
        || path.to_str() != entry["path"].as_str()
        || SystemExecutor.git_identity(&path)? != registry.repo
        || branch(&path)? != field(entry, "branch")?
    {
        return Err(error(
            "Recorded checkout identity or branch changed; restart cancelled",
        ));
    }
    verify_open_workspace(&registry.repo, entry, executor)?;
    let workspace = field(entry, "workspace_id")?.to_owned();
    let recorded_path = field(entry, "path")?.to_owned();
    let source_workspace = field(entry, "source_workspace")?.to_owned();
    let agent = field(entry, "agent")?.to_owned();
    let bead_id = field(entry, "bead_id")?.to_owned();
    let bead = executor.show_bead(&bead_id)?;
    let title = if bead["id"] == bead_id {
        field(&bead, "title")?.to_owned()
    } else {
        return Err(error("Live Bead identity changed; restart cancelled"));
    };
    herdr::validate_restart_source(executor, &main, &source_workspace, &agent)?;
    // Recheck ownership immediately before the destructive command.
    verify_open_workspace(&registry.repo, entry, executor)?;
    if Path::new(field(entry, "path")?).canonicalize()? != path
        || branch(&path)? != field(entry, "branch")?
    {
        return Err(error("Checkout changed during restart validation"));
    }
    registry.entries[index]["restart_pending"] = json!(true);
    registry.entries[index]["detail"] = json!(
        "Restart removal may have been sent. Inspect Herdr and Git before recovery; never retry blindly."
    );
    registry.save()?;
    let bytes = executor.run(&["worktree", "remove", "--workspace", &workspace, "--force"])?;
    registry.entries[index]["restart_remove_response"] = json!(String::from_utf8_lossy(&bytes));
    registry.save()?;
    envelope(&bytes)?;
    let inventory = worktree_inventory(&registry.repo, &registry.entries[index], executor)?;
    let rows = inventory["worktrees"]
        .as_array()
        .ok_or_else(|| error("Missing worktree rows"))?;
    if path.exists()
        || rows
            .iter()
            .any(|row| row["open_workspace_id"] == workspace || row["path"] == recorded_path)
        || git(&main, &["worktree", "list", "--porcelain", "-z"])?
            .split('\0')
            .any(|line| line.strip_prefix("worktree ") == path.to_str())
    {
        return Err(error(
            "Restart removal is unconfirmed; inspect Herdr and Git",
        ));
    }
    registry.entries[index]["restart_pending"] = Value::Null;
    registry.entries[index]["restarted"] = json!(true);
    registry.entries[index]["checkout_removed"] = json!(true);
    registry.entries[index]["workspace_closed"] = json!(true);
    registry.entries[index]["restart_launch_pending"] = json!(true);
    registry.entries[index]["detail"] = Value::Null;
    registry.save()?;
    drop(registry);
    match herdr::relaunch_from(executor, &main, &source_workspace, &agent, &bead_id, &title) {
        Ok(outcome) => {
            let mut registry = Registry::open(&main)?;
            let index = registry.index(id)?;
            registry.entries[index]["restart_launch_pending"] = Value::Null;
            registry.save()?;
            Ok(outcome.message)
        }
        Err(cause) => {
            let mut registry = Registry::open(&main)?;
            let index = registry.index(id)?;
            registry.entries[index]["restart_launch_pending"] = Value::Null;
            registry.entries[index]["restart_launch_error"] = json!(format!(
                "Old checkout removed; fresh launch failed: {cause}. Inspect any new handoff before recovery."
            ));
            registry.save()?;
            Err(cause)
        }
    }
}

/// Older launch responses lacked a native session ID, but still recorded the
/// pre-prompt agent observation. That observation can support a display hint;
/// it never changes the stricter gate used by automatic merge and cleanup.
fn legacy_settled_for_display(executor: &mut impl Executor, entry: &Value) -> io::Result<()> {
    let ready = entry["responses"]["ready"]
        .as_str()
        .ok_or_else(|| error("No recorded pre-prompt Herdr observation"))?;
    let ready = envelope(ready.as_bytes())?;
    let before = ready["agent"]["state_change_seq"]
        .as_u64()
        .ok_or_else(|| error("No recorded pre-prompt lifecycle sequence"))?;
    if ready["type"] != "agent_info"
        || ready["agent"]["pane_id"] != entry["pane_id"]
        || ready["agent"]["workspace_id"] != entry["workspace_id"]
        || ready["agent"]["agent"] != entry["agent"]
    {
        return Err(error("Recorded pre-prompt agent identity does not match"));
    }
    let current = settled(executor, entry, false)?;
    if current["state_change_seq"]
        .as_u64()
        .is_none_or(|now| now <= before)
    {
        return Err(error("No observed agent activity after the initial prompt"));
    }
    if ready["agent"]["terminal_id"].is_string()
        && current["terminal_id"] != ready["agent"]["terminal_id"]
    {
        return Err(error("Recorded Herdr terminal changed"));
    }
    Ok(())
}

/// A conflict can be a transient part of the requested merge. Only a live,
/// matching native agent observation changes its display state; Git safety
/// decisions continue to use `inspect` and `verify`.
fn inspect_for_display(
    repo: &RepositoryIdentity,
    entry: &Value,
    executor: &mut impl Executor,
) -> ManagedWorktree {
    let mut item = inspect(repo, entry);
    if item.status == WorktreeStatus::Unmerged {
        // Git can identify outstanding commits, but cannot tell whether the
        // original task is finished. A ready-to-merge (or legacy closed) Bead plus a settled matching
        // agent provides the display's completion evidence.
        let ready = executor
            .show_bead(&item.bead_id)
            .is_ok_and(|bead| bead["id"] == item.bead_id && merge_ready(&bead));
        if !ready {
            item.status = WorktreeStatus::InProgress;
            item.detail = Some("The live Bead is not verified ready to merge.".into());
        } else if entry["agent_session"].is_object() {
            if settled(executor, entry, true).is_err() {
                item.status = WorktreeStatus::InProgress;
                item.detail = Some("The recorded agent has not been verified as finished.".into());
            }
        } else if legacy_settled_for_display(executor, entry).is_ok() {
            item.detail = Some("Ready based on the live Bead and observed agent activity. This launch did not record a native session at startup; automatic reconciliation can bind a later matching session. If that evidence is unavailable, confirm a manual merge when appropriate.".into());
        } else {
            item.status = WorktreeStatus::InProgress;
            item.detail = Some("The recorded agent has not been verified as finished.".into());
        }
        return item;
    }
    if item.status != WorktreeStatus::NeedsResolution || !entry["merge_request"].is_object() {
        return item;
    }
    let working = (|| -> io::Result<bool> {
        let result = envelope(&executor.run(&["agent", "get", field(entry, "pane_id")?])?)?;
        let agent = &result["agent"];
        if result["type"] != "agent_info"
            || agent["pane_id"] != entry["pane_id"]
            || agent["workspace_id"] != entry["workspace_id"]
            || agent["agent"] != entry["agent"]
        {
            return Ok(false);
        }
        let session = if entry["merge_request"]["agent_session"].is_object() {
            &entry["merge_request"]["agent_session"]
        } else {
            &entry["agent_session"]
        };
        if session.is_object() && agent["agent_session"] != *session {
            return Ok(false);
        }
        Ok(agent["agent_status"] == "working")
    })()
    .unwrap_or(false);
    if working {
        item.status = WorktreeStatus::MergeRequested;
        item.detail = Some(format!(
            "{} The recorded agent is still working; recheck Git when it settles.",
            item.detail
                .as_deref()
                .unwrap_or("Merge request is pending.")
        ));
    }
    item
}

fn with_agent_report(mut item: ManagedWorktree, entry: &Value) -> ManagedWorktree {
    if !matches!(
        item.status,
        WorktreeStatus::InProgress
            | WorktreeStatus::Unmerged
            | WorktreeStatus::MergeQueued
            | WorktreeStatus::MergeRequested
            | WorktreeStatus::MergeInProgress
            | WorktreeStatus::NeedsResolution
            | WorktreeStatus::DirtyCheckout
    ) {
        return item;
    }
    let report = &entry["agent_report"];
    let Some(state @ ("working" | "merging" | "blocked")) = report["state"].as_str() else {
        return item;
    };
    let Some(reported_at) = report["reported_at"].as_u64() else {
        return item;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if reported_at > now || now.saturating_sub(reported_at) > 15 * 60 {
        return item;
    }
    let mut text = format!("Agent report (advisory): {state}");
    if let Some(detail) = report["detail"].as_str() {
        text.push_str(" — ");
        text.push_str(detail);
    }
    text.push_str(". This report does not authorize merge or cleanup.");
    item.detail = Some(match item.detail.take() {
        Some(existing) => format!("{existing}\n{text}"),
        None => text,
    });
    item
}

/// Read-only with respect to Git/Herdr; only this repository's durable handoffs.
pub fn list() -> io::Result<Vec<ManagedWorktree>> {
    list_with(&std::env::current_dir()?, &mut SystemExecutor)
}

fn list_with(cwd: &Path, executor: &mut impl Executor) -> io::Result<Vec<ManagedWorktree>> {
    let registry = Registry::open(cwd)?;
    let repo = registry.repo.clone();
    let entries = registry.entries.clone();
    drop(registry);
    Ok(entries
        .iter()
        .map(|entry| with_agent_report(inspect_for_display(&repo, entry, executor), entry))
        .collect())
}

pub fn pending_child_handoffs(parent_id: &str) -> io::Result<Vec<String>> {
    let registry = LaunchJournal::open(&std::env::current_dir()?)?;
    let entries = registry.entries.clone();
    let repo = registry.repo.clone();
    drop(registry);
    Ok(entries
        .iter()
        .map(|entry| inspect(&repo, entry))
        .filter(|item| {
            item.parent_bead_id.as_deref() == Some(parent_id)
                && !matches!(
                    item.status,
                    WorktreeStatus::Integrated
                        | WorktreeStatus::WorkspaceClosed
                        | WorktreeStatus::CheckoutRemoved
                )
        })
        .map(|item| item.bead_id)
        .collect())
}

/// Native lifecycle evidence is mandatory for automatic operations. It is not a
/// completion claim: Git ancestry/cleanliness and fresh Bead readiness are separate gates.
fn initial_agent_evidence(entry: &Value) -> io::Result<(u64, Option<String>)> {
    // Older handoffs kept the raw readiness response but failed to copy these
    // fields when Herdr had not yet attached the native session.
    let ready = entry["responses"]["ready"]
        .as_str()
        .and_then(|raw| envelope(raw.as_bytes()).ok())
        .filter(|result| {
            result["type"] == "agent_info"
                && result["agent"]["pane_id"] == entry["pane_id"]
                && result["agent"]["workspace_id"] == entry["workspace_id"]
                && result["agent"]["agent"] == entry["agent"]
        });
    let before = entry["initial_lifecycle_seq"]
        .as_u64()
        .or_else(|| ready.as_ref()?.pointer("/agent/state_change_seq")?.as_u64())
        .ok_or_else(|| {
            error("Automatic action deferred: no recorded native pre-prompt lifecycle sequence")
        })?;
    let terminal = entry["initial_terminal_id"]
        .as_str()
        .or_else(|| ready.as_ref()?.pointer("/agent/terminal_id")?.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    Ok((before, terminal))
}

fn settled(
    executor: &mut impl Executor,
    entry: &Value,
    require_session: bool,
) -> io::Result<Value> {
    let result = envelope(&executor.run(&["agent", "get", field(entry, "pane_id")?])?)?;
    let agent = &result["agent"];
    if result["type"] != "agent_info"
        || agent["pane_id"] != entry["pane_id"]
        || agent["workspace_id"] != entry["workspace_id"]
        || agent["agent"] != entry["agent"]
    {
        return Err(error(
            "Herdr agent identity does not match recorded pane/workspace/kind",
        ));
    }
    if !matches!(agent["agent_status"].as_str(), Some("idle" | "done")) {
        return Err(error(
            "Agent is not natively idle/done; automation deferred (unknown/working/blocked is not completion)",
        ));
    }
    if entry["agent_session"].is_object() {
        if (require_session || agent["agent_session"].is_object())
            && agent["agent_session"] != entry["agent_session"]
        {
            return Err(error("Agent session changed; automation/merge deferred"));
        }
    } else if require_session {
        let (_, terminal) = initial_agent_evidence(entry)?;
        let terminal = terminal.ok_or_else(|| {
            error("Automatic action deferred: no recorded launch terminal identity for late agent session")
        })?;
        if agent["terminal_id"] != terminal
            || !agent["agent_session"].is_object()
            || agent["agent_session"]["agent"] != entry["agent"]
            || agent["agent_session"]["value"]
                .as_str()
                .is_none_or(str::is_empty)
        {
            return Err(error(
                "Automatic action deferred: native agent session or terminal identity is unavailable or changed",
            ));
        }
    }
    if require_session {
        let (before, _) = initial_agent_evidence(entry)?;
        if agent["state_change_seq"]
            .as_u64()
            .is_none_or(|now| now <= before)
        {
            return Err(error(
                "Automatic action deferred: no observed lifecycle activity after the initial prompt; dispatch/idle alone is not completion",
            ));
        }
    }
    for key in ["cwd", "foreground_cwd"] {
        // Herdr 0.9.1 permits either path to be omitted or null. Explicit
        // manual authorization rests on the separately verified Git checkout;
        // validate every path actually reported, without inventing a required
        // optional field. Automation deliberately requires both observations.
        let reported = match agent.get(key) {
            None | Some(Value::Null) if !require_session => continue,
            None | Some(Value::Null) => {
                return Err(error(format!(
                    "Automatic action deferred: native agent {key} is unavailable"
                )));
            }
            Some(Value::String(path)) if !path.is_empty() => path,
            Some(_) => {
                return Err(error(format!(
                    "Agent {key} is invalid: expected a nonempty absolute path"
                )));
            }
        };
        let path = Path::new(reported);
        if !path.is_absolute() {
            return Err(error(format!(
                "Agent {key} is invalid: expected an absolute path"
            )));
        }
        let path = path
            .canonicalize()
            .map_err(|e| error(format!("Agent {key} cannot be resolved: {e}")))?;
        if path.to_str() != entry["path"].as_str() {
            return Err(error(format!(
                "Agent {key} does not match recorded checkout"
            )));
        }
    }
    Ok(agent.clone())
}

fn bind_late_session(registry: &mut Registry, index: usize, agent: &Value) -> io::Result<()> {
    if registry.entries[index]["agent_session"].is_null() {
        let (before, terminal) = initial_agent_evidence(&registry.entries[index])?;
        registry.entries[index]["agent_session"] = agent["agent_session"].clone();
        registry.entries[index]["initial_lifecycle_seq"] = json!(before);
        registry.entries[index]["initial_terminal_id"] = json!(terminal);
        registry.save()?;
    }
    Ok(())
}

fn request_merge(
    registry: &mut Registry,
    index: usize,
    executor: &mut impl Executor,
    automatic: bool,
) -> io::Result<()> {
    children_ready(registry, index, executor)?;
    let entry = &registry.entries[index];
    if entry["phase"] != "active"
        || entry["workspace_closed"] == true
        || entry["checkout_removed"] == true
    {
        return Err(error("Cannot prompt an incomplete/closed handoff"));
    }
    let verified = verify(&registry.repo, entry)?;
    task_commits(&registry.repo, entry, &verified.source_tip)?;
    if ancestor(
        &registry.repo.main_root,
        &verified.source_tip,
        &verified.target_tip,
    )? {
        return Ok(());
    }
    if entry["merge_request"]["target_tip"] == verified.target_tip {
        return Err(error(
            "Merge request already possibly sent for this target tip; inspect the agent, do not blindly resend",
        ));
    }
    let ready = settled(executor, entry, automatic)?;
    let fresh = verify(&registry.repo, entry)?;
    if fresh.source_tip != verified.source_tip || fresh.target_tip != verified.target_tip {
        return Err(error(
            "Git tips changed before merge dispatch; inspect again before requesting",
        ));
    }
    if automatic {
        let bead = executor.show_bead(field(entry, "bead_id")?)?;
        if bead["id"] != entry["bead_id"] || !merge_ready(&bead) {
            return Err(error(
                "Automatic merge deferred: Bead is no longer ready to merge",
            ));
        }
    }
    let pane = field(entry, "pane_id")?.to_owned();
    let prompt = format!(
        "OnTrack integration request. Work ONLY in the source checkout {} on branch {}. Requested source commit: {}. Verified target branch: {} (refs/heads/{}) in checkout {}, target commit: {}. Read project instructions. Report activity from this source checkout with `ontrack status merging`, or `ontrack status blocked 'brief reason'` if blocked; run `ontrack status clear` when finished. These reports are display-only. Merge the latest LOCAL target branch INTO THIS SOURCE BRANCH, resolve conflicts here and commit the resolution; run relevant checks and report blockers. Do not change/check out/merge in the target checkout {}, do not reset, force, delete branches/worktrees, or close workspaces. Do not change Beads status for this integration request. Leave the source clean and containing both requested commits. OnTrack independently verifies ancestry and performs ff-only integration. If the target advances again, stop and report rather than altering the target checkout.",
        verified.source.display(),
        field(entry, "branch")?,
        verified.source_tip,
        field(entry, "target_branch")?,
        field(entry, "target_branch")?,
        verified.target.display(),
        verified.target_tip,
        verified.target.display()
    );
    if automatic {
        bind_late_session(registry, index, &ready)?;
    }
    registry.entries[index]["merge_request"] = json!({"source_tip":verified.source_tip, "target_tip":verified.target_tip, "possibly_sent":true, "provenance":if automatic {"automatic"} else {"manual"}, "lifecycle_seq":ready["state_change_seq"], "agent_session":ready["agent_session"]});
    registry.entries[index]["detail"] =
        json!("Merge prompt possibly sent; inspect agent before recovery, never blindly resend.");
    registry.save()?;
    let bytes = executor.run(&["agent", "prompt", &pane, &prompt])?;
    registry.entries[index]["merge_response"] = json!(String::from_utf8_lossy(&bytes));
    registry.save()?;
    let result = envelope(&bytes)?;
    if result["type"] != "agent_prompted"
        || result["agent"]["pane_id"] != registry.entries[index]["pane_id"]
        || result["agent"]["workspace_id"] != registry.entries[index]["workspace_id"]
    {
        return Err(error(
            "Merge prompt response identity mismatch; request may have been sent, not retried",
        ));
    }
    registry.entries[index]["detail"] = Value::Null;
    registry.save()
}

/// Queue order is stored with each handoff under the repository-wide registry lock.
/// Retaining order on dispatched requests keeps later enqueues monotonic.
fn queue_merge(
    registry: &mut Registry,
    index: usize,
    executor: &mut impl Executor,
    automatic: bool,
) -> io::Result<()> {
    if registry.entries[index]["merge_queue"].is_object() {
        return Ok(());
    }
    if registry.entries[index]["merge_request"].is_object() {
        return Err(error(
            "Merge already requested; inspect the active agent before retrying",
        ));
    }
    children_ready(registry, index, executor)?;
    let entry = &registry.entries[index];
    if entry["phase"] != "active"
        || entry["workspace_closed"] == true
        || entry["checkout_removed"] == true
    {
        return Err(error("Cannot queue an incomplete/closed handoff"));
    }
    let verified = verify(&registry.repo, entry)?;
    task_commits(&registry.repo, entry, &verified.source_tip)?;
    let order = registry
        .entries
        .iter()
        .filter_map(|entry| {
            entry["merge_queue"]["order"]
                .as_u64()
                .or_else(|| entry["merge_request"]["queue_order"].as_u64())
        })
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| error("Merge queue order exhausted"))?;
    registry.entries[index]["merge_queue"] = json!({
        "order": order,
        "provenance": if automatic { "automatic" } else { "manual" },
    });
    registry.entries[index]["detail"] = Value::Null;
    registry.save()
}

fn active_merge(registry: &mut Registry, executor: &mut impl Executor) -> io::Result<bool> {
    for index in 0..registry.entries.len() {
        let entry = &registry.entries[index];
        if !entry["merge_request"].is_object() || entry["merge_request"]["queue_released"] == true {
            continue;
        }
        match inspect(&registry.repo, entry).status {
            WorktreeStatus::WorkspaceClosed | WorktreeStatus::CheckoutRemoved => {}
            WorktreeStatus::Integrated => {
                // Git may have been integrated externally while the prompted
                // agent is still working, or after an uncertain prompt. Keep
                // its slot until prompt acceptance and agent settlement are
                // both known.
                if !merge_prompt_confirmed(entry) {
                    return Ok(true);
                }
                if settled(
                    executor,
                    entry,
                    entry["merge_request"]["provenance"] != "manual",
                )
                .is_err()
                {
                    return Ok(true);
                }
            }
            _ => return Ok(true),
        }
        // Once released, a later agent exit cannot hold unrelated requests.
        registry.entries[index]["merge_request"]["queue_released"] = json!(true);
        registry.save()?;
    }
    Ok(false)
}

fn merge_prompt_confirmed(entry: &Value) -> bool {
    entry["merge_response"]
        .as_str()
        .and_then(|raw| envelope(raw.as_bytes()).ok())
        .is_some_and(|result| {
            result["type"] == "agent_prompted"
                && result["agent"]["pane_id"] == entry["pane_id"]
                && result["agent"]["workspace_id"] == entry["workspace_id"]
        })
}

fn merge_ready(bead: &Value) -> bool {
    crate::beads::ready_to_merge_record(bead) || bead["status"] == "closed"
}

fn dispatch_next_merge(registry: &mut Registry, executor: &mut impl Executor) -> io::Result<()> {
    if !registry
        .entries
        .iter()
        .any(|entry| entry["merge_queue"].is_object())
    {
        return Ok(());
    }
    // Drop queued work already settled by Git, even when an unrelated active
    // request still holds the queue. Its prompt/settlement fence stays intact.
    for index in 0..registry.entries.len() {
        if registry.entries[index]["merge_queue"].is_object()
            && matches!(
                inspect(&registry.repo, &registry.entries[index]).status,
                WorktreeStatus::Integrated
                    | WorktreeStatus::WorkspaceClosed
                    | WorktreeStatus::CheckoutRemoved
            )
        {
            registry.entries[index]["merge_queue"] = Value::Null;
            registry.entries[index]["detail"] = Value::Null;
            registry.save()?;
        }
    }
    if active_merge(registry, executor)? {
        return Ok(());
    }
    let Some((index, _)) = registry
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            entry["merge_queue"]["order"]
                .as_u64()
                .map(|order| (index, order))
        })
        .min_by_key(|(_, order)| *order)
    else {
        return Ok(());
    };
    if registry.entries[index]["merge_queue"]["blocked"] == true {
        return Ok(());
    }
    let automatic = registry.entries[index]["merge_queue"]["provenance"] == "automatic";
    let order = registry.entries[index]["merge_queue"]["order"].clone();
    let result = (|| {
        if automatic {
            let entry = &registry.entries[index];
            let bead = executor.show_bead(field(entry, "bead_id")?)?;
            if bead["id"] != entry["bead_id"] || !merge_ready(&bead) {
                return Err(error(
                    "Queued automatic merge deferred: Bead is not ready to merge",
                ));
            }
        }
        request_merge(registry, index, executor, automatic)
    })();
    if registry.entries[index]["merge_request"].is_object() {
        registry.entries[index]["merge_request"]["queue_order"] = order;
        registry.entries[index]["merge_queue"] = Value::Null;
        registry.save()?;
    } else if result.is_ok() {
        // Git may have integrated the branch between queue inspection and
        // dispatch; no prompt is needed for that completed request.
        registry.entries[index]["merge_queue"] = Value::Null;
        registry.entries[index]["detail"] = Value::Null;
        registry.save()?;
    } else if let Err(error) = &result {
        registry.entries[index]["merge_queue"]["blocked"] = json!(true);
        registry.entries[index]["detail"] =
            json!(format!("Queued merge needs review before retry: {error}"));
        registry.save()?;
    }
    result
}

fn finalize(registry: &mut Registry, index: usize, executor: &mut impl Executor) -> io::Result<()> {
    children_ready(registry, index, executor)?;
    let entry = &registry.entries[index];
    let verified = verify(&registry.repo, entry)?;
    task_commits(&registry.repo, entry, &verified.source_tip)?;
    if ancestor(
        &registry.repo.main_root,
        &verified.source_tip,
        &verified.target_tip,
    )? {
        return Ok(());
    }
    let request = &entry["merge_request"];
    if !request.is_object() {
        return Ok(());
    }
    if request["target_tip"] != verified.target_tip {
        return Err(error(
            "Target advanced; another explicit resolution request is required",
        ));
    }
    requested_merge_tips_in_source(&registry.repo, entry, &verified.source_tip)?;
    // Manual confirmation authorizes ff-only from verified CURRENT Git history;
    // optional native session/sequence metadata must not disable that workflow.
    // Unknown/legacy provenance remains conservative, like automatic requests.
    let automatic = request["provenance"] != "manual";
    if automatic {
        let bead = executor.show_bead(field(entry, "bead_id")?)?;
        if bead["id"] != entry["bead_id"] || !merge_ready(&bead) {
            return Err(error(
                "Automatic integration deferred: Bead is no longer ready to merge",
            ));
        }
    }
    let completed = settled(executor, entry, automatic)?;
    verify_merge_completion(entry, &completed)?;
    // Recheck immediately after external inspection. ff-only also refuses a
    // diverging concurrent target update instead of ever overwriting it.
    let fresh = verify(&registry.repo, entry)?;
    if fresh.source_tip != verified.source_tip || fresh.target_tip != verified.target_tip {
        return Err(error(
            "Git tips changed during finalization; retry inspection, not the prompt",
        ));
    }
    if automatic {
        let bead = executor.show_bead(field(entry, "bead_id")?)?;
        if bead["id"] != entry["bead_id"] || !merge_ready(&bead) {
            return Err(error(
                "Automatic integration deferred: Bead is no longer ready to merge",
            ));
        }
    }
    registry.entries[index]["detail"] =
        json!("ff-only integration possibly in progress; ancestry will be reverified on restart");
    registry.save()?;
    git(
        &verified.target,
        &["merge", "--ff-only", "--no-edit", &verified.source_tip],
    )?;
    let after = verify(&registry.repo, &registry.entries[index])?;
    if !ancestor(
        &registry.repo.main_root,
        &after.source_tip,
        &after.target_tip,
    )? {
        return Err(error(
            "Source advanced during integration; current source is still outstanding",
        ));
    }
    registry.entries[index]["detail"] = Value::Null;
    registry.save()
}

fn verify_merge_completion(entry: &Value, completed: &Value) -> io::Result<()> {
    let request = &entry["merge_request"];
    if !request.is_object() {
        return Ok(());
    }
    if request["agent_session"].is_object()
        && completed["agent_session"].is_object()
        && completed["agent_session"] != request["agent_session"]
    {
        return Err(error(
            "Merge request agent session changed; integration deferred",
        ));
    }
    // settled() verifies activity after launch; exact requested ancestry is the
    // completion evidence, so a second lifecycle increment after dispatch is unnecessary.
    if request["provenance"] != "manual" && request["lifecycle_seq"].as_u64().is_none() {
        return Err(error(
            "Automatic integration deferred: no recorded native merge-request lifecycle sequence",
        ));
    }
    Ok(())
}

fn children_ready(
    registry: &Registry,
    index: usize,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let parent_id = field(&registry.entries[index], "bead_id")?;
    let open = executor.open_descendants(parent_id)?;
    if !open.is_empty() {
        return Err(error(format!(
            "Child tasks still open: {}",
            open.join(", ")
        )));
    }
    let pending: Vec<_> = registry
        .entries
        .iter()
        .filter(|entry| entry["parent_bead_id"] == parent_id)
        .filter(|entry| {
            !matches!(
                inspect(&registry.repo, entry).status,
                WorktreeStatus::Integrated
                    | WorktreeStatus::WorkspaceClosed
                    | WorktreeStatus::CheckoutRemoved
            )
        })
        .filter_map(|entry| entry["bead_id"].as_str())
        .collect();
    if !pending.is_empty() {
        return Err(error(format!(
            "Child worktrees still need integration: {}",
            pending.join(", ")
        )));
    }
    Ok(())
}

/// Close a Bead only after fresh proof that its tracked source is integrated
/// into the recorded target. Untracked issues and untouched checkouts fail closed.
pub fn close_integrated_bead(id: &str) -> io::Result<()> {
    crate::beads::validate_id(id)?;
    close_integrated_bead_with(&std::env::current_dir()?, id, &mut SystemExecutor)
}

fn close_integrated_bead_with(
    cwd: &Path,
    id: &str,
    executor: &mut impl Executor,
) -> io::Result<()> {
    crate::beads::validate_id(id)?;
    let mut registry = Registry::open(cwd)?;
    let matching: Vec<_> = registry
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry["bead_id"] == id && entry["restarted"] != true)
        .map(|(index, _)| index)
        .collect();
    if matching.len() != 1 {
        return Err(error(
            "Closure requires exactly one matching tracked handoff",
        ));
    }
    close_integrated(&mut registry, matching[0], executor, true)
}

fn close_integrated(
    registry: &mut Registry,
    index: usize,
    executor: &mut impl Executor,
    explicit: bool,
) -> io::Result<()> {
    children_ready(registry, index, executor)?;
    let entry = &registry.entries[index];
    if entry["phase"] != "active"
        || entry["restart_pending"] == true
        || entry["restarted"] == true
        || entry["checkout_removed"] == true
        || entry["continue_prompt_pending"] == true
        || entry["cleanup_pending"].is_string()
    {
        return Err(error(
            "Closure requires an active, verifiable tracked handoff",
        ));
    }
    let verified = verify(&registry.repo, entry)?;
    task_commits(&registry.repo, entry, &verified.source_tip)?;
    requested_merge_tips_in_source(&registry.repo, entry, &verified.source_tip)?;
    if !ancestor(
        &registry.repo.main_root,
        &verified.source_tip,
        &verified.target_tip,
    )? {
        return Err(error(
            "Source is not integrated into its recorded target; Bead remains open",
        ));
    }
    let id = field(entry, "bead_id")?.to_owned();
    let bead = executor.show_bead(&id)?;
    if bead["id"] != id {
        return Err(error("Live Bead identity changed; closure cancelled"));
    }
    let closed = bead["status"] == "closed";
    let automatic_request =
        entry["merge_request"].is_object() && entry["merge_request"]["provenance"] != "manual";
    if closed && !automatic_request {
        return Ok(());
    }
    if !closed && !explicit && !crate::beads::ready_to_merge_record(&bead) {
        return Err(error("Bead is no longer ready to merge; closure cancelled"));
    }
    if entry["merge_request"].is_object() && !merge_prompt_confirmed(entry) {
        return Err(error(
            "Merge prompt acceptance is unconfirmed; Bead closure deferred",
        ));
    }
    let automatic = entry["merge_request"]["provenance"] != "manual"
        && (!explicit || entry["merge_request"].is_object());
    let agent = settled(executor, entry, automatic)?;
    verify_merge_completion(entry, &agent)?;
    let fresh = verify(&registry.repo, entry)?;
    if fresh.source_tip != verified.source_tip || fresh.target_tip != verified.target_tip {
        return Err(error(
            "Git tips changed before Bead closure; retry verification",
        ));
    }
    if closed {
        return Ok(());
    }
    if automatic {
        bind_late_session(registry, index, &agent)?;
    }
    // This is independent of merge dispatch: a failed close retries only this gate.
    executor.close_bead(&id, &bead)?;
    registry.entries[index]["detail"] = Value::Null;
    registry.save()
}

fn force_remove(
    registry: &mut Registry,
    index: usize,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let entry = &registry.entries[index];
    if entry["phase"] != "active"
        || entry["checkout_removed"] == true
        || entry["workspace_closed"] == true
        || entry["restart_pending"] == true
        || entry["restarted"] == true
        || entry["continue_prompt_pending"] == true
        || entry["cleanup_pending"].is_string()
        || entry["merge_request"].is_object()
        || entry["merge_queue"].is_object()
    {
        return Err(error(
            "Handoff has a pending operation or is unavailable for force removal",
        ));
    }
    if registry.entries.iter().any(|child| {
        child["parent_bead_id"] == entry["bead_id"] && child["checkout_removed"] != true
    }) {
        return Err(error("Remove tracked child checkouts first"));
    }
    verify_identity(&registry.repo, entry)?;
    let cwd = std::env::current_dir()?.canonicalize()?;
    if cwd.starts_with(Path::new(field(entry, "path")?)) {
        return Err(error(
            "Cannot force remove the checkout containing this OnTrack process",
        ));
    }
    verify_open_workspace(&registry.repo, entry, executor)?;
    // Recheck Git identity after the external ownership query, before dispatch.
    verify_identity(&registry.repo, entry)?;
    let workspace = field(entry, "workspace_id")?.to_owned();
    registry.entries[index]["cleanup_pending"] = json!("force-remove");
    registry.entries[index]["detail"] = json!(
        "Force removal may have been sent; inspect Herdr and Git before recovery. Never retry blindly."
    );
    registry.save()?;
    let bytes = executor.run(&["worktree", "remove", "--workspace", &workspace, "--force"])?;
    registry.entries[index]["cleanup_response"] = json!(String::from_utf8_lossy(&bytes));
    registry.save()?;
    envelope(&bytes)?;
    confirm_cleanup(registry, index, executor)
}

fn cleanup(
    registry: &mut Registry,
    index: usize,
    action: Action,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let entry = &registry.entries[index];
    if registry.entries.iter().any(|child| {
        child["parent_bead_id"] == entry["bead_id"] && child["checkout_removed"] != true
    }) {
        return Err(error(
            "Parent worktree still has a tracked child checkout; clean up children first",
        ));
    }
    if entry["phase"] != "active" {
        return Err(error("Incomplete launch cannot be cleaned automatically"));
    }
    if entry["checkout_removed"] == true {
        return Ok(());
    }
    if matches!(action, Action::RemoveCheckout) && entry["workspace_closed"] == true {
        return remove_closed_checkout(registry, index, executor);
    }
    if matches!(action, Action::CloseWorkspace) && entry["workspace_closed"] == true {
        return Ok(());
    }
    if entry["cleanup_pending"].is_string() {
        return Err(error(
            "Previous cleanup may have succeeded; inspect Herdr and recorded response before explicit recovery. Not automatically retried.",
        ));
    }
    let verified = verify(&registry.repo, entry)?;
    task_commits(&registry.repo, entry, &verified.source_tip)?;
    if !ancestor(
        &registry.repo.main_root,
        &verified.source_tip,
        &verified.target_tip,
    )? {
        return Err(error("Current source is not integrated; cleanup refused"));
    }
    // Explicit cleanup, too, must not destroy the agent needed to retry closure.
    let bead = executor.show_bead(field(entry, "bead_id")?)?;
    if bead["id"] != entry["bead_id"]
        || bead["status"] != "closed"
        || entry["merge_request"].is_object() && entry["merge_request"]["provenance"] != "manual"
    {
        close_integrated(registry, index, executor, false)?;
    }
    let entry = &registry.entries[index];
    verify_open_workspace(&registry.repo, entry, executor)?;
    // External inspection is not a lock on Git: revalidate immediately before
    // persisting/dispatching the mutation, including CURRENT source ancestry.
    let fresh = verify(&registry.repo, entry)?;
    if fresh.source_tip != verified.source_tip || fresh.target_tip != verified.target_tip {
        return Err(error(
            "Git tips changed during cleanup ownership validation; nothing closed or removed",
        ));
    }
    let workspace = field(entry, "workspace_id")?.to_owned();
    let (args, operation) = match action {
        Action::CloseWorkspace => (vec!["workspace", "close", workspace.as_str()], "close"),
        Action::RemoveCheckout => (
            vec!["worktree", "remove", "--workspace", workspace.as_str()],
            "remove",
        ),
        Action::RequestMerge | Action::ForceRemoveCheckout => {
            return Err(error("Not a cleanup action"));
        }
    };
    registry.entries[index]["cleanup_pending"] = json!(operation);
    registry.entries[index]["cleanup_source_tip"] = json!(verified.source_tip);
    registry.save()?;
    let bytes = executor.run(&args)?;
    registry.entries[index]["cleanup_response"] = json!(String::from_utf8_lossy(&bytes));
    registry.save()?;
    // Even a successful transport is not proof: independently verify topology.
    envelope(&bytes)?;
    confirm_cleanup(registry, index, executor)
}

fn remove_closed_checkout(
    registry: &mut Registry,
    index: usize,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let entry = registry.entries[index].clone();
    if entry["phase"] != "active" || entry["workspace_closed"] != true {
        return Err(error(
            "Checkout removal requires a closed managed workspace",
        ));
    }
    if entry["cleanup_pending"].is_string() {
        return Err(error(
            "Checkout removal needs recovery; refresh before retrying",
        ));
    }
    if registry.entries.iter().any(|child| {
        child["parent_bead_id"] == entry["bead_id"] && child["checkout_removed"] != true
    }) {
        return Err(error("Remove tracked child checkouts first"));
    }

    let verified = verify(&registry.repo, &entry)?;
    task_commits(&registry.repo, &entry, &verified.source_tip)?;
    if !ancestor(
        &registry.repo.main_root,
        &verified.source_tip,
        &verified.target_tip,
    )? {
        return Err(error(
            "Current source is not integrated; checkout removal refused",
        ));
    }
    requested_merge_tips_in_source(&registry.repo, &entry, &verified.source_tip)?;

    ensure_closed_checkout_in_herdr(&registry.repo, &entry, executor)?;
    let checkout = Path::new(field(&entry, "path")?);

    let cwd = std::env::current_dir()?.canonicalize()?;
    if cwd.starts_with(&verified.source) {
        return Err(error(
            "Cannot remove the checkout running this OnTrack process",
        ));
    }
    let bead_id = field(&entry, "bead_id")?.to_owned();
    let bead = executor.show_bead(&bead_id)?;
    if bead["id"] != bead_id || bead["status"] != "closed" {
        close_integrated(registry, index, executor, false)?;
    }

    let fresh = verify(&registry.repo, &entry)?;
    if fresh.source_tip != verified.source_tip || fresh.target_tip != verified.target_tip {
        return Err(error(
            "Git tips changed during checkout validation; nothing removed",
        ));
    }
    ensure_closed_checkout_in_herdr(&registry.repo, &entry, executor)?;
    let final_git = verify(&registry.repo, &entry)?;
    if final_git.source_tip != verified.source_tip || final_git.target_tip != verified.target_tip {
        return Err(error(
            "Git tips changed during Herdr validation; nothing removed",
        ));
    }
    registry.entries[index]["cleanup_pending"] = json!("remove-closed-git");
    registry.entries[index]["cleanup_source_tip"] = json!(verified.source_tip);
    registry.entries[index]["detail"] = json!(
        "Git checkout removal may have started; refresh to verify before any retry. The branch is preserved."
    );
    registry.save()?;

    let path = checkout
        .to_str()
        .ok_or_else(|| error("Non UTF-8 checkout path"))?;
    let output = git_output(
        &registry.repo.main_root,
        &["worktree", "remove", "--", path],
    );
    match output {
        Ok(output) if output.status.success() => confirm_cleanup(registry, index, executor),
        Ok(output) => {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            registry.entries[index]["detail"] = json!(if message.is_empty() {
                "Git did not remove the checkout. Refresh to verify its state before retrying."
                    .to_owned()
            } else {
                format!("Git could not remove the checkout: {message}")
            });
            registry.save()?;
            Err(error(
                registry.entries[index]["detail"]
                    .as_str()
                    .unwrap_or_default(),
            ))
        }
        Err(error) => {
            registry.entries[index]["detail"] = json!(format!(
                "Git checkout removal result is unknown: {error}. Refresh to verify before any retry."
            ));
            registry.save()?;
            Err(error)
        }
    }
}

fn ensure_closed_checkout_in_herdr(
    repo: &RepositoryIdentity,
    entry: &Value,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let inventory = worktree_inventory(repo, entry, executor)?;
    let rows = inventory["worktrees"]
        .as_array()
        .ok_or_else(|| error("Missing Herdr worktree verification rows"))?;
    let recorded_path = field(entry, "path")?;
    let source = Path::new(recorded_path);
    let mut matches = 0;
    for row in rows {
        let Some(path) = row["path"].as_str() else {
            continue;
        };
        let matches_source = path == recorded_path
            || (source.exists()
                && Path::new(path)
                    .canonicalize()
                    .is_ok_and(|path| path == source));
        if matches_source {
            matches += 1;
            if row["branch"] != entry["branch"] {
                return Err(error(
                    "Herdr checkout branch does not match the recorded branch",
                ));
            }
            if row["open_workspace_id"].is_string() {
                return Err(error("Herdr still reports this checkout as open"));
            }
        }
    }
    if source.exists() && matches != 1 {
        return Err(error(
            "Herdr did not identify exactly one recorded checkout",
        ));
    }
    if !source.exists() && matches != 0 {
        return Err(error("Herdr still lists the removed checkout"));
    }
    Ok(())
}

fn worktree_inventory(
    repo: &RepositoryIdentity,
    entry: &Value,
    executor: &mut impl Executor,
) -> io::Result<Value> {
    let result = envelope(
        &executor.run(&[
            "worktree",
            "list",
            "--cwd",
            repo.main_root
                .to_str()
                .ok_or_else(|| error("Non UTF-8 main checkout"))?,
        ])?,
    )?;
    if result["type"] != "worktree_list" {
        return Err(error("Unexpected Herdr worktree listing; cleanup refused"));
    }
    let reported_root = Path::new(field(&result["source"], "source_checkout_path")?);
    if !reported_root.is_absolute()
        || reported_root.canonicalize()? != repo.main_root
        || result["source"]["source_workspace_id"] != entry["source_workspace"]
    {
        return Err(error(
            "Herdr source checkout/workspace relationship changed; cleanup refused",
        ));
    }
    let rows = result["worktrees"]
        .as_array()
        .ok_or_else(|| error("Missing worktree verification rows"))?;
    let main_rows: Vec<_> = rows
        .iter()
        .filter(|row| row["open_workspace_id"] == entry["source_workspace"])
        .collect();
    if main_rows.len() != 1
        || main_rows[0]["branch"] != default_branch(repo)?
        || Path::new(field(main_rows[0], "path")?).canonicalize()? != repo.main_root
    {
        return Err(error(
            "Herdr primary checkout/source workspace ownership changed; cleanup refused",
        ));
    }
    Ok(result)
}

fn verify_open_workspace(
    repo: &RepositoryIdentity,
    entry: &Value,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let result = worktree_inventory(repo, entry, executor)?;
    let rows = result["worktrees"]
        .as_array()
        .ok_or_else(|| error("Missing worktree verification rows"))?;
    let source = Path::new(field(entry, "path")?);
    let ownership: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["open_workspace_id"] == entry["workspace_id"]
                || row["path"].as_str().is_some_and(|path| {
                    Path::new(path)
                        .canonicalize()
                        .is_ok_and(|path| path == source)
                })
        })
        .collect();
    if entry["workspace_id"] == entry["source_workspace"] || ownership.len() != 1 {
        return Err(error(
            "Recorded workspace no longer uniquely owns the managed checkout; nothing closed or removed",
        ));
    }
    let row = ownership[0];
    let path = Path::new(field(row, "path")?);
    if row["open_workspace_id"] != entry["workspace_id"]
        || row["branch"] != entry["branch"]
        || !path.is_absolute()
        || path.canonicalize()? != source
    {
        return Err(error(
            "Recorded workspace ID/path/branch ownership changed; nothing closed or removed",
        ));
    }
    Ok(())
}

fn confirm_cleanup(
    registry: &mut Registry,
    index: usize,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let entry = &registry.entries[index];
    let operation = field(entry, "cleanup_pending")?.to_owned();
    if operation == "remove-closed-git" {
        return confirm_closed_git_removal(registry, index, executor);
    }
    let result = worktree_inventory(&registry.repo, entry, executor)?;
    let rows = result["worktrees"]
        .as_array()
        .ok_or_else(|| error("Missing worktree verification rows"))?;
    if rows
        .iter()
        .any(|row| row["open_workspace_id"] == entry["workspace_id"])
    {
        return Err(error(
            "Workspace still open; cleanup unconfirmed, inspect before recovery",
        ));
    }
    let source_ref = format!("refs/heads/{}", field(entry, "branch")?);
    let current_source = tip(&registry.repo.main_root, &source_ref)?;
    if operation != "force-remove" {
        task_commits(&registry.repo, entry, &current_source)?;
        let target = target_checkout(&registry.repo, entry)?;
        if !ancestor(
            &registry.repo.main_root,
            &current_source,
            &tip(&target, "HEAD")?,
        )? {
            return Err(error(
                "Source advanced during cleanup; new work is outstanding",
            ));
        }
    }
    if operation == "remove" || operation == "force-remove" {
        let path = Path::new(field(entry, "path")?);
        if !checkout_missing(entry)? || rows.iter().any(|row| row["path"] == entry["path"]) {
            return Err(error("Checkout removal unconfirmed; checkout still exists"));
        }
        let inventory = git(
            &registry.repo.main_root,
            &["worktree", "list", "--porcelain", "-z"],
        )?;
        if inventory
            .split('\0')
            .any(|line| line.strip_prefix("worktree ") == path.to_str())
        {
            return Err(error(
                "Checkout remains registered in Git; removal unconfirmed",
            ));
        }
        registry.entries[index]["checkout_removed"] = json!(true);
    } else {
        verify(&registry.repo, entry)?;
    }
    if operation == "force-remove" {
        registry.entries[index]["force_removed"] = json!(true);
    }
    registry.entries[index]["workspace_closed"] = json!(true);
    registry.entries[index]["cleanup_pending"] = Value::Null;
    registry.entries[index]["detail"] = Value::Null;
    registry.save()
}

fn confirm_closed_git_removal(
    registry: &mut Registry,
    index: usize,
    executor: &mut impl Executor,
) -> io::Result<()> {
    let entry = registry.entries[index].clone();
    let repo = &registry.repo;
    let path = Path::new(field(&entry, "path")?);
    let branch_ref = format!("refs/heads/{}", field(&entry, "branch")?);
    let source_tip = tip(&repo.main_root, &branch_ref)?;
    let target = target_checkout_identity(repo, &entry)?;
    let target_tip = tip(&target, "HEAD")?;
    task_commits(repo, &entry, &source_tip)?;
    if !ancestor(&repo.main_root, &source_tip, &target_tip)? {
        return Err(error(
            "Source is no longer integrated; checkout removal is unconfirmed",
        ));
    }
    if !ancestor(
        &repo.main_root,
        field(&entry, "cleanup_source_tip")?,
        &source_tip,
    )? {
        return Err(error("Preserved branch changed during checkout removal"));
    }

    let registered = git(&repo.main_root, &["worktree", "list", "--porcelain", "-z"])?
        .split('\0')
        .any(|line| line.strip_prefix("worktree ") == path.to_str());
    if checkout_missing(&entry)? && !registered {
        ensure_closed_checkout_in_herdr(repo, &entry, executor)?;
        registry.entries[index]["checkout_removed"] = json!(true);
        registry.entries[index]["cleanup_pending"] = Value::Null;
        registry.entries[index]["cleanup_response"] =
            json!("git worktree remove -- <recorded path>");
        registry.entries[index]["detail"] = Value::Null;
        registry.save()?;
        return Ok(());
    }

    if path.exists() && registered {
        let verified = verify(repo, &entry)?;
        if verified.source_tip != source_tip {
            return Err(error(
                "Checkout changed during removal; inspect before retrying",
            ));
        }
        ensure_closed_checkout_in_herdr(repo, &entry, executor)?;
        registry.entries[index]["cleanup_pending"] = Value::Null;
        registry.entries[index]["cleanup_source_tip"] = Value::Null;
        registry.entries[index]["detail"] = Value::Null;
        registry.save()?;
        return Ok(());
    }

    Err(error(
        "Checkout removal is unresolved; path and Git registration do not agree",
    ))
}

pub fn act(id: &str, action: Action) -> io::Result<ManagedWorktree> {
    act_with(&std::env::current_dir()?, id, action, &mut SystemExecutor)
}

fn act_with(
    cwd: &Path,
    id: &str,
    action: Action,
    executor: &mut impl Executor,
) -> io::Result<ManagedWorktree> {
    let mut registry = Registry::open(cwd)?;
    let index = registry.index(id)?;
    let result = match action {
        Action::RequestMerge => {
            if registry.entries[index]["merge_request"].is_object() {
                // An explicit retry of the active request may be needed after
                // the target advances. It retains the active queue slot.
                request_merge(&mut registry, index, executor, false)
            } else {
                if registry.entries[index]["merge_queue"].is_object() {
                    registry.entries[index]["merge_queue"]["blocked"] = Value::Null;
                    registry.entries[index]["merge_queue"]["provenance"] = json!("manual");
                    registry.entries[index]["detail"] = Value::Null;
                    registry.save()?;
                } else {
                    queue_merge(&mut registry, index, executor, false)?;
                }
                dispatch_next_merge(&mut registry, executor)
            }
        }
        Action::ForceRemoveCheckout => force_remove(&mut registry, index, executor),
        _ => cleanup(&mut registry, index, action, executor),
    };
    if let Err(e) = result {
        registry.entries[index]["detail"] = json!(e.to_string());
        registry.save()?;
        return Err(e);
    }
    Ok(inspect(&registry.repo, &registry.entries[index]))
}

pub fn reconcile(policy: CleanupPolicy) -> io::Result<Vec<ManagedWorktree>> {
    reconcile_with(&std::env::current_dir()?, policy, &mut SystemExecutor)
}

fn reconcile_with(
    cwd: &Path,
    policy: CleanupPolicy,
    executor: &mut impl Executor,
) -> io::Result<Vec<ManagedWorktree>> {
    let mut registry = Registry::open(cwd)?;
    for index in 0..registry.entries.len() {
        let result = (|| {
            if registry.entries[index]["phase"] != "active"
                || registry.entries[index]["checkout_removed"] == true
                || registry.entries[index]["restart_pending"] == true
            {
                return Ok(());
            }
            // Read-only confirmation recovers successful-but-timed-out cleanup;
            // it NEVER retries the close/remove command.
            if registry.entries[index]["cleanup_pending"].is_string() {
                return confirm_cleanup(&mut registry, index, executor);
            }
            // External removal is a checkout-existence observation, not a
            // claim that the preserved source branch was integrated.
            if checkout_missing(&registry.entries[index])? {
                registry.entries[index]["checkout_removed"] = json!(true);
                registry.entries[index]["checkout_removed_externally"] = json!(true);
                registry.save()?;
                return Ok(());
            }
            let status = inspect(&registry.repo, &registry.entries[index]).status;
            if matches!(status, WorktreeStatus::MergeRequested) {
                finalize(&mut registry, index, executor)?;
            }
            let status = inspect(&registry.repo, &registry.entries[index]).status;
            if status == WorktreeStatus::WorkspaceClosed {
                // A closed workspace requires a fresh explicit removal confirmation.
                return Ok(());
            }
            let mut closed_now = false;
            if status == WorktreeStatus::Integrated {
                let entry = &registry.entries[index];
                let bead = executor.show_bead(field(entry, "bead_id")?)?;
                if bead["id"] != entry["bead_id"] {
                    return Err(error("Live Bead identity changed"));
                }
                if bead["status"] != "closed" {
                    if !crate::beads::ready_to_merge_record(&bead) {
                        return Ok(());
                    }
                    close_integrated(&mut registry, index, executor, false)?;
                    closed_now = true;
                }
            }
            let auto_merge = if registry.entries[index]["parent_bead_id"].as_str().is_some() {
                policy.auto_merge_children
            } else {
                policy.auto_merge
            };
            if !(auto_merge || policy.auto_close_workspace || policy.auto_remove_checkout) {
                return Ok(());
            }
            if !matches!(
                status,
                WorktreeStatus::Unmerged | WorktreeStatus::Integrated
            ) {
                return Ok(());
            }
            if (status == WorktreeStatus::Unmerged && !auto_merge)
                || (status == WorktreeStatus::Integrated
                    && !(policy.auto_close_workspace || policy.auto_remove_checkout))
            {
                return Ok(());
            }
            let entry = &registry.entries[index];
            let verified = verify(&registry.repo, entry)?;
            task_commits(&registry.repo, entry, &verified.source_tip)?;
            let bead = executor.show_bead(field(entry, "bead_id")?)?;
            if bead["id"] != entry["bead_id"] || !merge_ready(&bead) {
                return Ok(());
            }
            // Only open workspaces reach this agent gate; closed workspace
            // removal is an explicit action and does not query a departed agent.
            if !closed_now || registry.entries[index]["merge_request"]["provenance"] == "manual" {
                let agent = settled(executor, entry, true)?;
                bind_late_session(&mut registry, index, &agent)?;
            }
            if status == WorktreeStatus::Unmerged {
                if auto_merge
                    && registry.entries[index]["merge_request"].is_null()
                    && registry.entries[index]["merge_queue"].is_null()
                {
                    queue_merge(&mut registry, index, executor, true)?;
                }
            } else if policy.auto_remove_checkout {
                // Removal closes the workspace too. Never close first when both enabled.
                cleanup(&mut registry, index, Action::RemoveCheckout, executor)?;
            } else if policy.auto_close_workspace && status == WorktreeStatus::Integrated {
                cleanup(&mut registry, index, Action::CloseWorkspace, executor)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            registry.entries[index]["detail"] = json!(e.to_string());
            registry.save()?;
        }
    }
    if let Err(e) = dispatch_next_merge(&mut registry, executor) {
        // The queued entry carries the recovery detail. A later explicit
        // request can unblock it after the underlying condition is fixed.
        let _ = e;
    }
    let repo = registry.repo.clone();
    let entries = registry.entries.clone();
    drop(registry);
    Ok(entries
        .iter()
        .map(|entry| with_agent_report(inspect_for_display(&repo, entry, executor), entry))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    const ID: &str = "ot-test";
    const BRANCH: &str = "ontrack/ot-test";

    #[test]
    fn managed_board_moves_share_registry_lock_and_preserve_backward_state() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut registry = Registry::open(&fixture.main).unwrap();
        for cwd in [&fixture.main, &fixture.source] {
            let result = crate::beads::move_bead_with(
                cwd,
                "Bead-1",
                "in_progress",
                |_| panic!("A contended move must fail before reading Beads"),
                |_| panic!("A contended move must not mutate Beads"),
            );
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
        }
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "worktrees::tests::registry_move_child"])
            .env("ONTRACK_TEST_MOVE_REPOSITORY", &fixture.main)
            .env("ONTRACK_TEST_MOVE_EXPECT_BUSY", "1")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        registry.entries[0]["move_interleaving"] = json!("preserved");
        registry.save().unwrap();
        drop(registry);

        let main = fixture.main.clone();
        let mut closing = Fake {
            hook: Some(Box::new(move |_| {
                let result = crate::beads::move_bead_with(
                    &main,
                    "Bead-1",
                    "in_progress",
                    |_| panic!("A move must not read during closure's agent inspection"),
                    |_| panic!("A move must not write during closure's agent inspection"),
                );
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
            })),
            ..Fake::default()
        };
        closing.json(fixture.agent(12, "done"));
        reconcile_with(&fixture.main, CleanupPolicy::default(), &mut closing).unwrap();
        assert!(closing.closed.contains("Bead-1"));

        let live = std::cell::RefCell::new(
            json!({"id":"Bead-1", "status":"in_progress", "labels":[crate::beads::READY_TO_MERGE_LABEL]}),
        );
        crate::beads::move_bead_with(
            &fixture.source,
            "Bead-1",
            "in_progress",
            |_| {
                assert_eq!(
                    Registry::open(&fixture.main).err().unwrap().kind(),
                    io::ErrorKind::WouldBlock
                );
                Ok(live.borrow().clone())
            },
            |args| {
                assert_eq!(
                    Registry::open(&fixture.main).err().unwrap().kind(),
                    io::ErrorKind::WouldBlock
                );
                assert!(args.contains(&"--if-status=in_progress"));
                assert!(args.contains(&"--remove-label=ontrack:ready-to-merge"));
                live.borrow_mut()["labels"] = json!([]);
                Ok(())
            },
        )
        .unwrap();
        let mut executor = Fake {
            bead: Some(live.into_inner()),
            ..Fake::default()
        };
        reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_close_workspace: true,
                auto_remove_checkout: true,
                ..CleanupPolicy::default()
            },
            &mut executor,
        )
        .unwrap();
        assert!(executor.closed.is_empty());
        assert!(executor.calls.iter().all(|args| args[0] == "show_bead"));
        assert_eq!(
            Registry::open(&fixture.main).unwrap().entries[0]["move_interleaving"],
            "preserved"
        );
        let before = fs::read(fixture.main.join(".git/ontrack-worktrees/registry.json")).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "worktrees::tests::registry_move_child"])
            .env("ONTRACK_TEST_MOVE_REPOSITORY", &fixture.main)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        assert_eq!(
            fs::read(fixture.main.join(".git/ontrack-worktrees/registry.json")).unwrap(),
            before
        );
    }

    #[test]
    fn registry_move_child() {
        let Some(repository) = std::env::var_os("ONTRACK_TEST_MOVE_REPOSITORY") else {
            return;
        };
        let busy = std::env::var_os("ONTRACK_TEST_MOVE_EXPECT_BUSY").is_some();
        let result = crate::beads::move_bead_with(
            Path::new(&repository),
            "Bead-1",
            "in_progress",
            |_| {
                assert!(
                    !busy,
                    "Move must not read while another process owns the registry"
                );
                Ok(
                    json!({"id":"Bead-1", "status":"in_progress", "labels":[crate::beads::READY_TO_MERGE_LABEL]}),
                )
            },
            |_| {
                assert!(
                    !busy,
                    "Move must not write while another process owns the registry"
                );
                Ok(())
            },
        );
        if busy {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
        } else {
            result.unwrap();
        }
    }

    #[test]
    fn board_moves_without_registry_do_not_require_git_or_create_registry() {
        let fixture = Fixture::new();
        for cwd in [&fixture.main, &fixture.directory] {
            let updated = std::cell::Cell::new(false);
            crate::beads::move_bead_with(cwd, "Bead-1", "in_progress", |_| {
                Ok(json!({"id":"Bead-1", "status":"in_progress", "labels":[crate::beads::READY_TO_MERGE_LABEL]}))
            }, |_| { updated.set(true); Ok(()) }).unwrap();
            assert!(updated.get());
            assert!(!fixture.main.join(".git/ontrack-worktrees").exists());
        }
    }

    #[test]
    fn failed_bead_close_retries_after_integration_without_remerging_or_cleanup() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        fixture.request();
        let mut first = Fake {
            close_failures: 1,
            ..Fake::default()
        };
        first.json(fixture.agent(14, "done"));
        first.json(fixture.agent(14, "done"));
        let rows = reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_close_workspace: true,
                auto_remove_checkout: true,
                ..CleanupPolicy::default()
            },
            &mut first,
        )
        .unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert!(
            rows[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("dependency guard")
        );
        assert!(first.closed.is_empty());
        assert!(fixture.source.exists());
        assert!(!first.calls.iter().any(|args| {
            args.get(1)
                .is_some_and(|s| s == "prompt" || s == "close" || s == "remove")
        }));
        let integrated_tip = tip(&fixture.main, "HEAD").unwrap();

        let mut retry = Fake::default();
        retry.json(fixture.agent(14, "done"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut retry).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert!(rows[0].detail.is_none());
        assert!(retry.closed.contains("Bead-1"));
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), integrated_tip);
        assert_eq!(
            retry
                .calls
                .iter()
                .filter(|args| args[0] == "close_bead")
                .count(),
            1
        );
        assert!(fixture.source.exists());
    }

    #[test]
    fn integrated_work_moved_backward_is_not_closed_or_cleaned() {
        let fixture = Fixture::managed();
        fixture.integrated();
        for status in ["open", "blocked", "in_progress", "deferred"] {
            let mut executor = Fake {
                bead: Some(json!({"id":"Bead-1", "status":status})),
                ..Fake::default()
            };
            let rows = reconcile_with(
                &fixture.main,
                CleanupPolicy {
                    auto_merge: true,
                    auto_close_workspace: true,
                    auto_remove_checkout: true,
                    ..CleanupPolicy::default()
                },
                &mut executor,
            )
            .unwrap();
            assert_eq!(rows[0].status, WorktreeStatus::Integrated);
            assert!(executor.closed.is_empty());
            assert!(executor.calls.iter().all(|args| args[0] == "show_bead"));
            assert!(fixture.source.exists());
        }
    }

    #[test]
    fn explicit_close_can_close_backward_moved_work_but_still_requires_integration() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut executor = Fake {
            bead: Some(json!({"id":"Bead-1", "status":"open"})),
            ..Fake::default()
        };
        executor.json(fixture.agent(12, "done"));
        close_integrated_bead_with(&fixture.main, "Bead-1", &mut executor).unwrap();
        assert!(executor.closed.contains("Bead-1"));
        assert!(fixture.source.exists());
    }

    #[test]
    fn legacy_closed_integration_is_not_reopened_or_closed_again() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut executor = Fake::legacy_closed();
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert!(executor.calls.iter().all(|args| args[0] == "show_bead"));
    }

    #[test]
    fn explicit_bead_close_requires_tracked_integrated_task_commits_and_finished_children() {
        let fixture = Fixture::managed();
        let mut executor = Fake::default();
        assert!(close_integrated_bead_with(&fixture.main, "untracked", &mut executor).is_err());
        assert!(
            close_integrated_bead_with(&fixture.main, "Bead-1", &mut executor)
                .unwrap_err()
                .to_string()
                .contains("No task commits")
        );
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        assert!(
            close_integrated_bead_with(&fixture.main, "Bead-1", &mut executor)
                .unwrap_err()
                .to_string()
                .contains("not integrated")
        );
        git(&fixture.main, &["merge", "--ff-only", BRANCH]).unwrap();
        executor.open_children.push("Bead-1.1".into());
        assert!(
            close_integrated_bead_with(&fixture.main, "Bead-1", &mut executor)
                .unwrap_err()
                .to_string()
                .contains("Child tasks still open")
        );
        executor.open_children.clear();
        executor.json(fixture.agent(12, "done"));
        close_integrated_bead_with(&fixture.main, "Bead-1", &mut executor).unwrap();
        assert!(executor.closed.contains("Bead-1"));
        assert!(fixture.source.exists());
    }

    #[test]
    fn closure_rechecks_live_marker_after_agent_observation() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut executor = Fake::default();
        executor.bead_reads.push_back(json!({"id":"Bead-1", "status":"in_progress", "labels":[crate::beads::READY_TO_MERGE_LABEL]}));
        executor.bead = Some(json!({"id":"Bead-1", "status":"in_progress"}));
        executor.json(fixture.agent(12, "done"));
        assert!(close_integrated_bead_with(&fixture.main, "Bead-1", &mut executor).is_err());
        assert!(executor.closed.is_empty());
        assert!(fixture.source.exists());
    }

    #[test]
    fn manual_merges_wait_in_durable_order_and_dispatch_after_integration() {
        let fixture = Fixture::managed();
        let second = fixture.second_managed();
        Fixture::commit_at(&fixture.source, "first-task", "first\n");
        Fixture::commit_at(&second, "second-task", "second\n");
        fixture.request();

        let mut no_prompt = Fake::default();
        let queued = act_with(
            &fixture.main,
            "ot-second",
            Action::RequestMerge,
            &mut no_prompt,
        )
        .unwrap();
        assert_eq!(queued.status, WorktreeStatus::MergeQueued);
        assert!(no_prompt.calls.is_empty());
        // Reopening the registry models an OnTrack restart.
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[1]["merge_queue"]["order"], 2);
        assert_eq!(registry.entries[0]["merge_request"]["queue_order"], 1);
        drop(registry);

        let mut next = Fake::default();
        next.json(fixture.agent(14, "done"));
        next.json(fixture.agent(14, "done"));
        next.json(fixture.agent(14, "done"));
        next.json(fixture.second_agent(&second, 12, "done"));
        next.json(json!({"result":{"type":"agent_prompted", "agent":{
            "pane_id":"wSecond:p1", "workspace_id":"wSecond"}}}));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut next).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert_eq!(rows[1].status, WorktreeStatus::MergeRequested);
        let prompts: Vec<_> = next
            .calls
            .iter()
            .filter(|args| args.get(1).is_some_and(|s| s == "prompt"))
            .collect();
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0][2], "wSecond:p1");
        let current_target = tip(&fixture.main, "HEAD").unwrap();
        assert!(prompts[0][3].contains(&current_target));
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["merge_request"]["queue_released"], true);
    }

    #[test]
    fn automatic_reconciliation_queues_multiple_ready_merges_but_prompts_one() {
        let fixture = Fixture::managed();
        let second = fixture.second_managed();
        Fixture::commit_at(&fixture.source, "first-task", "first\n");
        Fixture::commit_at(&second, "second-task", "second\n");
        let mut executor = Fake::default();
        executor.json(fixture.agent(12, "done"));
        executor.json(fixture.second_agent(&second, 12, "done"));
        executor.json(fixture.agent(12, "done"));
        executor.json(fixture.prompt_result());
        let rows = reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_merge: true,
                ..CleanupPolicy::default()
            },
            &mut executor,
        )
        .unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::MergeRequested);
        assert_eq!(rows[1].status, WorktreeStatus::MergeQueued);
        assert_eq!(
            executor
                .calls
                .iter()
                .filter(|args| args.get(1).is_some_and(|s| s == "prompt"))
                .count(),
            1
        );
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            registry.entries[1]["merge_queue"]["provenance"],
            "automatic"
        );
    }

    #[test]
    fn unresolved_active_merge_holds_later_queue_without_reprompting() {
        let fixture = Fixture::managed();
        let second = fixture.second_managed();
        Fixture::commit_at(&fixture.source, "first-task", "first\n");
        Fixture::commit_at(&second, "second-task", "second\n");
        fixture.request();
        act_with(
            &fixture.main,
            "ot-second",
            Action::RequestMerge,
            &mut Fake::default(),
        )
        .unwrap();
        Fixture::commit_at(&fixture.main, "target-change", "advanced\n");
        let mut executor = Fake::default();
        executor.json(fixture.agent(13, "working"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::MergeRequested);
        assert_eq!(rows[1].status, WorktreeStatus::MergeQueued);
        assert!(
            !executor
                .calls
                .iter()
                .any(|args| args.get(1).is_some_and(|s| s == "prompt"))
        );
    }

    #[test]
    fn integrated_queued_entry_is_cleared_while_active_request_still_holds_queue() {
        let fixture = Fixture::managed();
        let second = fixture.second_managed();
        Fixture::commit_at(&fixture.source, "first-task", "first\n");
        Fixture::commit_at(&second, "second-task", "second\n");
        fixture.request();
        assert_eq!(
            act_with(
                &fixture.main,
                "ot-second",
                Action::RequestMerge,
                &mut Fake::default(),
            )
            .unwrap()
            .status,
            WorktreeStatus::MergeQueued
        );

        git(&fixture.main, &["merge", "--ff-only", "ontrack/ot-second"]).unwrap();
        let mut executor = Fake::default();
        let mut registry = Registry::open(&fixture.main).unwrap();
        dispatch_next_merge(&mut registry, &mut executor).unwrap();
        assert!(registry.entries[1]["merge_queue"].is_null());
        assert!(registry.entries[1]["detail"].is_null());
        assert!(registry.entries[0]["merge_request"].is_object());
        assert!(
            !executor
                .calls
                .iter()
                .any(|args| args.get(1).is_some_and(|s| s == "prompt"))
        );
    }

    #[test]
    fn externally_integrated_branch_waits_for_prompted_agent_to_settle() {
        let fixture = Fixture::managed();
        let second = fixture.second_managed();
        Fixture::commit_at(&fixture.source, "first-task", "first\n");
        Fixture::commit_at(&second, "second-task", "second\n");
        fixture.request();
        act_with(
            &fixture.main,
            "ot-second",
            Action::RequestMerge,
            &mut Fake::default(),
        )
        .unwrap();
        git(&fixture.main, &["merge", "--ff-only", BRANCH]).unwrap();
        let mut working = Fake::default();
        working.json(fixture.agent(13, "working"));
        working.json(fixture.agent(13, "working"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut working).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert_eq!(rows[1].status, WorktreeStatus::MergeQueued);
        assert!(
            !working
                .calls
                .iter()
                .any(|args| args.get(1).is_some_and(|s| s == "prompt"))
        );
    }

    #[test]
    fn uncertain_prompt_holds_queue_even_if_git_was_integrated_externally() {
        let fixture = Fixture::managed();
        let second = fixture.second_managed();
        Fixture::commit_at(&fixture.source, "first-task", "first\n");
        Fixture::commit_at(&second, "second-task", "second\n");
        let mut uncertain = Fake::default();
        uncertain.json(fixture.agent(12, "idle"));
        uncertain.outputs.push_back(Err(error("prompt timed out")));
        assert!(act_with(&fixture.main, ID, Action::RequestMerge, &mut uncertain).is_err());
        git(&fixture.main, &["merge", "--ff-only", BRANCH]).unwrap();
        let queued = act_with(
            &fixture.main,
            "ot-second",
            Action::RequestMerge,
            &mut Fake::default(),
        )
        .unwrap();
        assert_eq!(queued.status, WorktreeStatus::MergeQueued);
        let mut no_prompt = Fake::default();
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut no_prompt).unwrap();
        assert_eq!(rows[1].status, WorktreeStatus::MergeQueued);
        assert!(no_prompt.calls.iter().all(|call| call[0] == "show_bead"));
    }

    #[test]
    fn child_handoff_targets_parent_checkout_and_never_default_branch() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "parent-task", "parent work\n");
        let child = json!({"id":"Bead-1.1", "title":"Child", "parent":"Bead-1"});
        let child_branch = "ontrack/child-task";
        let child_path = fixture.directory.join("child");
        let mut journal = LaunchJournal::begin(
            &fixture.main,
            &child,
            child_branch,
            "opencode",
            "child-task",
            "wMain",
        )
        .unwrap();
        assert_eq!(journal.base_branch(), Some(BRANCH));
        git(
            &fixture.main,
            &[
                "worktree",
                "add",
                "-b",
                child_branch,
                child_path.to_str().unwrap(),
                BRANCH,
            ],
        )
        .unwrap();
        let created = json!({"workspace":{"workspace_id":"wChild"},
            "root_pane":{"pane_id":"wChild:p1"},
            "worktree":{"path":child_path,"branch":child_branch}});
        journal.created(&created).unwrap();
        journal.phase("active").unwrap();
        drop(journal);
        Fixture::commit_at(&child_path, "child-task", "child work\n");
        let default_tip = tip(&fixture.main, "HEAD").unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        let entry = &registry.entries[1];
        assert_eq!(entry["target_branch"], BRANCH);
        assert_eq!(entry["target_path"], json!(fixture.source));
        assert_eq!(
            inspect(&registry.repo, entry).status,
            WorktreeStatus::Unmerged
        );
        drop(registry);
        let parent_merge = act_with(
            &fixture.main,
            ID,
            Action::RequestMerge,
            &mut Fake::default(),
        )
        .err()
        .unwrap();
        assert!(
            parent_merge
                .to_string()
                .contains("Child worktrees still need integration"),
            "{parent_merge}"
        );
        let agent = json!({"result":{"type":"agent_info", "agent":{
            "pane_id":"wChild:p1", "workspace_id":"wChild", "agent":"opencode",
            "agent_status":"idle", "cwd":child_path, "foreground_cwd":child_path,
            "state_change_seq":12
        }}});
        let mut request = Fake::default();
        request.json(agent.clone());
        request.json(json!({"result":{"type":"agent_prompted", "agent":{
            "pane_id":"wChild:p1", "workspace_id":"wChild"}}}));
        assert_eq!(
            act_with(
                &fixture.main,
                "child-task",
                Action::RequestMerge,
                &mut request
            )
            .unwrap()
            .status,
            WorktreeStatus::MergeRequested
        );
        let mut finish = Fake {
            bead: Some(json!({"id":"Bead-1","status":"open"})),
            ..Fake::default()
        };
        finish.json(agent);
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut finish).unwrap();
        assert_eq!(rows[1].status, WorktreeStatus::Integrated);
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[1]).status,
            WorktreeStatus::Integrated
        );
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), default_tip);
    }

    #[test]
    fn removed_child_stays_removed_after_parent_checkout_is_removed() {
        let fixture = Fixture::managed();
        let child_path = fixture.directory.join("child");
        let child_branch = "ontrack/child-task";
        let child = json!({"id":"Bead-1.1", "title":"Child", "parent":"Bead-1"});
        let mut journal = LaunchJournal::begin(
            &fixture.main,
            &child,
            child_branch,
            "opencode",
            "child-task",
            "wMain",
        )
        .unwrap();
        git(
            &fixture.main,
            &[
                "worktree",
                "add",
                "-b",
                child_branch,
                child_path.to_str().unwrap(),
                BRANCH,
            ],
        )
        .unwrap();
        let created = json!({"workspace":{"workspace_id":"wChild"},
            "root_pane":{"pane_id":"wChild:p1"},
            "worktree":{"path":child_path,"branch":child_branch}});
        journal.created(&created).unwrap();
        journal.phase("active").unwrap();
        drop(journal);

        Fixture::commit_at(&child_path, "child-task", "child work\n");
        git(&fixture.source, &["merge", "--ff-only", child_branch]).unwrap();
        git(
            &fixture.main,
            &["worktree", "remove", child_path.to_str().unwrap()],
        )
        .unwrap();
        let child_tip = tip(&fixture.main, &format!("refs/heads/{child_branch}")).unwrap();
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[1]["checkout_removed"] = json!(true);
        registry.entries[1]["workspace_closed"] = json!(true);
        registry.save().unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[1]).status,
            WorktreeStatus::CheckoutRemoved
        );
        drop(registry);

        Fixture::commit_at(&fixture.source, "parent-task", "parent work\n");
        git(&fixture.main, &["merge", "--ff-only", BRANCH]).unwrap();
        git(
            &fixture.main,
            &["worktree", "remove", fixture.source.to_str().unwrap()],
        )
        .unwrap();
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]["checkout_removed"] = json!(true);
        registry.entries[0]["workspace_closed"] = json!(true);
        registry.save().unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[1]).status,
            WorktreeStatus::CheckoutRemoved
        );
        drop(registry);
        assert_eq!(
            list_with(&fixture.main, &mut Fake::default()).unwrap()[1].status,
            WorktreeStatus::CheckoutRemoved
        );

        let advanced_path = fixture.directory.join("advanced-child");
        git(
            &fixture.main,
            &[
                "worktree",
                "add",
                advanced_path.to_str().unwrap(),
                child_branch,
            ],
        )
        .unwrap();
        Fixture::commit_at(&advanced_path, "later-child", "new work\n");
        git(
            &fixture.main,
            &["worktree", "remove", advanced_path.to_str().unwrap()],
        )
        .unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[1]).status,
            WorktreeStatus::NeedsAttention
        );
        drop(registry);

        git(
            &fixture.main,
            &[
                "update-ref",
                &format!("refs/heads/{child_branch}"),
                &child_tip,
            ],
        )
        .unwrap();
        fs::create_dir(&child_path).unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[1]).status,
            WorktreeStatus::NeedsAttention
        );
        drop(registry);

        fs::remove_dir(&child_path).unwrap();
        git(&fixture.main, &["branch", "-D", BRANCH]).unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[1]).status,
            WorktreeStatus::NeedsAttention
        );
    }

    #[test]
    fn active_child_cannot_merge_without_parent_checkout() {
        let fixture = Fixture::managed();
        let child_path = fixture.directory.join("child");
        let child_branch = "ontrack/child-task";
        let child = json!({"id":"Bead-1.1", "title":"Child", "parent":"Bead-1"});
        let mut journal = LaunchJournal::begin(
            &fixture.main,
            &child,
            child_branch,
            "opencode",
            "child-task",
            "wMain",
        )
        .unwrap();
        git(
            &fixture.main,
            &[
                "worktree",
                "add",
                "-b",
                child_branch,
                child_path.to_str().unwrap(),
                BRANCH,
            ],
        )
        .unwrap();
        let created = json!({"workspace":{"workspace_id":"wChild"},
            "root_pane":{"pane_id":"wChild:p1"},
            "worktree":{"path":child_path,"branch":child_branch}});
        journal.created(&created).unwrap();
        journal.phase("active").unwrap();
        drop(journal);
        Fixture::commit_at(&child_path, "child-task", "child work\n");

        git(
            &fixture.main,
            &["worktree", "remove", fixture.source.to_str().unwrap()],
        )
        .unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[1]).status,
            WorktreeStatus::NeedsAttention
        );
        drop(registry);
        let mut executor = Fake::default();
        assert!(
            act_with(
                &fixture.main,
                "child-task",
                Action::RequestMerge,
                &mut executor
            )
            .is_err()
        );
        assert!(executor.calls.is_empty());
    }

    #[test]
    fn child_launch_requires_managed_parent_worktree() {
        let fixture = Fixture::new();
        let child = json!({"id":"Bead-1.1", "title":"Child", "parent":"Bead-1"});
        let error = LaunchJournal::begin(
            &fixture.main,
            &child,
            "ontrack/child",
            "codex",
            "child",
            "wMain",
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("parent worktree"));
    }

    #[test]
    fn reserved_parent_can_host_child_then_resume_without_second_worktree() {
        let fixture = Fixture::new();
        let mut journal =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        git(
            &fixture.main,
            &[
                "worktree",
                "add",
                "-b",
                BRANCH,
                fixture.source.to_str().unwrap(),
            ],
        )
        .unwrap();
        let created = json!({"workspace":{"workspace_id":"wTask"},
            "root_pane":{"pane_id":"wTask:p1"},
            "worktree":{"path":fixture.source,"branch":BRANCH}});
        journal.created(&created).unwrap();
        journal.phase("reserved").unwrap();
        drop(journal);
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            inspect(&registry.repo, &registry.entries[0]).status,
            WorktreeStatus::Reserved
        );
        drop(registry);
        let child = json!({"id":"Bead-1.1", "title":"Child", "parent":"Bead-1"});
        let child_journal = LaunchJournal::begin(
            &fixture.main,
            &child,
            "ontrack/child",
            "opencode",
            "child",
            "wMain",
        )
        .unwrap();
        assert_eq!(child_journal.base_branch(), Some(BRANCH));
        drop(child_journal);
        let resumed =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        assert_eq!(resumed.reserved_target().unwrap().0, "wTask");
    }

    fn test_bead() -> Value {
        json!({
            "id": "Bead-1",
            "title": "Fix the selected handoff",
            "description": "First line\nSecond line"
        })
    }

    #[test]
    fn interleaved_journals_preserve_both_latest_entries_and_captured_context() {
        let fixture = Fixture::new();
        let mut first =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        let mut second = LaunchJournal::begin(
            &fixture.main,
            &json!({"id":"Bead-2","title":"Second"}),
            "ontrack/second",
            "codex",
            "second",
            "wMain",
        )
        .unwrap();
        first.capture("create", b"first response").unwrap();
        second.phase("agent_start_possibly_sent").unwrap();
        first.failed("first recovery").unwrap();
        second.capture("start", b"second response").unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries.len(), 2);
        assert_eq!(registry.entries[0]["responses"]["create"], "first response");
        assert_eq!(
            registry.entries[0]["bead_description"],
            test_bead()["description"]
        );
        assert_eq!(registry.entries[0]["detail"], "first recovery");
        assert_eq!(registry.entries[1]["responses"]["start"], "second response");
        assert_eq!(registry.entries[1]["phase"], "agent_start_possibly_sent");
    }

    #[test]
    fn paused_real_launch_intent_does_not_prevent_second_journal_progress() {
        use std::sync::{Arc, Barrier, mpsc};
        let fixture = Fixture::new();
        let main = fixture.main.clone();
        let (tx, rx) = mpsc::channel();
        let gate = Arc::new(Barrier::new(2));
        let worker_gate = gate.clone();
        let first = std::thread::spawn(move || {
            let mut journal =
                LaunchJournal::begin(&main, &test_bead(), BRANCH, "opencode", ID, "wMain").unwrap();
            tx.send(()).unwrap();
            worker_gate.wait();
            journal.capture("create", b"first resumes").unwrap();
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut second = LaunchJournal::begin(
            &fixture.main,
            &json!({"id":"Bead-2"}),
            "ontrack/second",
            "opencode",
            "second",
            "wMain",
        )
        .unwrap();
        second.capture("create", b"second progressed").unwrap();
        gate.wait();
        first.join().unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["responses"]["create"], "first resumes");
        assert_eq!(
            registry.entries[1]["responses"]["create"],
            "second progressed"
        );
    }

    #[test]
    fn journal_contention_exhaustion_is_bounded_and_preserves_the_last_durable_phase() {
        let fixture = Fixture::new();
        let mut journal =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(
            journal
                .phase("agent_start_possibly_sent")
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(registry.entries[0]["phase"], "create_possibly_sent");
        drop(registry);
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["phase"], "create_possibly_sent");
    }

    #[test]
    fn two_reserved_parent_resumes_have_exactly_one_atomic_start_claim() {
        use std::sync::{Arc, Barrier};
        let fixture = Fixture::new();
        let mut initial =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        git(
            &fixture.main,
            &[
                "worktree",
                "add",
                "-b",
                BRANCH,
                fixture.source.to_str().unwrap(),
            ],
        )
        .unwrap();
        initial.created(&json!({"workspace":{"workspace_id":"wTask"},"root_pane":{"pane_id":"wTask:p1"},"worktree":{"path":fixture.source,"branch":BRANCH}})).unwrap();
        initial.phase("reserved").unwrap();
        let first =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        let second =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        let gate = Arc::new(Barrier::new(2));
        let workers: Vec<_> = [first, second]
            .into_iter()
            .map(|mut journal| {
                let gate = gate.clone();
                std::thread::spawn(move || {
                    gate.wait();
                    if journal.phase("agent_start_possibly_sent").is_ok() {
                        journal
                            .capture("start", b"only the winning handoff started")
                            .unwrap();
                        true
                    } else {
                        assert!(journal.failed("loser must not overwrite winner").is_err());
                        false
                    }
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| usize::from(worker.join().unwrap()))
                .sum::<usize>(),
            1
        );
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries.len(), 1);
        assert_eq!(registry.entries[0]["phase"], "agent_start_possibly_sent");
        assert_eq!(
            registry.entries[0]["responses"]["start"],
            "only the winning handoff started"
        );
        assert_ne!(
            registry.entries[0]["detail"],
            "loser must not overwrite winner"
        );
    }

    #[test]
    fn display_observations_validate_session_and_never_mutate_durable_status() {
        let fixture = Fixture::managed();
        let before = fs::read(fixture.main.join(".git/ontrack-worktrees/registry.json")).unwrap();
        for mode in [
            "working",
            "blocked",
            "idle",
            "done",
            "error",
            "unknown",
            "missing",
            "session",
            "pane",
            "poll_error",
        ] {
            let mut executor = Fake::default();
            let mut agent = fixture.agent(12, mode);
            match mode {
                "missing" => agent["result"]["agent"]["agent_status"] = Value::Null,
                "session" => {
                    agent["result"]["agent"]["agent_session"]["value"] = json!("replacement")
                }
                "pane" => agent["result"]["agent"]["pane_id"] = json!("other"),
                _ => {}
            }
            if mode == "poll_error" {
                executor.outputs.push_back(Err(error("offline")));
            } else {
                executor.json(agent);
            }
            let rows = observe_agents_with(&fixture.main, &mut executor).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, ID);
            assert!(rows[0].detail.contains("workspace wTask"));
            assert!(rows[0].detail.contains("pane wTask:p1"));
            assert!(rows[0].detail.contains(
                if matches!(mode, "missing" | "session" | "pane" | "poll_error") {
                    "unknown"
                } else {
                    mode
                }
            ));
            assert_eq!(rows[0].blocked, matches!(mode, "blocked" | "error"));
            assert_eq!(
                fs::read(fixture.main.join(".git/ontrack-worktrees/registry.json")).unwrap(),
                before
            );
        }
    }

    struct Fixture {
        directory: PathBuf,
        main: PathBuf,
        source: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = loop {
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let path = std::env::temp_dir().join(format!(
                    "ontrack-worktrees-{}-{nanos}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create worktree test fixture: {error}"),
                }
            };
            let main = directory.join("main");
            fs::create_dir(&main).unwrap();
            git(&main, &["init", "--initial-branch=trunk"]).unwrap();
            for (key, value) in [
                ("user.name", "OnTrack Test"),
                ("user.email", "test@example.invalid"),
                ("commit.gpgsign", "false"),
                ("core.hooksPath", "/dev/null"),
                ("init.defaultBranch", "wrong-guess"),
            ] {
                git(&main, &["config", "--local", key, value]).unwrap();
            }
            Self::commit_at(&main, "file", "base\n");
            let source = directory.join("source");
            Self {
                directory,
                main,
                source,
            }
        }

        fn commit_at(path: &Path, name: &str, contents: &str) {
            fs::write(path.join(name), contents).unwrap();
            git(path, &["add", "--", name]).unwrap();
            git(path, &["commit", "-m", "test change"]).unwrap();
        }

        fn managed() -> Self {
            let fixture = Self::new();
            let mut journal =
                LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                    .unwrap();
            git(
                &fixture.main,
                &[
                    "worktree",
                    "add",
                    "-b",
                    BRANCH,
                    fixture.source.to_str().unwrap(),
                ],
            )
            .unwrap();
            let created = json!({"workspace":{"workspace_id":"wTask"}, "root_pane":{"pane_id":"wTask:p1"}, "worktree":{"path":fixture.source, "branch":BRANCH}});
            journal
                .capture(
                    "create",
                    &json!({"result":created}).to_string().into_bytes(),
                )
                .unwrap();
            journal.created(&created).unwrap();
            journal
                .capture("ready", &fixture.agent(10, "idle").to_string().into_bytes())
                .unwrap();
            journal.phase("active").unwrap();
            drop(journal);
            fixture
        }

        fn second_managed(&self) -> PathBuf {
            let source = self.directory.join("second");
            let mut journal = LaunchJournal::begin(
                &self.main,
                &json!({"id":"Bead-2", "title":"Second task"}),
                "ontrack/ot-second",
                "opencode",
                "ot-second",
                "wMain",
            )
            .unwrap();
            git(
                &self.main,
                &[
                    "worktree",
                    "add",
                    "-b",
                    "ontrack/ot-second",
                    source.to_str().unwrap(),
                ],
            )
            .unwrap();
            let created = json!({"workspace":{"workspace_id":"wSecond"}, "root_pane":{"pane_id":"wSecond:p1"}, "worktree":{"path":source,"branch":"ontrack/ot-second"}});
            journal.created(&created).unwrap();
            journal
                .capture(
                    "ready",
                    self.second_agent(&source, 10, "idle")
                        .to_string()
                        .as_bytes(),
                )
                .unwrap();
            journal.phase("active").unwrap();
            source
        }

        fn second_agent(&self, source: &Path, seq: u64, state: &str) -> Value {
            json!({"result":{"type":"agent_info", "agent":{
                "pane_id":"wSecond:p1", "workspace_id":"wSecond", "agent":"opencode",
                "agent_status":state, "cwd":source, "foreground_cwd":source,
                "agent_session":{"agent":"opencode", "kind":"id", "source":"herdr:opencode", "value":"session-2"},
                "state_change_seq":seq
            }}})
        }

        fn agent(&self, seq: u64, state: &str) -> Value {
            json!({"result":{"type":"agent_info", "agent":{
                "pane_id":"wTask:p1", "workspace_id":"wTask", "agent":"opencode",
                "agent_status":state, "cwd":self.source, "foreground_cwd":self.source,
                "agent_session":{"agent":"opencode", "kind":"id", "source":"herdr:opencode", "value":"session-1"},
                "state_change_seq":seq
            }}})
        }

        fn late_session_agent(&self, seq: u64, state: &str) -> Value {
            let mut agent = self.agent(seq, state);
            agent["result"]["agent"]["terminal_id"] = json!("term-task");
            agent
        }

        fn simulate_legacy_late_session(&self) {
            let mut registry = Registry::open(&self.main).unwrap();
            let mut ready = self.late_session_agent(10, "idle");
            ready["result"]["agent"]
                .as_object_mut()
                .unwrap()
                .remove("agent_session");
            registry.entries[0]["responses"]["ready"] = json!(ready.to_string());
            registry.entries[0]["agent_session"] = Value::Null;
            registry.entries[0]["initial_lifecycle_seq"] = Value::Null;
            registry.entries[0]["initial_terminal_id"] = Value::Null;
            registry.save().unwrap();
        }

        fn prompt_result(&self) -> Value {
            json!({"result":{"type":"agent_prompted", "agent":{"pane_id":"wTask:p1", "workspace_id":"wTask"}}})
        }

        fn inventory(&self, open: bool, checkout: bool) -> Value {
            let mut rows =
                vec![json!({"path":self.main, "branch":"trunk", "open_workspace_id":"wMain"})];
            if checkout {
                rows.push(json!({"path":self.source, "branch":BRANCH, "open_workspace_id":if open {json!("wTask")} else {Value::Null}}));
            }
            json!({"result":{"type":"worktree_list", "source":{"source_checkout_path":self.main, "source_workspace_id":"wMain"}, "worktrees":rows}})
        }

        fn status(&self) -> ManagedWorktree {
            let registry = Registry::open(&self.main).unwrap();
            inspect(&registry.repo, &registry.entries[0])
        }

        fn request(&self) {
            let mut executor = Fake::default();
            executor.json(self.agent(12, "idle"));
            executor.json(self.prompt_result());
            assert_eq!(
                act_with(&self.main, ID, Action::RequestMerge, &mut executor)
                    .unwrap()
                    .status,
                WorktreeStatus::MergeRequested
            );
            assert_eq!(executor.calls[1][..3], ["agent", "prompt", "wTask:p1"]);
            assert!(executor.calls[1][3].contains("INTO THIS SOURCE BRANCH"));
        }

        fn integrated(&self) {
            Self::commit_at(&self.source, "task", "implemented\n");
            git(&self.main, &["merge", "--ff-only", BRANCH]).unwrap();
        }
    }

    type Hook = Box<dyn FnMut(&[&str])>;

    #[derive(Default)]
    struct Fake {
        outputs: VecDeque<io::Result<Vec<u8>>>,
        calls: Vec<Vec<String>>,
        bead: Option<Value>,
        bead_reads: VecDeque<Value>,
        open_children: Vec<String>,
        closed: std::collections::HashSet<String>,
        close_failures: usize,
        hook: Option<Hook>,
    }

    impl Fake {
        fn legacy_closed() -> Self {
            Self {
                closed: ["Bead-1".into(), "Bead-2".into(), "Bead-1.1".into()].into(),
                ..Self::default()
            }
        }

        fn json(&mut self, value: Value) {
            self.outputs.push_back(Ok(value.to_string().into_bytes()));
        }
    }

    impl Executor for Fake {
        fn open_descendants(&mut self, _id: &str) -> io::Result<Vec<String>> {
            Ok(self.open_children.clone())
        }
        fn dependency_ready(&mut self, _id: &str) -> io::Result<bool> {
            Ok(true)
        }

        fn run(&mut self, args: &[&str]) -> io::Result<Vec<u8>> {
            self.calls
                .push(args.iter().map(|s| (*s).to_owned()).collect());
            if let Some(hook) = self.hook.as_mut() {
                hook(args);
            }
            self.outputs
                .pop_front()
                .unwrap_or_else(|| panic!("Unexpected Herdr call: {args:?}"))
        }
        fn show_bead(&mut self, id: &str) -> io::Result<Value> {
            self.calls.push(vec!["show_bead".into(), id.into()]);
            if let Some(bead) = self.bead_reads.pop_front() {
                return Ok(bead);
            }
            if self.closed.contains(id) {
                return Ok(json!({"id":id, "status":"closed"}));
            }
            Ok(self
                .bead
                .clone()
                .unwrap_or_else(|| json!({"id":id, "status":"in_progress", "labels":[crate::beads::READY_TO_MERGE_LABEL]})))
        }
        fn close_bead(&mut self, id: &str, expected: &Value) -> io::Result<()> {
            self.calls.push(vec!["close_bead".into(), id.into()]);
            if self.close_failures > 0 {
                self.close_failures -= 1;
                return Err(error("Beads dependency guard refused closure"));
            }
            let live = self.show_bead(id)?;
            if live["id"] != id
                || live["status"] != expected["status"]
                || crate::beads::ready_to_merge_record(&live)
                    != crate::beads::ready_to_merge_record(expected)
            {
                return Err(error("Live Bead moved backward; closure cancelled"));
            }
            self.closed.insert(id.into());
            Ok(())
        }
        fn git_identity(&mut self, path: &Path) -> io::Result<RepositoryIdentity> {
            SystemExecutor.git_identity(path)
        }
    }

    #[test]
    fn continue_keeps_existing_checkout_and_fences_uncertain_delivery() {
        let fixture = Fixture::managed();
        fs::write(fixture.source.join("unfinished"), "keep me").unwrap();
        let mut executor = Fake {
            bead: Some(json!({"id": "Bead-1", "status": "in_progress"})),
            ..Fake::default()
        };
        executor.json(fixture.inventory(true, true));
        executor.json(fixture.agent(12, "idle"));
        executor.json(fixture.inventory(true, true));
        executor.json(fixture.prompt_result());
        assert!(
            continue_task_with(&fixture.main, ID, &mut executor)
                .unwrap()
                .contains("Continue prompt sent")
        );
        assert_eq!(
            fs::read_to_string(fixture.source.join("unfinished")).unwrap(),
            "keep me"
        );
        assert_eq!(
            executor.calls.last().unwrap()[..3],
            ["agent", "prompt", "wTask:p1"]
        );
        let registry = Registry::open(&fixture.main).unwrap();
        assert!(registry.entries[0]["continue_prompt_pending"].is_null());
        assert_eq!(registry.entries[0]["continue_prompt_before_seq"], 12);
        drop(registry);

        let mut duplicate = Fake {
            bead: Some(json!({"id": "Bead-1", "status": "open"})),
            ..Fake::default()
        };
        duplicate.json(fixture.inventory(true, true));
        duplicate.json(fixture.agent(12, "idle"));
        assert!(continue_task_with(&fixture.main, ID, &mut duplicate).is_err());
        assert!(
            !duplicate
                .calls
                .iter()
                .any(|call| call.get(1).is_some_and(|s| s == "prompt"))
        );

        let mut uncertain = Fake {
            bead: Some(json!({"id": "Bead-1", "status": "open"})),
            ..Fake::default()
        };
        uncertain.json(fixture.inventory(true, true));
        uncertain.json(fixture.agent(13, "idle"));
        uncertain.json(fixture.inventory(true, true));
        uncertain
            .outputs
            .push_back(Err(error("Herdr connection lost")));
        assert!(continue_task_with(&fixture.main, ID, &mut uncertain).is_err());
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        assert_eq!(
            Registry::open(&fixture.main).unwrap().entries[0]["continue_prompt_pending"],
            true
        );
        let mut retry = Fake::default();
        assert!(continue_task_with(&fixture.main, ID, &mut retry).is_err());
        assert!(retry.calls.is_empty());
    }

    #[test]
    fn continue_rejects_replaced_or_busy_agent_before_prompt() {
        let fixture = Fixture::managed();
        for state in ["working", "blocked", "error", "idle"] {
            let mut executor = Fake {
                bead: Some(json!({"id": "Bead-1", "status": "open"})),
                ..Fake::default()
            };
            executor.json(fixture.inventory(true, true));
            let mut agent = fixture.agent(12, state);
            if state == "idle" {
                agent["result"]["agent"]["agent_session"]["value"] = json!("replacement");
            }
            executor.json(agent);
            assert!(continue_task_with(&fixture.main, ID, &mut executor).is_err());
            assert!(
                !executor
                    .calls
                    .iter()
                    .any(|call| call.get(1).is_some_and(|s| s == "prompt"))
            );
            assert!(
                Registry::open(&fixture.main).unwrap().entries[0]["continue_prompt_pending"]
                    .is_null()
            );
        }
    }

    #[test]
    fn intent_is_durable_and_only_write_transactions_hold_the_exclusive_lock() {
        let fixture = Fixture::new();
        let journal =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        let persisted: Value = serde_json::from_slice(
            &fs::read(
                journal
                    .repo
                    .common_dir
                    .join("ontrack-worktrees/registry.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(persisted["version"], 1);
        assert_eq!(persisted["entries"][0]["phase"], "create_possibly_sent");
        assert_eq!(
            persisted["entries"][0]["bead_title"],
            "Fix the selected handoff"
        );
        assert_eq!(
            persisted["entries"][0]["bead_description"],
            "First line\nSecond line"
        );
        assert!(!fixture.source.exists());
        let registry = Registry::open(&fixture.main).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "worktrees::tests::registry_lock_child"])
            .env("ONTRACK_TEST_LOCK_REPOSITORY", &fixture.main)
            .env("ONTRACK_TEST_LOCK_EXPECT_BUSY", "1")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        drop(registry);
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "worktrees::tests::registry_lock_child"])
            .env("ONTRACK_TEST_LOCK_REPOSITORY", &fixture.main)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["cross_process_write"], true);
        drop(registry);
        drop(journal);
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        assert!(!fixture.status().handoff_accepted);
        assert_eq!(
            fixture.status().bead_title.as_deref(),
            Some("Fix the selected handoff")
        );
        assert_eq!(
            fixture.status().bead_description.as_deref(),
            Some("First line\nSecond line")
        );
        assert!(
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .is_err()
        );
    }

    #[test]
    fn older_registry_entries_without_bead_context_remain_viewable() {
        let fixture = Fixture::managed();
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]
            .as_object_mut()
            .unwrap()
            .remove("bead_title");
        registry.entries[0]
            .as_object_mut()
            .unwrap()
            .remove("bead_description");
        registry.save().unwrap();
        drop(registry);
        let item = fixture.status();
        assert_eq!(item.bead_id, "Bead-1");
        assert!(item.bead_title.is_none());
        assert!(item.bead_description.is_none());
    }

    #[test]
    fn accepted_handoff_is_derived_from_durable_active_phase() {
        let pending = json!({"id":"wt", "bead_id":"bead", "phase":"initial_prompt_possibly_sent"});
        let active = json!({"id":"wt", "bead_id":"bead", "phase":"active"});
        assert!(!view(&pending, WorktreeStatus::NeedsAttention, None).handoff_accepted);
        assert!(view(&active, WorktreeStatus::Unmerged, None).handoff_accepted);
    }

    #[test]
    fn registry_lock_child() {
        if let Some(repository) = std::env::var_os("ONTRACK_TEST_LOCK_REPOSITORY") {
            let registry = Registry::open(Path::new(&repository));
            if std::env::var_os("ONTRACK_TEST_LOCK_EXPECT_BUSY").is_some() {
                assert_eq!(registry.err().unwrap().kind(), io::ErrorKind::WouldBlock);
            } else {
                let mut registry = registry.unwrap();
                registry.entries[0]["cross_process_write"] = json!(true);
                registry.save().unwrap();
            }
        }
    }

    #[test]
    fn partial_launch_response_and_failure_survive_restart() {
        let fixture = Fixture::new();
        let mut journal =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        journal
            .capture(
                "create",
                b"malformed but recoverable response with wTask:p1",
            )
            .unwrap();
        journal
            .failed("creation timed out; do not blindly retry")
            .unwrap();
        drop(journal);
        let registry = Registry::open(&fixture.main).unwrap();
        assert!(
            registry.entries[0]["responses"]["create"]
                .as_str()
                .unwrap()
                .contains("wTask:p1")
        );
        assert!(
            inspect(&registry.repo, &registry.entries[0])
                .detail
                .unwrap()
                .contains("timed out")
        );
    }

    #[test]
    fn corrupt_or_unknown_registry_is_not_overwritten() {
        let fixture = Fixture::managed();
        let directory = fixture.main.join(".git/ontrack-worktrees");
        for bytes in [b"not JSON".as_slice(), b"{\"version\":999}"] {
            fs::write(directory.join("registry.json"), bytes).unwrap();
            assert!(Registry::open(&fixture.main).is_err());
            assert_eq!(fs::read(directory.join("registry.json")).unwrap(), bytes);
        }
    }

    #[test]
    fn default_is_actual_primary_branch_not_init_guess_and_detached_refuses() {
        let fixture = Fixture::new();
        let repo = SystemExecutor.git_identity(&fixture.main).unwrap();
        assert_eq!(default_branch(&repo).unwrap(), "trunk");
        git(&fixture.main, &["checkout", "--detach"]).unwrap();
        assert!(
            default_branch(&repo)
                .unwrap_err()
                .to_string()
                .contains("detached")
        );
    }

    #[test]
    fn remote_heads_must_agree_and_match_primary_checkout() {
        let fixture = Fixture::new();
        let repo = SystemExecutor.git_identity(&fixture.main).unwrap();
        git(
            &fixture.main,
            &["update-ref", "refs/remotes/origin/trunk", "HEAD"],
        )
        .unwrap();
        git(
            &fixture.main,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ],
        )
        .unwrap();
        assert_eq!(default_branch(&repo).unwrap(), "trunk");
        git(
            &fixture.main,
            &["update-ref", "refs/remotes/other/different", "HEAD"],
        )
        .unwrap();
        git(
            &fixture.main,
            &[
                "symbolic-ref",
                "refs/remotes/other/HEAD",
                "refs/remotes/other/different",
            ],
        )
        .unwrap();
        assert!(
            default_branch(&repo)
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
        git(
            &fixture.main,
            &["symbolic-ref", "--delete", "refs/remotes/other/HEAD"],
        )
        .unwrap();
        git(&fixture.main, &["checkout", "-b", "not-default"]).unwrap();
        assert!(
            default_branch(&repo)
                .unwrap_err()
                .to_string()
                .contains("must be on default branch trunk")
        );
    }

    #[test]
    fn merge_requires_source_resolution_then_verified_ff_only() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        Fixture::commit_at(&fixture.main, "target", "target advanced\n");
        let before = tip(&fixture.main, "HEAD").unwrap();
        fixture.request();
        let result = reconcile_with(
            &fixture.main,
            CleanupPolicy::default(),
            &mut Fake::default(),
        )
        .unwrap();
        assert_eq!(result[0].status, WorktreeStatus::MergeRequested);
        assert!(
            result[0]
                .detail
                .as_ref()
                .unwrap()
                .contains("no longer contains both commits recorded")
        );
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), before);
        git(&fixture.source, &["merge", "--no-edit", "trunk"]).unwrap();
        let source_tip = tip(&fixture.source, "HEAD").unwrap();
        let mut executor = Fake::default();
        executor.json(fixture.agent(14, "done"));
        executor.json(fixture.agent(14, "done"));
        let result =
            reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(result[0].status, WorktreeStatus::Integrated);
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), source_tip);
        assert_eq!(branch(&fixture.main).unwrap(), "trunk");
        assert_eq!(branch(&fixture.source).unwrap(), BRANCH);
        assert!(executor.closed.contains("Bead-1"));
    }

    #[test]
    fn automatic_merge_accepts_git_proof_without_post_dispatch_sequence_increment() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        Fixture::commit_at(&fixture.main, "target", "target advanced\n");

        let mut dispatch = Fake::default();
        dispatch.json(fixture.agent(12, "idle"));
        dispatch.json(fixture.prompt_result());
        {
            let mut registry = Registry::open(&fixture.main).unwrap();
            request_merge(&mut registry, 0, &mut dispatch, true).unwrap();
            assert_eq!(registry.entries[0]["merge_request"]["lifecycle_seq"], 12);
        }

        git(&fixture.source, &["merge", "--no-ff", "--no-edit", "trunk"]).unwrap();
        let resolved_source = tip(&fixture.source, "HEAD").unwrap();
        let mut executor = Fake::default();
        executor.json(fixture.agent(12, "done"));
        executor.json(fixture.agent(12, "done"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();

        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), resolved_source);
        assert!(executor.closed.contains("Bead-1"));
        assert_eq!(branch(&fixture.main).unwrap(), "trunk");
    }

    #[test]
    fn automatic_dispatch_requires_git_resolution_and_rejects_unsettled_or_mismatched_agents() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        let mut request = Fake::default();
        request.json(fixture.agent(12, "idle"));
        request.json(fixture.prompt_result());
        {
            let mut registry = Registry::open(&fixture.main).unwrap();
            request_merge(&mut registry, 0, &mut request, true).unwrap();
        }
        let mut executor = Fake::default();
        for _ in 0..2 {
            executor.json(fixture.agent(12, "idle"));
        }
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert_eq!(
            tip(&fixture.main, "HEAD").unwrap(),
            tip(&fixture.source, "HEAD").unwrap()
        );
        git(&fixture.main, &["merge", "--ff-only", BRANCH]).unwrap();
        let request_evidence =
            Registry::open(&fixture.main).unwrap().entries[0]["merge_request"].clone();
        for mode in ["working", "session", "request_session", "missing_sequence"] {
            {
                let mut registry = Registry::open(&fixture.main).unwrap();
                registry.entries[0]["merge_request"] = request_evidence.clone();
                if mode == "request_session" {
                    registry.entries[0]["merge_request"]["agent_session"]["value"] =
                        json!("different-request-session");
                } else if mode == "missing_sequence" {
                    registry.entries[0]["merge_request"]["lifecycle_seq"] = Value::Null;
                }
                registry.save().unwrap();
            }
            let mut observed =
                fixture.agent(14, if mode == "working" { "working" } else { "idle" });
            if mode == "session" {
                observed["result"]["agent"]["agent_session"]["value"] = json!("replacement");
            }
            let mut executor = Fake::default();
            for _ in 0..4 {
                executor.json(observed.clone());
            }
            let rows = reconcile_with(
                &fixture.main,
                CleanupPolicy {
                    auto_merge: true,
                    auto_close_workspace: true,
                    auto_remove_checkout: true,
                    ..CleanupPolicy::default()
                },
                &mut executor,
            )
            .unwrap();
            assert_eq!(rows[0].status, WorktreeStatus::Integrated);
            assert!(executor.closed.is_empty(), "{mode}");
            assert!(
                executor
                    .calls
                    .iter()
                    .all(|args| args[0] == "show_bead"
                        || args.get(1).is_some_and(|arg| arg == "get")),
                "{mode}"
            );
            assert!(fixture.source.exists());
            let mut explicit = Fake::default();
            for _ in 0..3 {
                explicit.json(observed.clone());
            }
            assert!(
                close_integrated_bead_with(&fixture.main, "Bead-1", &mut explicit).is_err(),
                "explicit {mode}"
            );
            let mut cleanup = Fake::default();
            for _ in 0..3 {
                cleanup.json(observed.clone());
            }
            assert!(
                act_with(&fixture.main, ID, Action::CloseWorkspace, &mut cleanup).is_err(),
                "cleanup {mode}"
            );
            let mut legacy_cleanup = Fake::legacy_closed();
            for _ in 0..3 {
                legacy_cleanup.json(observed.clone());
            }
            assert!(
                act_with(
                    &fixture.main,
                    ID,
                    Action::CloseWorkspace,
                    &mut legacy_cleanup
                )
                .is_err(),
                "already closed {mode}"
            );
        }
        {
            let mut registry = Registry::open(&fixture.main).unwrap();
            registry.entries[0]["merge_request"] = request_evidence;
            registry.save().unwrap();
        }
        let main = fixture.main.clone();
        let source = fixture.source.clone();
        let mut advanced = Fake {
            hook: Some(Box::new(move |args| {
                if args.starts_with(&["worktree", "remove"]) {
                    git(&main, &["worktree", "remove", source.to_str().unwrap()]).unwrap();
                }
            })),
            ..Fake::default()
        };
        advanced.json(fixture.agent(14, "done"));
        advanced.json(fixture.agent(14, "done"));
        advanced.json(fixture.inventory(true, true));
        advanced.json(json!({"result":{"type":"worktree_removed"}}));
        advanced.json(fixture.inventory(false, false));
        let rows = reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_merge: true,
                auto_close_workspace: true,
                auto_remove_checkout: true,
                ..CleanupPolicy::default()
            },
            &mut advanced,
        )
        .unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::CheckoutRemoved);
        assert!(advanced.closed.contains("Bead-1"));
    }

    #[test]
    fn timed_out_prompt_is_persisted_and_never_resent() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        let mut executor = Fake::default();
        executor.json(fixture.agent(12, "idle"));
        executor
            .outputs
            .push_back(Err(error("timeout after possible delivery")));
        assert!(act_with(&fixture.main, ID, Action::RequestMerge, &mut executor).is_err());
        let mut retry = Fake::default();
        let e = act_with(&fixture.main, ID, Action::RequestMerge, &mut retry).unwrap_err();
        assert!(e.to_string().contains("already possibly sent"));
        assert!(retry.calls.is_empty());
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["merge_request"]["possibly_sent"], true);
    }

    #[test]
    fn target_advancement_requires_explicit_new_resolution_request() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        fixture.request();
        Fixture::commit_at(&fixture.main, "target", "advanced after prompt\n");
        let before = tip(&fixture.main, "HEAD").unwrap();
        let mut executor = Fake::default();
        executor.json(fixture.agent(13, "idle"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::NeedsResolution);
        assert_eq!(executor.calls, vec![vec!["agent", "get", "wTask:p1"]]);
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), before);
        fixture.request();
    }

    #[test]
    fn working_agent_keeps_unresolved_merge_pending_in_worktree_list() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "file", "source change\n");
        Fixture::commit_at(&fixture.main, "file", "target change\n");
        fixture.request();
        assert!(git(&fixture.source, &["merge", "--no-edit", "trunk"]).is_err());
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsResolution);

        let mut working = Fake::default();
        working.json(fixture.agent(13, "working"));
        let rows = list_with(&fixture.main, &mut working).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::MergeRequested);
        assert!(rows[0].detail.as_deref().unwrap().contains("still working"));
        assert_eq!(working.calls, vec![vec!["agent", "get", "wTask:p1"]]);

        for state in ["idle", "done", "blocked"] {
            let mut executor = Fake::default();
            executor.json(fixture.agent(14, state));
            assert_eq!(
                list_with(&fixture.main, &mut executor).unwrap()[0].status,
                WorktreeStatus::NeedsResolution,
                "{state}"
            );
        }
        let mut mismatched = fixture.agent(14, "working");
        mismatched["result"]["agent"]["agent_session"]["value"] = json!("replacement");
        let mut executor = Fake::default();
        executor.json(mismatched);
        assert_eq!(
            list_with(&fixture.main, &mut executor).unwrap()[0].status,
            WorktreeStatus::NeedsResolution
        );
        let mut unavailable = Fake::default();
        unavailable
            .outputs
            .push_back(Err(error("Herdr unavailable")));
        assert_eq!(
            list_with(&fixture.main, &mut unavailable).unwrap()[0].status,
            WorktreeStatus::NeedsResolution
        );
    }

    #[test]
    fn source_merge_against_target_head_has_concrete_display_status_without_authorizing_merge() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "file", "source change\n");
        Fixture::commit_at(&fixture.main, "file", "target change\n");
        assert!(git(&fixture.source, &["merge", "--no-edit", "trunk"]).is_err());
        let item = fixture.status();
        assert_eq!(item.status, WorktreeStatus::MergeInProgress);
        assert!(item.detail.unwrap().contains("unresolved conflicts"));
        assert_eq!(
            list_with(&fixture.main, &mut Fake::default()).unwrap()[0].status,
            WorktreeStatus::MergeInProgress
        );
        assert!(
            act_with(
                &fixture.main,
                ID,
                Action::RequestMerge,
                &mut Fake::default()
            )
            .is_err()
        );

        fs::write(fixture.main.join("uncommitted"), "target dirty\n").unwrap();
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        fs::remove_file(fixture.main.join("uncommitted")).unwrap();
        Fixture::commit_at(&fixture.main, "later", "new target head\n");
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        git(&fixture.source, &["merge", "--abort"]).unwrap();
        let cherry_pick = git(
            &fixture.source,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "CHERRY_PICK_HEAD",
            ],
        )
        .unwrap();
        fs::write(&cherry_pick, tip(&fixture.main, "HEAD").unwrap()).unwrap();
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
    }

    #[test]
    fn activity_report_is_scoped_bounded_advisory_and_clearable() {
        let fixture = Fixture::managed();
        assert!(report_activity(&fixture.main, "working", None).is_err());
        assert!(report_activity(&fixture.source, "invalid", None).is_err());
        assert!(report_activity(&fixture.source, "blocked", None).is_err());
        assert!(report_activity(&fixture.source, "working", Some("bad\nline")).is_err());
        assert!(report_activity(&fixture.source, "working", Some(&"x".repeat(201))).is_err());
        assert!(report_activity(&fixture.source, "clear", Some("unexpected")).is_err());

        report_activity(
            &fixture.source,
            "merging",
            Some("resolving target conflict"),
        )
        .unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["agent_report"]["state"], "merging");
        let item = with_agent_report(
            inspect(&registry.repo, &registry.entries[0]),
            &registry.entries[0],
        );
        assert!(item.detail.unwrap().contains("resolving target conflict"));
        drop(registry);
        assert_eq!(fixture.status().status, WorktreeStatus::InProgress);
        assert!(
            list_with(&fixture.main, &mut Fake::default()).unwrap()[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("Agent report (advisory): merging")
        );

        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]["agent_report"]["reported_at"] = json!(0);
        registry.save().unwrap();
        assert!(
            with_agent_report(
                inspect(&registry.repo, &registry.entries[0]),
                &registry.entries[0],
            )
            .detail
            .as_deref()
            .is_none_or(|detail| !detail.contains("Agent report"))
        );
        drop(registry);

        report_activity(&fixture.source, "clear", None).unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert!(registry.entries[0]["agent_report"].is_null());
    }

    #[test]
    fn working_agent_keeps_advanced_target_pending() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        fixture.request();
        Fixture::commit_at(&fixture.main, "target", "advanced after prompt\n");
        let mut executor = Fake::default();
        executor.json(fixture.agent(13, "working"));
        let rows = list_with(&fixture.main, &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::MergeRequested);
        assert!(
            rows[0]
                .detail
                .as_deref()
                .unwrap()
                .contains("Target advanced")
        );
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsResolution);

        let before = tip(&fixture.main, "HEAD").unwrap();
        let mut reconcile = Fake::default();
        reconcile.json(fixture.agent(13, "working"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut reconcile).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::MergeRequested);
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), before);
        assert_eq!(reconcile.calls, vec![vec!["agent", "get", "wTask:p1"]]);
    }

    #[test]
    fn conflicts_stay_in_source_and_are_visible_until_resolved() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "file", "source change\n");
        Fixture::commit_at(&fixture.main, "file", "target change\n");
        fixture.request();
        let before = tip(&fixture.main, "HEAD").unwrap();
        assert!(git(&fixture.source, &["merge", "--no-edit", "trunk"]).is_err());
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsResolution);
        assert!(
            act_with(
                &fixture.main,
                ID,
                Action::RemoveCheckout,
                &mut Fake::default()
            )
            .is_err()
        );
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), before);
        Fixture::commit_at(&fixture.source, "file", "resolved both\n");
        let mut executor = Fake::default();
        executor.json(fixture.agent(14, "done"));
        executor.json(fixture.agent(14, "done"));
        assert_eq!(
            reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap()[0]
                .status,
            WorktreeStatus::Integrated
        );
    }

    #[test]
    fn dirty_target_dirty_source_wrong_branch_and_missing_source_block_mutations() {
        for mode in ["target", "source", "branch", "missing"] {
            let fixture = Fixture::managed();
            fixture.integrated();
            match mode {
                "target" => fs::write(fixture.main.join("dirty"), "untracked").unwrap(),
                "source" => fs::write(fixture.source.join("dirty"), "untracked").unwrap(),
                "branch" => {
                    git(&fixture.source, &["checkout", "-b", "other"]).unwrap();
                }
                _ => {
                    fs::rename(&fixture.source, fixture.directory.join("moved")).unwrap();
                }
            }
            assert_eq!(
                fixture.status().status,
                if matches!(mode, "target" | "source") {
                    WorktreeStatus::DirtyCheckout
                } else {
                    WorktreeStatus::NeedsAttention
                },
                "{mode}"
            );
            let mut executor = Fake::default();
            assert!(
                act_with(&fixture.main, ID, Action::RemoveCheckout, &mut executor).is_err(),
                "{mode}"
            );
            assert!(executor.calls.is_empty());
        }
    }

    #[test]
    fn uncommitted_changes_in_active_checkouts_do_not_claim_merge_readiness() {
        for (location, file) in [
            ("source", "in-progress"),
            ("target", "in-progress"),
            ("source", "file"),
            ("target", "file"),
        ] {
            let fixture = Fixture::managed();
            let path = if location == "source" {
                &fixture.source
            } else {
                &fixture.main
            };
            fs::write(path.join(file), "uncommitted").unwrap();
            let status = fixture.status();
            assert_eq!(status.status, WorktreeStatus::DirtyCheckout, "{location}");
            assert!(
                status
                    .detail
                    .unwrap()
                    .contains("Uncommitted checkout changes")
            );
            let rows = list_with(&fixture.main, &mut Fake::default()).unwrap();
            assert_eq!(rows[0].status, WorktreeStatus::DirtyCheckout, "{location}");
            let mut executor = Fake::default();
            assert!(
                act_with(&fixture.main, ID, Action::RequestMerge, &mut executor).is_err(),
                "{location}"
            );
            assert!(executor.calls.is_empty());
        }
    }

    #[test]
    fn merge_request_with_ordinary_changes_stays_pending() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        fixture.request();
        fs::write(fixture.source.join("in-progress"), "uncommitted").unwrap();
        let status = fixture.status();
        assert_eq!(status.status, WorktreeStatus::MergeRequested);
        assert!(status.detail.unwrap().contains(DIRTY_CHECKOUT_DETAIL));
    }

    #[test]
    fn external_integration_is_recognized_and_source_advancement_reopens() {
        let fixture = Fixture::managed();
        fixture.integrated();
        assert_eq!(fixture.status().status, WorktreeStatus::Integrated);
        Fixture::commit_at(&fixture.source, "new-task", "more work\n");
        assert_eq!(fixture.status().status, WorktreeStatus::Unmerged);
        assert!(
            act_with(
                &fixture.main,
                ID,
                Action::CloseWorkspace,
                &mut Fake::default()
            )
            .is_err()
        );
    }

    #[test]
    fn finalization_rechecks_tips_after_native_agent_observation() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        fixture.request();
        let main = fixture.main.clone();
        let mut advanced = false;
        let mut executor = Fake {
            hook: Some(Box::new(move |_| {
                if !advanced {
                    Fixture::commit_at(&main, "race", "advanced concurrently\n");
                    advanced = true;
                }
            })),
            ..Fake::default()
        };
        executor.json(fixture.agent(14, "done"));
        executor.json(fixture.agent(14, "done"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::NeedsResolution);
        assert!(
            !ancestor(
                &fixture.main,
                &tip(&fixture.source, "HEAD").unwrap(),
                &tip(&fixture.main, "HEAD").unwrap()
            )
            .unwrap()
        );
    }

    #[test]
    fn cleanup_close_uses_exact_command_and_confirms_workspace_topology() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut executor = Fake::legacy_closed();
        executor.json(fixture.inventory(true, true));
        executor.json(json!({"result":{"type":"workspace_closed"}}));
        executor.json(fixture.inventory(false, true));
        let outcome = act_with(&fixture.main, ID, Action::CloseWorkspace, &mut executor).unwrap();
        assert_eq!(outcome.status, WorktreeStatus::WorkspaceClosed);
        assert_eq!(executor.calls[2], ["workspace", "close", "wTask"]);
        assert!(fixture.source.exists());
        assert!(tip(&fixture.main, &format!("refs/heads/{BRANCH}")).is_ok());
    }

    #[test]
    fn force_remove_preserves_unmerged_branch_and_never_closes_bead() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "unmerged work\n");
        let source_tip = tip(&fixture.source, "HEAD").unwrap();
        fs::write(fixture.source.join("uncommitted"), "discard me").unwrap();
        let main = fixture.main.clone();
        let source = fixture.source.clone();
        let journal_root = main.clone();
        let mut executor = Fake {
            hook: Some(Box::new(move |args| {
                if args.starts_with(&["worktree", "remove"]) {
                    let registry_path = journal_root.join(".git/ontrack-worktrees/registry.json");
                    let journal: Value =
                        serde_json::from_slice(&fs::read(registry_path).unwrap()).unwrap();
                    assert!(journal.to_string().contains("force-remove"));
                    git(
                        &main,
                        &["worktree", "remove", "--force", source.to_str().unwrap()],
                    )
                    .unwrap();
                }
            })),
            ..Default::default()
        };
        executor.json(fixture.inventory(true, true));
        executor.json(json!({"result":{"type":"worktree_removed"}}));
        executor.json(fixture.inventory(false, false));
        let outcome = act_with(
            &fixture.main,
            ID,
            Action::ForceRemoveCheckout,
            &mut executor,
        )
        .unwrap();
        assert_eq!(outcome.status, WorktreeStatus::CheckoutRemoved);
        assert!(!fixture.source.exists());
        assert_eq!(
            tip(&fixture.main, &format!("refs/heads/{BRANCH}")).unwrap(),
            source_tip
        );
        assert_eq!(
            executor.calls[1],
            ["worktree", "remove", "--workspace", "wTask", "--force"]
        );
        assert_eq!(executor.calls.len(), 3);
        assert!(executor.closed.is_empty());
        assert_eq!(fixture.status().status, WorktreeStatus::CheckoutRemoved);
    }

    #[test]
    fn force_remove_rejects_wrong_ownership_and_tracked_children() {
        let fixture = Fixture::managed();
        let mut wrong = fixture.inventory(true, true);
        wrong["result"]["worktrees"][1]["branch"] = json!("other");
        let mut executor = Fake::default();
        executor.json(wrong);
        assert!(
            act_with(
                &fixture.main,
                ID,
                Action::ForceRemoveCheckout,
                &mut executor
            )
            .is_err()
        );
        assert_eq!(executor.calls.len(), 1);
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry
            .entries
            .push(json!({"id":"child", "parent_bead_id":"Bead-1"}));
        registry.save().unwrap();
        drop(registry);
        let mut executor = Fake::default();
        assert!(
            act_with(
                &fixture.main,
                ID,
                Action::ForceRemoveCheckout,
                &mut executor
            )
            .is_err()
        );
        assert!(executor.calls.is_empty());
        assert!(fixture.source.exists());
    }

    #[test]
    fn force_remove_requires_absence_and_recovers_without_resending() {
        let fixture = Fixture::managed();
        let mut executor = Fake::default();
        executor.json(fixture.inventory(true, true));
        executor.json(json!({"result":{"type":"worktree_removed"}}));
        executor.json(fixture.inventory(false, false));
        assert!(
            act_with(
                &fixture.main,
                ID,
                Action::ForceRemoveCheckout,
                &mut executor
            )
            .is_err()
        );
        assert!(fixture.source.exists());
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        git(
            &fixture.main,
            &[
                "worktree",
                "remove",
                "--force",
                fixture.source.to_str().unwrap(),
            ],
        )
        .unwrap();
        let mut recovery = Fake::default();
        recovery.json(fixture.inventory(false, false));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut recovery).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::CheckoutRemoved);
        assert_eq!(recovery.calls.len(), 1);
        assert_eq!(recovery.calls[0][..2], ["worktree", "list"]);
        assert!(recovery.closed.is_empty());
    }

    #[test]
    fn force_remove_timeout_is_fenced_and_not_retried() {
        let fixture = Fixture::managed();
        let mut executor = Fake::default();
        executor.json(fixture.inventory(true, true));
        executor.outputs.push_back(Err(error("removal timed out")));
        assert!(
            act_with(
                &fixture.main,
                ID,
                Action::ForceRemoveCheckout,
                &mut executor
            )
            .is_err()
        );
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        let mut retry = Fake::default();
        assert!(act_with(&fixture.main, ID, Action::ForceRemoveCheckout, &mut retry).is_err());
        assert!(retry.calls.is_empty());
        let mut recovery = Fake::default();
        recovery.json(fixture.inventory(true, true));
        reconcile_with(&fixture.main, CleanupPolicy::default(), &mut recovery).unwrap();
        assert!(
            recovery
                .calls
                .iter()
                .all(|args| args[0] != "close_bead" && !args.iter().any(|arg| arg == "remove"))
        );
        assert!(fixture.source.exists());
    }

    #[test]
    fn restart_checks_ownership_before_any_mutation() {
        let fixture = Fixture::managed();
        let mut wrong = fixture.inventory(true, true);
        wrong["result"]["worktrees"][1]["branch"] = json!("other-branch");
        let mut executor = Fake::default();
        executor.json(wrong);
        assert!(restart_with(&fixture.main, ID, &mut executor).is_err());
        assert_eq!(executor.calls.len(), 1);
        assert_eq!(executor.calls[0][..2], ["worktree", "list"]);
        assert!(fixture.source.exists());
        assert!(Registry::open(&fixture.main).unwrap().entries[0]["restart_pending"].is_null());
    }

    #[test]
    fn restart_force_removal_is_journaled_and_never_blindly_retried() {
        let fixture = Fixture::managed();
        fs::write(
            fixture.source.join("uncommitted"),
            "discard on confirmed restart",
        )
        .unwrap();
        let mut executor = Fake {
            bead: Some(test_bead()),
            ..Default::default()
        };
        executor.json(fixture.inventory(true, true));
        executor.json(fixture.inventory(true, true));
        executor
            .outputs
            .push_back(Ok(b"--kind <KIND> [possible values: opencode]".to_vec()));
        executor
            .outputs
            .push_back(Ok(b"opencode: current (v1) (/tmp/opencode)".to_vec()));
        executor.json(fixture.inventory(true, true));
        executor
            .outputs
            .push_back(Err(error("Herdr timed out after removal may have started")));
        assert!(restart_with(&fixture.main, ID, &mut executor).is_err());
        assert_eq!(
            executor.calls.last().unwrap(),
            &["worktree", "remove", "--workspace", "wTask", "--force"]
        );
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["restart_pending"], true);
        drop(registry);
        let mut retry = Fake::default();
        assert!(restart_with(&fixture.main, ID, &mut retry).is_err());
        assert!(retry.calls.is_empty());
    }

    #[test]
    fn restart_verifies_forced_removal_and_exposes_failed_relaunch() {
        let fixture = Fixture::managed();
        fs::write(fixture.source.join("uncommitted"), "discard me").unwrap();
        let main = fixture.main.clone();
        let source = fixture.source.clone();
        let mut executor = Fake {
            bead: Some(test_bead()),
            hook: Some(Box::new(move |args| {
                if args.starts_with(&["worktree", "remove"]) {
                    git(
                        &main,
                        &["worktree", "remove", "--force", source.to_str().unwrap()],
                    )
                    .unwrap();
                }
            })),
            ..Default::default()
        };
        executor.json(fixture.inventory(true, true));
        executor.json(fixture.inventory(true, true));
        executor
            .outputs
            .push_back(Ok(b"[possible values: opencode]".to_vec()));
        executor
            .outputs
            .push_back(Ok(b"opencode: current (v1) (/tmp/opencode)".to_vec()));
        executor.json(fixture.inventory(true, true));
        executor.json(json!({"result":{"type":"worktree_removed"}}));
        executor.json(fixture.inventory(false, false));
        executor
            .outputs
            .push_back(Err(error("discovery unavailable")));
        assert!(restart_with(&fixture.main, ID, &mut executor).is_err());
        assert!(!fixture.source.exists());
        assert!(tip(&fixture.main, &format!("refs/heads/{BRANCH}")).is_ok());
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["checkout_removed"], true);
        assert_eq!(registry.entries[0]["restarted"], true);
        assert!(registry.entries[0]["restart_launch_error"].is_string());
        assert!(registry.entries[0]["restart_launch_pending"].is_null());
    }

    #[test]
    fn failed_cleanup_is_not_retried_and_can_recover_from_read_only_confirmation() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut executor = Fake::legacy_closed();
        executor.json(fixture.inventory(true, true));
        executor
            .outputs
            .push_back(Err(error("close timed out; may have succeeded")));
        assert!(act_with(&fixture.main, ID, Action::CloseWorkspace, &mut executor).is_err());
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        let mut confirm = Fake::default();
        confirm.json(fixture.inventory(true, true));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut confirm).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::NeedsAttention);
        assert_eq!(confirm.calls[0][..2], ["worktree", "list"]);
        let mut confirm = Fake::default();
        confirm.json(fixture.inventory(false, true));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut confirm).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::WorkspaceClosed);
        assert_eq!(confirm.calls.len(), 1);
    }

    #[test]
    fn auto_remove_precedes_close_and_preserves_branch() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let main = fixture.main.clone();
        let source = fixture.source.clone();
        let mut executor = Fake {
            hook: Some(Box::new(move |args| {
                if args.starts_with(&["worktree", "remove"]) {
                    git(&main, &["worktree", "remove", source.to_str().unwrap()]).unwrap();
                }
            })),
            ..Fake::default()
        };
        executor.json(fixture.agent(12, "done"));
        executor.json(fixture.inventory(true, true));
        executor.json(json!({"result":{"type":"worktree_removed"}}));
        executor.json(fixture.inventory(false, false));
        let policy = CleanupPolicy {
            auto_merge: true,
            auto_merge_children: true,
            auto_close_workspace: true,
            auto_remove_checkout: true,
        };
        let rows = reconcile_with(&fixture.main, policy, &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::CheckoutRemoved);
        assert!(
            executor
                .calls
                .iter()
                .any(|args| args == &["worktree", "remove", "--workspace", "wTask"])
        );
        assert!(executor.closed.contains("Bead-1"));
        assert!(
            !executor
                .calls
                .iter()
                .flatten()
                .any(|s| s == "--force" || s == "--group" || s == "close")
        );
        assert!(tip(&fixture.main, &format!("refs/heads/{BRANCH}")).is_ok());
    }

    #[test]
    fn reconcile_records_externally_removed_checkout_without_cleanup_commands() {
        for git_removal in [true, false] {
            let fixture = Fixture::managed();
            Fixture::commit_at(&fixture.source, "task", "unmerged work\n");
            let source_tip = tip(&fixture.source, "HEAD").unwrap();
            if git_removal {
                git(
                    &fixture.main,
                    &["worktree", "remove", fixture.source.to_str().unwrap()],
                )
                .unwrap();
            } else {
                fs::remove_dir_all(&fixture.source).unwrap();
            }

            let mut executor = Fake::default();
            let rows =
                reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
            assert_eq!(rows[0].status, WorktreeStatus::CheckoutRemoved);
            assert!(executor.calls.is_empty());
            assert_eq!(
                tip(&fixture.main, &format!("refs/heads/{BRANCH}")).unwrap(),
                source_tip
            );
            let registry = Registry::open(&fixture.main).unwrap();
            assert_eq!(registry.entries[0]["checkout_removed"], true);
            assert_eq!(registry.entries[0]["checkout_removed_externally"], true);
            drop(registry);

            assert_eq!(fixture.status().status, WorktreeStatus::CheckoutRemoved);
            assert_eq!(
                reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap()[0]
                    .status,
                WorktreeStatus::CheckoutRemoved
            );
            fs::create_dir(&fixture.source).unwrap();
            assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        }
    }

    #[test]
    fn removed_integrated_checkout_stays_removed_when_target_is_dirty() {
        let fixture = Fixture::managed();
        fixture.integrated();
        git(
            &fixture.main,
            &["worktree", "remove", fixture.source.to_str().unwrap()],
        )
        .unwrap();
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]["checkout_removed"] = json!(true);
        registry.entries[0]["workspace_closed"] = json!(true);
        registry.save().unwrap();
        drop(registry);

        fs::write(fixture.main.join("local-export-change"), "uncommitted\n").unwrap();

        assert_eq!(fixture.status().status, WorktreeStatus::CheckoutRemoved);
    }

    #[test]
    fn reconcile_keeps_present_or_invalid_checkout_paths_visible() {
        let fixture = Fixture::managed();
        let mut executor = Fake::default();
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::InProgress);
        assert!(executor.calls.is_empty());
        let mut registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["checkout_removed"], false);
        registry.entries[0]["path"] = json!("relative/checkout");
        registry.save().unwrap();
        drop(registry);
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::NeedsAttention);
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["checkout_removed"], false);
    }

    #[test]
    fn reconcile_keeps_uncertain_restart_visible_when_checkout_is_missing() {
        let fixture = Fixture::managed();
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]["restart_pending"] = json!(true);
        registry.save().unwrap();
        drop(registry);
        git(
            &fixture.main,
            &["worktree", "remove", fixture.source.to_str().unwrap()],
        )
        .unwrap();

        let mut executor = Fake::default();
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::NeedsAttention);
        assert!(executor.calls.is_empty());
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["checkout_removed"], false);
        assert!(registry.entries[0]["checkout_removed_externally"].is_null());
    }

    #[test]
    fn automatic_actions_require_ready_bead_original_session_and_post_prompt_activity() {
        for mode in [
            "open",
            "unknown",
            "working",
            "blocked",
            "stale",
            "changed_session",
            "missing_session",
        ] {
            let fixture = Fixture::managed();
            fixture.integrated();
            let mut executor = Fake::default();
            let mut agent = fixture.agent(
                if mode == "stale" { 10 } else { 12 },
                if matches!(mode, "unknown" | "working" | "blocked") {
                    mode
                } else {
                    "done"
                },
            );
            if mode == "open" {
                executor.bead = Some(json!({"id":"Bead-1", "status":"open"}));
            }
            if mode == "changed_session" {
                agent["result"]["agent"]["agent_session"]["value"] = json!("replaced");
            }
            if mode == "missing_session" {
                let mut registry = Registry::open(&fixture.main).unwrap();
                registry.entries[0]["agent_session"] = Value::Null;
                registry.save().unwrap();
            }
            executor.json(agent);
            let rows = reconcile_with(
                &fixture.main,
                CleanupPolicy {
                    auto_merge: true,
                    auto_merge_children: true,
                    auto_close_workspace: true,
                    auto_remove_checkout: true,
                },
                &mut executor,
            )
            .unwrap();
            assert_eq!(rows[0].status, WorktreeStatus::Integrated, "{mode}");
            assert!(
                !executor.calls.iter().any(|args| args
                    .get(1)
                    .is_some_and(|s| s == "remove" || s == "close" || s == "prompt")),
                "{mode}"
            );
            if mode != "open" {
                assert!(rows[0].detail.is_some(), "{mode}");
            }
        }
    }

    #[test]
    fn default_policy_closes_integrated_bead_without_closing_or_removing_workspace() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut executor = Fake::default();
        executor.json(fixture.agent(12, "done"));
        assert_eq!(
            reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap()[0]
                .status,
            WorktreeStatus::Integrated
        );
        assert!(executor.closed.contains("Bead-1"));
        assert!(!executor.calls.iter().any(|args| {
            args.get(1)
                .is_some_and(|s| s == "close" || s == "remove" || s == "prompt")
        }));
    }

    #[test]
    fn readiness_without_native_session_still_records_initial_lifecycle_and_terminal() {
        let fixture = Fixture::new();
        let mut journal =
            LaunchJournal::begin(&fixture.main, &test_bead(), BRANCH, "opencode", ID, "wMain")
                .unwrap();
        let mut ready = fixture.late_session_agent(10, "idle");
        ready["result"]["agent"]
            .as_object_mut()
            .unwrap()
            .remove("agent_session");
        journal
            .capture("ready", ready.to_string().as_bytes())
            .unwrap();
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["initial_lifecycle_seq"], 10);
        assert_eq!(registry.entries[0]["initial_terminal_id"], "term-task");
        assert!(registry.entries[0]["agent_session"].is_null());
    }

    #[test]
    fn late_native_session_recovers_legacy_handoff_and_completes_automatic_lifecycle() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        fixture.simulate_legacy_late_session();
        let policy = CleanupPolicy {
            auto_merge: true,
            auto_merge_children: true,
            auto_close_workspace: true,
            auto_remove_checkout: true,
        };
        let mut request = Fake::default();
        request.json(fixture.late_session_agent(12, "done"));
        request.json(fixture.late_session_agent(12, "done"));
        request.json(fixture.prompt_result());
        assert_eq!(
            reconcile_with(&fixture.main, policy, &mut request).unwrap()[0].status,
            WorktreeStatus::MergeRequested
        );
        assert_eq!(
            request
                .calls
                .iter()
                .filter(|args| args.starts_with(&["agent".into(), "prompt".into()]))
                .count(),
            1
        );
        let registry = Registry::open(&fixture.main).unwrap();
        assert_eq!(registry.entries[0]["initial_lifecycle_seq"], 10);
        assert_eq!(registry.entries[0]["initial_terminal_id"], "term-task");
        assert_eq!(registry.entries[0]["agent_session"]["value"], "session-1");
        drop(registry);

        let main = fixture.main.clone();
        let source = fixture.source.clone();
        let mut finish = Fake {
            hook: Some(Box::new(move |args| {
                if args.starts_with(&["worktree", "remove"]) {
                    git(&main, &["worktree", "remove", source.to_str().unwrap()]).unwrap();
                }
            })),
            ..Fake::default()
        };
        finish.json(fixture.late_session_agent(14, "done"));
        finish.json(fixture.late_session_agent(14, "done"));
        finish.json(fixture.late_session_agent(14, "done"));
        finish.json(fixture.inventory(true, true));
        finish.json(json!({"result":{"type":"worktree_removed"}}));
        finish.json(fixture.inventory(false, false));
        assert_eq!(
            reconcile_with(&fixture.main, policy, &mut finish).unwrap()[0].status,
            WorktreeStatus::CheckoutRemoved
        );
        assert!(!fixture.source.exists());
        assert!(
            finish
                .calls
                .iter()
                .any(|args| { args == &["worktree", "remove", "--workspace", "wTask"] })
        );
    }

    #[test]
    fn late_native_session_on_changed_terminal_never_triggers_automatic_cleanup() {
        let fixture = Fixture::managed();
        fixture.integrated();
        fixture.simulate_legacy_late_session();
        let mut agent = fixture.late_session_agent(12, "done");
        agent["result"]["agent"]["terminal_id"] = json!("term-replaced");
        let mut executor = Fake::default();
        executor.json(agent);
        let rows = reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_merge: true,
                auto_merge_children: true,
                auto_close_workspace: true,
                auto_remove_checkout: true,
            },
            &mut executor,
        )
        .unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Integrated);
        assert!(
            rows[0]
                .detail
                .as_ref()
                .unwrap()
                .contains("terminal identity")
        );
        assert_eq!(executor.calls.len(), 3);
        assert!(fixture.source.exists());
        let registry = Registry::open(&fixture.main).unwrap();
        assert!(registry.entries[0]["agent_session"].is_null());
    }

    #[test]
    fn late_native_session_closes_integrated_workspace_when_only_close_is_enabled() {
        let fixture = Fixture::managed();
        fixture.integrated();
        fixture.simulate_legacy_late_session();
        let mut executor = Fake::default();
        executor.json(fixture.late_session_agent(12, "done"));
        executor.json(fixture.inventory(true, true));
        executor.json(json!({"result":{"type":"workspace_closed"}}));
        executor.json(fixture.inventory(false, true));
        let rows = reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_close_workspace: true,
                ..CleanupPolicy::default()
            },
            &mut executor,
        )
        .unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::WorkspaceClosed);
        assert!(fixture.source.exists());
        assert!(
            executor
                .calls
                .iter()
                .any(|args| { args == &["workspace", "close", "wTask"] })
        );
        assert!(
            !executor
                .calls
                .iter()
                .any(|args| args.starts_with(&["worktree".into(), "remove".into()]))
        );
    }

    #[test]
    fn ready_bead_with_native_completed_session_can_request_auto_merge_once() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implemented\n");
        let before = tip(&fixture.main, "HEAD").unwrap();
        let mut executor = Fake::default();
        executor.json(fixture.agent(12, "done"));
        executor.json(fixture.agent(12, "done"));
        executor.json(fixture.prompt_result());
        let policy = CleanupPolicy {
            auto_merge: true,
            ..CleanupPolicy::default()
        };
        let rows = reconcile_with(&fixture.main, policy, &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::MergeRequested);
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), before);
        assert_eq!(
            executor
                .calls
                .iter()
                .filter(|args| args.get(1).is_some_and(|s| s == "prompt"))
                .count(),
            1
        );
        let mut next = Fake::default();
        next.json(fixture.agent(14, "done"));
        next.bead = Some(json!({"id":"Bead-1", "status":"closed"}));
        next.json(fixture.agent(14, "done"));
        assert_eq!(
            reconcile_with(&fixture.main, policy, &mut next).unwrap()[0].status,
            WorktreeStatus::Integrated
        );
        assert!(
            !next
                .calls
                .iter()
                .any(|args| args.get(1).is_some_and(|s| s == "prompt"))
        );
    }

    #[test]
    fn child_auto_merge_and_cleanup_use_parent_target_and_preferences() {
        for (auto_merge, auto_merge_children, cleanup_action) in [
            (false, true, None),
            (false, true, Some(Action::CloseWorkspace)),
            (false, true, Some(Action::RemoveCheckout)),
            (true, false, None),
        ] {
            let should_request = auto_merge_children;
            let fixture = Fixture::managed();
            let child_path = fixture.directory.join("child");
            let child_branch = "ontrack/child";
            let child = json!({"id":"Bead-1.1", "title":"Child", "parent":"Bead-1"});
            let mut journal = LaunchJournal::begin(
                &fixture.main,
                &child,
                child_branch,
                "opencode",
                "child",
                "wMain",
            )
            .unwrap();
            git(
                &fixture.main,
                &[
                    "worktree",
                    "add",
                    "-b",
                    child_branch,
                    child_path.to_str().unwrap(),
                    BRANCH,
                ],
            )
            .unwrap();
            let created = json!({"workspace":{"workspace_id":"wChild"},
                "root_pane":{"pane_id":"wChild:p1"},
                "worktree":{"path":child_path,"branch":child_branch}});
            journal.created(&created).unwrap();
            let child_agent = |seq| {
                json!({"result":{"type":"agent_info", "agent":{
                    "pane_id":"wChild:p1", "workspace_id":"wChild", "agent":"opencode",
                    "agent_status":"done", "cwd":child_path, "foreground_cwd":child_path,
                    "agent_session":{"agent":"opencode", "kind":"id", "source":"herdr:opencode", "value":"child-session"},
                    "state_change_seq":seq
                }}})
            };
            journal
                .capture("ready", child_agent(10).to_string().as_bytes())
                .unwrap();
            journal.phase("active").unwrap();
            drop(journal);
            Fixture::commit_at(&child_path, "child-task", "completed work\n");

            let mut executor = Fake::default();
            executor.json(child_agent(12));
            if should_request {
                executor.json(child_agent(12));
                executor.json(json!({"result":{"type":"agent_prompted", "agent":{
                    "pane_id":"wChild:p1", "workspace_id":"wChild"}}}));
            }
            let rows = reconcile_with(
                &fixture.main,
                CleanupPolicy {
                    auto_merge,
                    auto_merge_children,
                    ..CleanupPolicy::default()
                },
                &mut executor,
            )
            .unwrap();
            assert_eq!(
                rows[1].status,
                if should_request {
                    WorktreeStatus::MergeRequested
                } else {
                    WorktreeStatus::Unmerged
                }
            );
            assert_eq!(
                executor
                    .calls
                    .iter()
                    .filter(|args| args.get(1).is_some_and(|s| s == "prompt"))
                    .count(),
                usize::from(should_request)
            );
            let registry = Registry::open(&fixture.main).unwrap();
            assert_eq!(registry.entries[1]["target_branch"], BRANCH);
            assert_eq!(registry.entries[1]["target_path"], json!(fixture.source));
            drop(registry);
            if should_request {
                let main_tip = tip(&fixture.main, "HEAD").unwrap();
                let child_tip = tip(&child_path, "HEAD").unwrap();
                let mut finish = Fake::default();
                finish.json(child_agent(14));
                finish.json(child_agent(14));
                finish.json(fixture.agent(12, "done"));
                let rows = reconcile_with(
                    &fixture.main,
                    CleanupPolicy {
                        auto_merge,
                        auto_merge_children,
                        ..CleanupPolicy::default()
                    },
                    &mut finish,
                )
                .unwrap();
                assert_eq!(rows[1].status, WorktreeStatus::Integrated);
                assert!(finish.closed.contains("Bead-1.1"));
                assert!(!finish.closed.contains("Bead-1"));
                assert_eq!(tip(&fixture.source, "HEAD").unwrap(), child_tip);
                assert_eq!(tip(&fixture.main, "HEAD").unwrap(), main_tip);

                if let Some(action) = cleanup_action {
                    let remove_checkout = matches!(action, Action::RemoveCheckout);
                    // A reserved Epic hosts children without an active parent agent.
                    let mut registry = Registry::open(&fixture.main).unwrap();
                    registry.entries[0]["phase"] = json!("reserved");
                    registry.save().unwrap();
                    drop(registry);
                    let inventory = |open, checkout| {
                        let mut inventory = fixture.inventory(true, true);
                        if checkout {
                            inventory["result"]["worktrees"].as_array_mut().unwrap().push(
                                json!({"path":child_path, "branch":child_branch,
                                    "open_workspace_id":if open {json!("wChild")} else {Value::Null}}),
                            );
                        }
                        inventory
                    };
                    let main = fixture.main.clone();
                    let child_checkout = child_path.clone();
                    let mut cleanup = Fake {
                        hook: Some(Box::new(move |args| {
                            if args.starts_with(&["worktree", "remove"]) {
                                git(
                                    &main,
                                    &["worktree", "remove", child_checkout.to_str().unwrap()],
                                )
                                .unwrap();
                            }
                        })),
                        ..Fake::default()
                    };
                    cleanup.json(child_agent(14));
                    cleanup.json(child_agent(14));
                    cleanup.json(inventory(true, true));
                    cleanup.json(json!({"result":{"type":if remove_checkout {
                        "worktree_removed"
                    } else {
                        "workspace_closed"
                    }}}));
                    cleanup.json(inventory(false, !remove_checkout));
                    let rows = reconcile_with(
                        &fixture.main,
                        CleanupPolicy {
                            auto_merge: false,
                            auto_merge_children: false,
                            auto_close_workspace: true,
                            auto_remove_checkout: remove_checkout,
                        },
                        &mut cleanup,
                    )
                    .unwrap();
                    assert_eq!(rows[0].status, WorktreeStatus::Reserved);
                    assert_eq!(
                        rows[1].status,
                        if remove_checkout {
                            WorktreeStatus::CheckoutRemoved
                        } else {
                            WorktreeStatus::WorkspaceClosed
                        }
                    );
                    let expected = if remove_checkout {
                        vec!["worktree", "remove", "--workspace", "wChild"]
                    } else {
                        vec!["workspace", "close", "wChild"]
                    };
                    assert!(cleanup.calls.iter().any(|args| args == &expected));
                    assert_eq!(child_path.exists(), !remove_checkout);
                    assert_eq!(tip(&fixture.source, "HEAD").unwrap(), child_tip);
                    assert_eq!(tip(&fixture.main, "HEAD").unwrap(), main_tip);
                    assert!(tip(&fixture.main, &format!("refs/heads/{child_branch}")).is_ok());
                }
            }
        }
    }

    #[test]
    fn child_default_does_not_auto_merge_top_level_handoff() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "completed work\n");
        let mut executor = Fake::default();
        executor.json(fixture.agent(12, "done"));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::Unmerged);
        assert!(
            !executor
                .calls
                .iter()
                .any(|args| args.get(1).is_some_and(|arg| arg == "prompt"))
        );
    }

    #[test]
    fn timed_out_removal_recovers_without_reissuing_and_branch_advancement_is_visible() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let main = fixture.main.clone();
        let source = fixture.source.clone();
        let mut executor = Fake {
            hook: Some(Box::new(move |args| {
                if args.starts_with(&["worktree", "remove"]) {
                    git(&main, &["worktree", "remove", source.to_str().unwrap()]).unwrap();
                }
            })),
            ..Fake::legacy_closed()
        };
        executor.json(fixture.inventory(true, true));
        executor
            .outputs
            .push_back(Err(error("remove timed out after possible success")));
        assert!(act_with(&fixture.main, ID, Action::RemoveCheckout, &mut executor).is_err());
        let mut confirm = Fake::default();
        confirm.json(fixture.inventory(false, false));
        let rows = reconcile_with(&fixture.main, CleanupPolicy::default(), &mut confirm).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::CheckoutRemoved);
        assert_eq!(confirm.calls.len(), 1);
        assert_eq!(confirm.calls[0][..2], ["worktree", "list"]);
        git(
            &fixture.main,
            &[
                "worktree",
                "add",
                fixture.directory.join("new-checkout").to_str().unwrap(),
                BRANCH,
            ],
        )
        .unwrap();
        Fixture::commit_at(
            &fixture.directory.join("new-checkout"),
            "new-work",
            "not integrated\n",
        );
        assert_eq!(fixture.status().status, WorktreeStatus::NeedsAttention);
        assert!(
            fixture
                .status()
                .detail
                .unwrap()
                .contains("advanced after checkout removal")
        );
    }

    #[test]
    fn dangling_remote_default_ref_is_not_treated_as_absent() {
        let fixture = Fixture::new();
        let repo = SystemExecutor.git_identity(&fixture.main).unwrap();
        git(
            &fixture.main,
            &["remote", "add", "origin", "/nonexistent-test-remote"],
        )
        .unwrap();
        git(
            &fixture.main,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/missing",
            ],
        )
        .unwrap();
        assert!(default_branch(&repo).is_err());
    }

    #[test]
    fn cleanup_rejects_reassigned_workspace_ids_and_changed_checkout_relationships() {
        for action in [Action::CloseWorkspace, Action::RemoveCheckout] {
            for mode in [
                "reassigned",
                "wrong_path",
                "wrong_branch",
                "wrong_source",
                "wrong_primary_owner",
            ] {
                let fixture = Fixture::managed();
                fixture.integrated();
                let mut inventory = fixture.inventory(true, true);
                match mode {
                    "reassigned" => {
                        inventory["result"]["worktrees"][1]["open_workspace_id"] = json!("wOther");
                        inventory["result"]["worktrees"].as_array_mut().unwrap().push(json!({"path":fixture.main, "branch":"trunk", "open_workspace_id":"wTask"}));
                    }
                    "wrong_path" => {
                        inventory["result"]["worktrees"][1]["path"] = json!(fixture.main)
                    }
                    "wrong_branch" => {
                        inventory["result"]["worktrees"][1]["branch"] = json!("unrelated-branch")
                    }
                    "wrong_source" => {
                        inventory["result"]["source"]["source_workspace_id"] = json!("wReassigned")
                    }
                    _ => {
                        inventory["result"]["worktrees"][0]["open_workspace_id"] =
                            json!("wReassigned")
                    }
                }
                let mut executor = Fake::legacy_closed();
                executor.json(inventory);
                assert!(
                    act_with(&fixture.main, ID, action, &mut executor).is_err(),
                    "{action:?} {mode}"
                );
                assert_eq!(executor.calls.len(), 2, "{action:?} {mode}");
                assert_eq!(executor.calls[1][..2], ["worktree", "list"]);
                assert!(fixture.source.exists());
                let registry = Registry::open(&fixture.main).unwrap();
                assert!(registry.entries[0]["cleanup_pending"].is_null());
            }
        }
    }

    #[test]
    fn active_worktree_requires_ready_bead_and_finished_recorded_agent_for_ready_label() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implementation\n");
        assert_eq!(fixture.status().status, WorktreeStatus::Unmerged);

        let mut open = Fake {
            bead: Some(json!({"id":"Bead-1","status":"in_progress"})),
            ..Fake::default()
        };
        let rows = list_with(&fixture.main, &mut open).unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::InProgress);
        assert_eq!(open.calls, vec![vec!["show_bead", "Bead-1"]]);

        let mut working = Fake::default();
        working.json(fixture.agent(12, "working"));
        assert_eq!(
            list_with(&fixture.main, &mut working).unwrap()[0].status,
            WorktreeStatus::InProgress
        );

        let mut changed_session = Fake::default();
        let mut agent = fixture.agent(12, "done");
        agent["result"]["agent"]["agent_session"]["value"] = json!("another-session");
        changed_session.json(agent);
        assert_eq!(
            list_with(&fixture.main, &mut changed_session).unwrap()[0].status,
            WorktreeStatus::InProgress
        );

        let mut idle_before_task_activity = Fake::default();
        idle_before_task_activity.json(fixture.agent(10, "idle"));
        assert_eq!(
            list_with(&fixture.main, &mut idle_before_task_activity).unwrap()[0].status,
            WorktreeStatus::InProgress
        );

        let mut finished = Fake::default();
        finished.json(fixture.agent(12, "done"));
        assert_eq!(
            list_with(&fixture.main, &mut finished).unwrap()[0].status,
            WorktreeStatus::Unmerged
        );
    }

    #[test]
    fn display_checks_release_registry_lock_before_external_calls() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implementation\n");
        for reconcile in [false, true] {
            let main = fixture.main.clone();
            let mut executor = Fake::default();
            executor.json(fixture.agent(12, "done"));
            executor.hook = Some(Box::new(move |_| {
                // A new handoff must be able to record intent while this
                // display-only Herdr lookup is in progress.
                assert!(Registry::open(&main).is_ok());
            }));
            let rows = if reconcile {
                reconcile_with(&fixture.main, CleanupPolicy::default(), &mut executor).unwrap()
            } else {
                list_with(&fixture.main, &mut executor).unwrap()
            };
            assert_eq!(rows[0].status, WorktreeStatus::Unmerged);
        }
    }

    #[test]
    fn legacy_handoff_uses_recorded_agent_activity_for_ready_display_only() {
        let fixture = Fixture::managed();
        Fixture::commit_at(&fixture.source, "task", "implementation\n");
        let mut registry = Registry::open(&fixture.main).unwrap();
        let entry = &mut registry.entries[0];
        entry.as_object_mut().unwrap().remove("agent_session");
        entry
            .as_object_mut()
            .unwrap()
            .remove("initial_lifecycle_seq");
        let mut ready: Value =
            serde_json::from_str(entry["responses"]["ready"].as_str().unwrap()).unwrap();
        ready["result"]["agent"]
            .as_object_mut()
            .unwrap()
            .remove("agent_session");
        ready["result"]["agent"]["terminal_id"] = json!("original-terminal");
        entry["responses"]["ready"] = json!(ready.to_string());
        registry.save().unwrap();
        drop(registry);

        let observed = |seq, state, terminal| {
            let mut agent = fixture.agent(seq, state);
            agent["result"]["agent"]["terminal_id"] = json!(terminal);
            let mut executor = Fake::default();
            executor.json(agent);
            list_with(&fixture.main, &mut executor).unwrap()[0].clone()
        };
        let ready = observed(12, "idle", "original-terminal");
        assert_eq!(ready.status, WorktreeStatus::Unmerged);
        assert!(
            ready
                .detail
                .unwrap()
                .contains("automatic reconciliation can bind a later matching session")
        );
        assert_eq!(
            observed(10, "idle", "original-terminal").status,
            WorktreeStatus::InProgress
        );
        assert_eq!(
            observed(12, "working", "original-terminal").status,
            WorktreeStatus::InProgress
        );
        assert_eq!(
            observed(12, "idle", "replacement-terminal").status,
            WorktreeStatus::InProgress
        );

        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]["responses"]["ready"] = Value::Null;
        registry.save().unwrap();
        drop(registry);
        assert_eq!(
            observed(12, "idle", "original-terminal").status,
            WorktreeStatus::InProgress
        );
    }

    #[test]
    fn immediately_ready_bead_does_not_complete_or_clean_untouched_checkout() {
        let fixture = Fixture::managed();
        let before = tip(&fixture.main, "HEAD").unwrap();
        let mut executor = Fake::default();
        executor.json(fixture.agent(12, "done"));
        let rows = reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_merge: true,
                auto_merge_children: true,
                auto_close_workspace: true,
                auto_remove_checkout: true,
            },
            &mut executor,
        )
        .unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::InProgress);
        assert!(rows[0].detail.as_ref().unwrap().contains("No task commits"));
        assert!(executor.calls.is_empty());
        assert!(fixture.source.exists());
        assert_eq!(tip(&fixture.main, "HEAD").unwrap(), before);
        for action in [
            Action::RequestMerge,
            Action::CloseWorkspace,
            Action::RemoveCheckout,
        ] {
            assert!(
                act_with(&fixture.main, ID, action, &mut executor)
                    .unwrap_err()
                    .to_string()
                    .contains("No-op completion is not supported")
            );
        }
        assert!(executor.calls.is_empty());
    }

    #[test]
    fn closed_integrated_workspace_can_remove_checkout_without_querying_departed_agent() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut close = Fake::legacy_closed();
        close.json(fixture.inventory(true, true));
        close.json(json!({"result":{"type":"workspace_closed"}}));
        close.json(fixture.inventory(false, true));
        let result = act_with(&fixture.main, ID, Action::CloseWorkspace, &mut close).unwrap();
        assert_eq!(result.status, WorktreeStatus::WorkspaceClosed);
        assert!(result.detail.unwrap().contains("Integration is verified"));
        let source_tip = tip(&fixture.source, "HEAD").unwrap();
        let mut later = Fake::legacy_closed();
        later.json(fixture.inventory(false, true));
        later.json(fixture.inventory(false, true));
        later.json(fixture.inventory(false, false));
        let removed = act_with(&fixture.main, ID, Action::RemoveCheckout, &mut later).unwrap();
        assert_eq!(removed.status, WorktreeStatus::CheckoutRemoved);
        assert!(!fixture.source.exists());
        assert_eq!(
            tip(&fixture.main, &format!("refs/heads/{BRANCH}")).unwrap(),
            source_tip
        );
        assert!(
            later
                .calls
                .iter()
                .all(|call| call.first().is_none_or(|arg| arg != "agent"))
        );
    }

    #[test]
    fn closed_workspace_removal_refuses_reopened_herdr_checkout() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]["workspace_closed"] = json!(true);
        registry.save().unwrap();
        drop(registry);
        let mut executor = Fake::legacy_closed();
        executor.json(fixture.inventory(true, true));
        let error = act_with(&fixture.main, ID, Action::RemoveCheckout, &mut executor).unwrap_err();
        assert!(error.to_string().contains("checkout as open"));
        assert!(fixture.source.exists());
    }

    #[test]
    fn closed_workspace_requires_explicit_removal_even_with_automatic_cleanup() {
        let fixture = Fixture::managed();
        fixture.integrated();
        let mut registry = Registry::open(&fixture.main).unwrap();
        registry.entries[0]["workspace_closed"] = json!(true);
        registry.save().unwrap();
        drop(registry);
        let mut executor = Fake::legacy_closed();
        let rows = reconcile_with(
            &fixture.main,
            CleanupPolicy {
                auto_remove_checkout: true,
                ..CleanupPolicy::default()
            },
            &mut executor,
        )
        .unwrap();
        assert_eq!(rows[0].status, WorktreeStatus::WorkspaceClosed);
        assert!(fixture.source.exists());
        assert!(executor.calls.is_empty());
    }

    #[test]
    fn closed_workspace_removal_refuses_unintegrated_or_dirty_checkout() {
        for integrated in [false, true] {
            let fixture = Fixture::managed();
            if integrated {
                fixture.integrated();
                fs::write(fixture.source.join("uncommitted"), "keep me").unwrap();
            } else {
                Fixture::commit_at(&fixture.source, "task", "not integrated\n");
            }
            let mut registry = Registry::open(&fixture.main).unwrap();
            registry.entries[0]["workspace_closed"] = json!(true);
            registry.save().unwrap();
            drop(registry);
            let mut executor = Fake::legacy_closed();
            assert!(act_with(&fixture.main, ID, Action::RemoveCheckout, &mut executor).is_err());
            assert!(fixture.source.exists());
            assert!(executor.calls.is_empty());
        }
    }

    #[test]
    fn manual_merge_without_optional_session_or_sequence_metadata_uses_current_git_history() {
        for missing_at_launch in [false, true] {
            let fixture = Fixture::managed();
            if missing_at_launch {
                let mut registry = Registry::open(&fixture.main).unwrap();
                registry.entries[0]["agent_session"] = Value::Null;
                registry.entries[0]["initial_lifecycle_seq"] = Value::Null;
                registry.save().unwrap();
            }
            Fixture::commit_at(&fixture.source, "task", "implemented\n");
            Fixture::commit_at(&fixture.main, "latest-target", "target changed\n");
            let mut native = fixture.agent(12, "idle");
            native["result"]["agent"]
                .as_object_mut()
                .unwrap()
                .remove("agent_session");
            native["result"]["agent"]
                .as_object_mut()
                .unwrap()
                .remove("state_change_seq");
            let mut request = Fake::default();
            request.json(native.clone());
            request.json(fixture.prompt_result());
            assert_eq!(
                act_with(&fixture.main, ID, Action::RequestMerge, &mut request)
                    .unwrap()
                    .status,
                WorktreeStatus::MergeRequested
            );
            {
                let registry = Registry::open(&fixture.main).unwrap();
                assert_eq!(registry.entries[0]["merge_request"]["provenance"], "manual");
                assert!(registry.entries[0]["merge_request"]["agent_session"].is_null());
                assert!(registry.entries[0]["merge_request"]["lifecycle_seq"].is_null());
            }
            git(&fixture.source, &["merge", "--no-edit", "trunk"]).unwrap();
            let mut complete = Fake::default();
            complete.json(native.clone());
            complete.json(native.clone());
            let rows =
                reconcile_with(&fixture.main, CleanupPolicy::default(), &mut complete).unwrap();
            assert_eq!(rows[0].status, WorktreeStatus::Integrated);
            assert_eq!(
                tip(&fixture.main, "HEAD").unwrap(),
                tip(&fixture.source, "HEAD").unwrap()
            );
            let mut automatic_cleanup = Fake::legacy_closed();
            automatic_cleanup.json(native);
            let rows = reconcile_with(
                &fixture.main,
                CleanupPolicy {
                    auto_merge: true,
                    auto_merge_children: true,
                    auto_close_workspace: true,
                    auto_remove_checkout: true,
                },
                &mut automatic_cleanup,
            )
            .unwrap();
            assert_eq!(rows[0].status, WorktreeStatus::Integrated);
            assert!(rows[0].detail.is_some());
            assert!(!automatic_cleanup.calls.iter().any(|args| {
                args.get(1)
                    .is_some_and(|s| s == "remove" || s == "close" || s == "prompt")
            }));
            assert!(fixture.source.exists());
        }
    }

    #[test]
    fn manual_dispatch_and_finalization_allow_null_or_omitted_optional_checkout_paths() {
        for keys in [
            vec!["foreground_cwd"],
            vec!["cwd"],
            vec!["cwd", "foreground_cwd"],
        ] {
            for omitted in [false, true] {
                let fixture = Fixture::managed();
                Fixture::commit_at(&fixture.source, "task", "implemented\n");
                Fixture::commit_at(&fixture.main, "latest-target", "target changed\n");
                let mut native = fixture.agent(12, "idle");
                for key in &keys {
                    if omitted {
                        native["result"]["agent"]
                            .as_object_mut()
                            .unwrap()
                            .remove(*key);
                    } else {
                        native["result"]["agent"][*key] = Value::Null;
                    }
                }
                let mut request = Fake::default();
                request.json(native.clone());
                request.json(fixture.prompt_result());
                assert_eq!(
                    act_with(&fixture.main, ID, Action::RequestMerge, &mut request)
                        .unwrap()
                        .status,
                    WorktreeStatus::MergeRequested,
                    "{keys:?}, omitted={omitted}"
                );
                assert_eq!(request.calls[1][..3], ["agent", "prompt", "wTask:p1"]);
                git(&fixture.source, &["merge", "--no-edit", "trunk"]).unwrap();
                let mut complete = Fake::default();
                complete.json(native.clone());
                complete.json(native);
                let rows =
                    reconcile_with(&fixture.main, CleanupPolicy::default(), &mut complete).unwrap();
                assert_eq!(
                    rows[0].status,
                    WorktreeStatus::Integrated,
                    "{keys:?}, omitted={omitted}"
                );
                assert_eq!(
                    tip(&fixture.main, "HEAD").unwrap(),
                    tip(&fixture.source, "HEAD").unwrap()
                );
            }
        }
    }

    #[test]
    fn manual_dispatch_and_finalization_refuse_invalid_or_mismatched_reported_paths() {
        for key in ["cwd", "foreground_cwd"] {
            for mode in [
                "wrong_checkout",
                "invalid_type",
                "empty",
                "relative",
                "unresolvable",
            ] {
                let fixture = Fixture::managed();
                Fixture::commit_at(&fixture.source, "task", "implemented\n");
                Fixture::commit_at(&fixture.main, "latest-target", "target changed\n");
                let mut native = fixture.agent(12, "idle");
                let (value, expected) = match mode {
                    "wrong_checkout" => (json!(fixture.main), "does not match recorded checkout"),
                    "invalid_type" => (json!(42), "is invalid"),
                    "empty" => (json!(""), "is invalid"),
                    "relative" => (json!("relative/path"), "is invalid"),
                    _ => (
                        json!(fixture.directory.join("missing-checkout")),
                        "cannot be resolved",
                    ),
                };
                native["result"]["agent"][key] = value;
                let mut invalid_dispatch = Fake::default();
                invalid_dispatch.json(native.clone());
                let message = act_with(
                    &fixture.main,
                    ID,
                    Action::RequestMerge,
                    &mut invalid_dispatch,
                )
                .unwrap_err()
                .to_string();
                assert!(
                    message.contains(key) && message.contains(expected),
                    "{mode}: {message}"
                );
                assert_eq!(invalid_dispatch.calls.len(), 1);
                {
                    let registry = Registry::open(&fixture.main).unwrap();
                    assert!(registry.entries[0]["merge_request"].is_null());
                }
                fixture.request();
                git(&fixture.source, &["merge", "--no-edit", "trunk"]).unwrap();
                let before = tip(&fixture.main, "HEAD").unwrap();
                let mut invalid_completion = Fake::default();
                invalid_completion.json(native);
                let rows = reconcile_with(
                    &fixture.main,
                    CleanupPolicy::default(),
                    &mut invalid_completion,
                )
                .unwrap();
                assert_eq!(rows[0].status, WorktreeStatus::MergeRequested);
                let message = rows[0].detail.as_ref().unwrap();
                assert!(
                    message.contains(key) && message.contains(expected),
                    "{mode}: {message}"
                );
                assert_eq!(tip(&fixture.main, "HEAD").unwrap(), before);
            }
        }
    }

    #[test]
    fn automatic_cleanup_defers_when_optional_checkout_path_evidence_is_unavailable() {
        for key in ["cwd", "foreground_cwd"] {
            for omitted in [false, true] {
                let fixture = Fixture::managed();
                fixture.integrated();
                let mut native = fixture.agent(12, "done");
                if omitted {
                    native["result"]["agent"]
                        .as_object_mut()
                        .unwrap()
                        .remove(key);
                } else {
                    native["result"]["agent"][key] = Value::Null;
                }
                let mut executor = Fake::default();
                executor.json(native);
                let rows = reconcile_with(
                    &fixture.main,
                    CleanupPolicy {
                        auto_merge: true,
                        auto_merge_children: true,
                        auto_close_workspace: true,
                        auto_remove_checkout: true,
                    },
                    &mut executor,
                )
                .unwrap();
                assert_eq!(rows[0].status, WorktreeStatus::Integrated);
                assert!(
                    rows[0]
                        .detail
                        .as_ref()
                        .unwrap()
                        .contains(&format!("native agent {key} is unavailable"))
                );
                assert_eq!(executor.calls.len(), 3);
                assert_eq!(executor.calls[2][..2], ["agent", "get"]);
                assert!(fixture.source.exists());
            }
        }
    }
}
