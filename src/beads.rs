//! Browsing, closed-parent repair, and explicit issue mutations through the Beads CLI.

use std::io;
use std::process::{Command, Output};

use serde_json::Value;

pub const BEAD_TYPES: &[&str] = &[
    "task",
    "bug",
    "feature",
    "epic",
    "chore",
    "decision",
    "spike",
    "story",
    "milestone",
];
pub const READY_LABEL: &str = "ontrack:ready";
pub const READY_TO_MERGE_LABEL: &str = "ontrack:ready-to-merge";
pub const HARNESS_PREFIX: &str = "ontrack:agent:";
pub const UNASSIGNED_LABEL: &str = "ontrack:unassigned";
pub const HARNESSES: &[&str] = &[
    "pi",
    "claude",
    "codex",
    "gemini",
    "cursor",
    "devin",
    "agy",
    "cline",
    "omp",
    "mastracode",
    "opencode",
    "copilot",
    "kimi",
    "kiro",
    "droid",
    "amp",
    "grok",
    "hermes",
    "kilo",
    "qodercli",
    "qwen",
    "letta",
    "maki",
    "muse",
];

#[derive(Debug, Clone)]
pub struct CreateBead {
    pub title: String,
    pub description: String,
    pub issue_type: String,
    pub priority: u8,
    pub harness: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateBead {
    pub id: String,
    pub fields: CreateBead,
    pub previous_harness: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Bead {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: u64,
    pub data: Value,
    pub dependency_ready: bool,
    pub blocked_by: Vec<String>,
}

const LIST_ARGS: &[&str] = &[
    "--readonly",
    "--no-color",
    "list",
    "--all",
    "--limit",
    "0",
    "--json",
];

pub fn list_beads() -> io::Result<Vec<Bead>> {
    parse_list(&run_bd(LIST_ARGS)?)
}

const READY_ARGS: &[&str] = &[
    "--readonly",
    "--no-color",
    "ready",
    "--limit",
    "0",
    "--json",
];
const BLOCKED_ARGS: &[&str] = &["--readonly", "--no-color", "blocked", "--json"];

/// Fetch graph eligibility together with the browser snapshot. Any failed read
/// rejects the snapshot so scheduling never treats an unknown graph as ready.
pub fn list_browser_beads() -> io::Result<Vec<Bead>> {
    list_browser_beads_with(run_bd)
}

fn list_browser_beads_with(
    mut run: impl FnMut(&[&str]) -> io::Result<Vec<u8>>,
) -> io::Result<Vec<Bead>> {
    let mut beads = parse_list(&run(LIST_ARGS)?)?;
    let closed: std::collections::HashSet<_> = beads
        .iter()
        .filter(|bead| bead.status == "closed")
        .map(|bead| bead.id.as_str())
        .collect();
    let mut repaired = false;
    for child in &beads {
        let Some(parent) = child.data["parent"].as_str() else {
            continue;
        };
        if child.status == "closed" || !closed.contains(parent) {
            continue;
        }
        validate_id(&child.id)?;
        validate_id(parent)?;
        let current = parse_show(&run(&show_args(&format!("--id={}", child.id)))?, &child.id)?;
        let status = required_string(&current, "status")?;
        if status == "closed" || current["parent"].as_str() != Some(parent) {
            continue;
        }
        let current_parent = parse_show(&run(&show_args(&format!("--id={parent}")))?, parent)?;
        if required_string(&current_parent, "status")? != "closed" {
            continue;
        }
        // shortcut: bd has no parent/revision guard; use one if the CLI adds it.
        run(&[
            "--no-color",
            "update",
            "--parent=",
            "--if-status",
            &status,
            "--json",
            "--",
            &child.id,
        ])?;
        repaired = true;
    }
    if repaired {
        beads = parse_list(&run(LIST_ARGS)?)?;
    }
    let ready = parse_records(&run(READY_ARGS)?)?;
    let blocked = parse_records(&run(BLOCKED_ARGS)?)?;
    apply_readiness(&mut beads, &ready, &blocked)?;
    Ok(beads)
}

fn apply_readiness(beads: &mut [Bead], ready: &[Value], blocked: &[Value]) -> io::Result<()> {
    let ready_ids = ready
        .iter()
        .map(|record| required_string(record, "id"))
        .collect::<io::Result<std::collections::HashSet<_>>>()?;
    let mut blockers = std::collections::HashMap::new();
    for record in blocked {
        let id = required_string(record, "id")?;
        let ids = record["blocked_by"]
            .as_array()
            .ok_or_else(|| invalid_data("Blocked Bead is missing blocked_by IDs"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| invalid_data("Invalid blocking Bead ID"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        blockers.insert(id, ids);
    }
    for bead in beads {
        bead.blocked_by = blockers.remove(&bead.id).unwrap_or_default();
        bead.dependency_ready = ready_ids.contains(&bead.id) && bead.blocked_by.is_empty();
    }
    Ok(())
}

pub fn dependency_ready(id: &str) -> io::Result<bool> {
    validate_id(id)?;
    let records = parse_records(&run_bd(READY_ARGS)?)?;
    let ids = records
        .iter()
        .map(|record| required_string(record, "id"))
        .collect::<io::Result<std::collections::HashSet<_>>>()?;
    Ok(ids.contains(id))
}

pub fn show_bead(id: &str) -> io::Result<Value> {
    validate_id(id)?;
    let id_arg = format!("--id={id}");
    let args = show_args(&id_arg);
    parse_show(&run_bd(&args)?, id)
}

pub fn create_bead(input: &CreateBead) -> io::Result<String> {
    let args = create_args(input)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    parse_created_id(&run_bd(&args)?)
}

/// Create one reviewed planning item. Each CLI value stays a separate argv
/// element, including agent-authored text that may contain shell metacharacters.
pub fn create_planned(
    item: &crate::planning::Item,
    issue_type: &str,
    parent: Option<&str>,
) -> io::Result<String> {
    let args = planned_args(item, issue_type, parent)?;
    parse_created_id(&run_bd(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?)
}

fn planned_args(
    item: &crate::planning::Item,
    issue_type: &str,
    parent: Option<&str>,
) -> io::Result<Vec<String>> {
    if !matches!(issue_type, "task" | "epic") || item.acceptance.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid planned Bead",
        ));
    }
    let input = CreateBead {
        title: item.title.clone(),
        description: item.description.clone(),
        issue_type: issue_type.into(),
        priority: item.priority,
        harness: None,
    };
    let mut args = create_args(&input)?;
    args.push(format!("--acceptance={}", item.acceptance));
    if let Some(parent) = parent {
        validate_id(parent)?;
        args.push(format!("--parent={parent}"));
        args.push("--no-inherit-labels".into());
    }
    Ok(args)
}

pub fn create_child(input: &CreateBead, parent_id: &str) -> io::Result<String> {
    validate_id(parent_id)?;
    let parent = show_bead(parent_id)?;
    let inherited = labels(&parent)
        .filter(|label| {
            *label != READY_LABEL
                && *label != READY_TO_MERGE_LABEL
                && *label != UNASSIGNED_LABEL
                && !label.starts_with(HARNESS_PREFIX)
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let args = create_child_args(input, parent_id, &inherited)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    parse_created_id(&run_bd(&args)?)
}

fn create_child_args(
    input: &CreateBead,
    parent_id: &str,
    inherited: &[String],
) -> io::Result<Vec<String>> {
    validate_id(parent_id)?;
    let mut args = create_args(input)?;
    args.push(format!("--parent={parent_id}"));
    args.push("--no-inherit-labels".into());
    for label in inherited {
        if label == READY_LABEL
            || label == READY_TO_MERGE_LABEL
            || label == UNASSIGNED_LABEL
            || label.starts_with(HARNESS_PREFIX)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "MXO parent state cannot be inherited by child",
            ));
        }
        args.push(format!("--labels={label}"));
    }
    Ok(args)
}

/// Beads cannot add a blocking edge from a parent to its descendant: that
/// would cascade the blocked status back to the child. Check the hierarchy
/// itself before handing a parent to an agent.
pub fn open_descendants(parent_id: &str) -> io::Result<Vec<String>> {
    validate_id(parent_id)?;
    Ok(open_descendants_in(&list_beads()?, parent_id))
}

pub(crate) fn open_descendants_in(beads: &[Bead], parent_id: &str) -> Vec<String> {
    let mut descendants = Vec::new();
    let mut frontier = vec![parent_id];
    let mut visited = std::collections::HashSet::new();
    while let Some(parent) = frontier.pop() {
        if !visited.insert(parent) {
            continue;
        }
        for bead in beads {
            if bead.data["parent"].as_str() == Some(parent) {
                frontier.push(&bead.id);
                if bead.status != "closed" {
                    descendants.push(bead.id.clone());
                }
            }
        }
    }
    descendants
}

pub fn update_bead(input: &UpdateBead) -> io::Result<()> {
    let args = update_args(input)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_bd(&args)?;
    Ok(())
}

pub fn set_ready(id: &str, ready: bool) -> io::Result<()> {
    set_ready_many(&[id.to_owned()], ready)
}

/// Move only the workflow state; agent sessions and unrelated issue fields are untouched.
pub fn move_bead(id: &str, target: &str) -> io::Result<()> {
    validate_id(id)?;
    validate_move_target(target)?;
    if target == "closed" {
        return crate::worktrees::close_integrated_bead(id);
    }
    move_bead_with(&std::env::current_dir()?, id, target, show_bead, |args| {
        run_bd(args).map(|_| ())
    })
}

pub(crate) fn move_bead_with(
    cwd: &std::path::Path,
    id: &str,
    target: &str,
    show: impl FnOnce(&str) -> io::Result<Value>,
    update: impl FnOnce(&[&str]) -> io::Result<()>,
) -> io::Result<()> {
    validate_id(id)?;
    validate_move_target(target)?;
    if target == "closed" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Closure requires the integration gate",
        ));
    }
    crate::worktrees::with_existing_registry_lock(cwd, || {
        let live = show(id)?;
        let args = move_args(id, target, &live)?;
        update(&args.iter().map(String::as_str).collect::<Vec<_>>())
    })
}

fn validate_move_target(target: &str) -> io::Result<()> {
    if matches!(
        target,
        "open" | "ready" | "blocked" | "in_progress" | "ready_to_merge" | "closed"
    ) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid Kanban target",
        ))
    }
}

fn move_args(id: &str, target: &str, live: &Value) -> io::Result<Vec<String>> {
    validate_id(id)?;
    validate_move_target(target)?;
    if live["id"] != id || target == "closed" {
        return Err(invalid_data(
            "Move requires a matching live Bead; closure requires integration proof",
        ));
    }
    let previous = required_string(live, "status")?;
    let status = match target {
        "ready" => "open",
        "ready_to_merge" => "in_progress",
        other => other,
    };
    let mut args = vec![
        "--no-color".into(),
        "update".into(),
        format!("--if-status={previous}"),
        format!("--status={status}"),
        "--json".into(),
    ];
    for (label, enabled) in [
        (READY_LABEL, target == "ready"),
        (READY_TO_MERGE_LABEL, target == "ready_to_merge"),
    ] {
        args.push(format!(
            "--{}-label={label}",
            if enabled { "add" } else { "remove" }
        ));
    }
    if previous == "deferred"
        || live["defer_until"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    {
        args.push("--defer=".into());
    }
    args.extend(["--".into(), id.into()]);
    Ok(args)
}

/// Executor-only: the caller must freshly verify tracked Git integration first.
pub(crate) fn close_verified_bead(id: &str, expected: &Value) -> io::Result<()> {
    let live = show_bead(id)?;
    let args = close_verified_args(id, expected, &live)?;
    run_bd(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    Ok(())
}

fn close_verified_args(id: &str, expected: &Value, live: &Value) -> io::Result<Vec<String>> {
    // shortcut: bd has no label CAS; use an atomic label guard when its CLI supports one.
    if expected["id"] != id
        || live["status"] != expected["status"]
        || ready_to_merge_record(live) != ready_to_merge_record(expected)
    {
        return Err(io::Error::other(
            "Live Bead workflow state changed; closure cancelled",
        ));
    }
    let mut args = move_args(id, "in_progress", live)?;
    // bd update retains normal close dependency guards and supports a status CAS;
    // bd close does not support --if-status.
    let status = args
        .iter_mut()
        .find(|arg| arg.starts_with("--status="))
        .unwrap();
    *status = "--status=closed".into();
    Ok(args)
}

fn set_ready_many(ids: &[String], ready: bool) -> io::Result<()> {
    for id in ids {
        validate_id(id)?;
    }
    let flag = if ready {
        "--add-label"
    } else {
        "--remove-label"
    };
    let mut args = vec!["--no-color", "update", flag, READY_LABEL];
    if ready {
        args.extend([
            "--if-status",
            "open",
            "--status",
            "open",
            "--remove-label",
            READY_TO_MERGE_LABEL,
        ]);
    }
    args.push("--");
    args.extend(ids.iter().map(String::as_str));
    run_bd(&args)?;
    Ok(())
}

/// Change an open issue and its open descendants in one CLI invocation.
/// Beads applies updates per ID, so callers must refresh after any error.
pub fn set_tree_ready(
    parent_id: &str,
    ready: bool,
    known_handoffs: &std::collections::HashSet<String>,
    protected_handoffs: &std::collections::HashSet<String>,
) -> io::Result<Vec<String>> {
    validate_id(parent_id)?;
    let beads = list_beads()?;
    let ids = tree_ready_targets_in(&beads, parent_id, ready, known_handoffs, protected_handoffs)?;
    set_ready_many(&ids, ready)?;
    Ok(ids)
}

fn tree_ready_targets_in(
    beads: &[Bead],
    parent_id: &str,
    ready: bool,
    known_handoffs: &std::collections::HashSet<String>,
    protected_handoffs: &std::collections::HashSet<String>,
) -> io::Result<Vec<String>> {
    let parent = beads
        .iter()
        .find(|bead| bead.id == parent_id)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Selected issue is no longer listed",
            )
        })?;
    if parent.status != "open" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only open issues can change descendant Ready markers",
        ));
    }
    let descendants: std::collections::HashSet<_> =
        open_descendants_in(beads, parent_id).into_iter().collect();
    let mut ids = vec![parent_id.to_owned()];
    ids.extend(
        beads
            .iter()
            .filter(|bead| {
                descendants.contains(&bead.id)
                    && bead.status == "open"
                    && (!ready || !known_handoffs.contains(&bead.id))
                    && (ready || !protected_handoffs.contains(&bead.id))
                    && (ready != is_ready(bead))
            })
            .map(|bead| bead.id.clone()),
    );
    Ok(ids)
}

pub fn is_ready(bead: &Bead) -> bool {
    bead.status == "open" && labels(&bead.data).any(|label| label == READY_LABEL)
}

pub fn ready_record(data: &Value) -> bool {
    data["status"] == "open"
        && data["issue_type"] != "epic"
        && labels(data).any(|label| label == READY_LABEL)
}

pub fn ready_to_merge_record(data: &Value) -> bool {
    data["status"] == "in_progress" && labels(data).any(|label| label == READY_TO_MERGE_LABEL)
}

pub fn harness(data: &Value) -> Option<&str> {
    labels(data)
        .filter_map(|label| label.strip_prefix(HARNESS_PREFIX))
        .find(|kind| HARNESSES.contains(kind))
}

pub fn explicitly_unassigned(data: &Value) -> bool {
    labels(data).any(|label| label == UNASSIGNED_LABEL)
}

pub fn has_harness_label(data: &Value) -> bool {
    labels(data).any(|label| label.starts_with(HARNESS_PREFIX))
}

pub(crate) fn labels(data: &Value) -> impl Iterator<Item = &str> {
    data.get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// Permanently delete only this issue, never cascading to dependent issues.
pub fn delete_bead(id: &str) -> io::Result<()> {
    run_bd(&delete_args(id)?)?;
    Ok(())
}

fn create_args(input: &CreateBead) -> io::Result<Vec<String>> {
    let title = validated_title(input)?;
    let mut args = vec![
        "--no-color".to_owned(),
        "create".to_owned(),
        format!("--title={title}"),
        format!("--description={}", input.description),
        format!("--type={}", input.issue_type),
        format!("--priority={}", input.priority),
        "--json".to_owned(),
    ];
    args.push(match &input.harness {
        Some(harness) => format!("--labels={HARNESS_PREFIX}{harness}"),
        None => format!("--labels={UNASSIGNED_LABEL}"),
    });
    Ok(args)
}

fn update_args(input: &UpdateBead) -> io::Result<Vec<String>> {
    validate_id(&input.id)?;
    let title = validated_title(&input.fields)?;
    let mut args = vec![
        "--no-color".to_owned(),
        "update".to_owned(),
        format!("--title={title}"),
        format!("--description={}", input.fields.description),
        format!("--type={}", input.fields.issue_type),
        format!("--priority={}", input.fields.priority),
        "--json".to_owned(),
    ];
    if input.fields.harness != input.previous_harness {
        if let Some(old) = &input.previous_harness {
            args.push(format!("--remove-label={HARNESS_PREFIX}{old}"));
        }
        if let Some(new) = &input.fields.harness {
            args.push(format!("--add-label={HARNESS_PREFIX}{new}"));
        }
    }
    if input.fields.harness.is_none() {
        args.push(format!("--add-label={UNASSIGNED_LABEL}"));
    } else {
        args.push(format!("--remove-label={UNASSIGNED_LABEL}"));
    }
    args.extend(["--".to_owned(), input.id.clone()]);
    Ok(args)
}

fn validated_title(input: &CreateBead) -> io::Result<&str> {
    let title = input.title.trim();
    if title.is_empty()
        || input
            .title
            .contains(['\n', '\r', '\0', '\u{85}', '\u{2028}', '\u{2029}'])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Beads title must be nonempty and single-line",
        ));
    }
    if input.priority > 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Beads priority must be between 0 and 4",
        ));
    }
    if !BEAD_TYPES.contains(&input.issue_type.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported Beads issue type",
        ));
    }
    if input
        .harness
        .as_deref()
        .is_some_and(|h| !HARNESSES.contains(&h))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported agent harness",
        ));
    }
    Ok(title)
}

fn delete_args(id: &str) -> io::Result<[&str; 6]> {
    validate_id(id)?;
    Ok(["--no-color", "delete", "--force", "--json", "--", id])
}

fn show_args(id_arg: &str) -> [&str; 8] {
    [
        "--readonly",
        "--no-color",
        "show",
        id_arg,
        "--json",
        "--long",
        "--include-comments",
        "--include-dependents",
    ]
}

pub(crate) fn validate_id(id: &str) -> io::Result<()> {
    if id.is_empty()
        || id.starts_with('-')
        || id.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Beads issue ID must be nonempty, not start with '-', and contain no whitespace or control characters",
        ));
    }
    Ok(())
}

fn run_bd(args: &[&str]) -> io::Result<Vec<u8>> {
    run_bd_with(args, |args| Command::new("bd").args(args).output())
}

fn run_bd_with(
    args: &[&str],
    execute: impl FnOnce(&[&str]) -> io::Result<Output>,
) -> io::Result<Vec<u8>> {
    let output = execute(args)
        .map_err(|error| io::Error::new(error.kind(), format!("Could not run bd: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!(
            "bd failed ({}): {}",
            output.status,
            stderr.trim()
        )));
    }
    Ok(output.stdout)
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn parse_records(bytes: &[u8]) -> io::Result<Vec<Value>> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| invalid_data(format!("Invalid bd JSON: {error}")))?;
    match value {
        Value::Array(records) => Ok(records),
        _ => Err(invalid_data("Expected a JSON array from bd")),
    }
}

fn required_string(record: &Value, field: &str) -> io::Result<String> {
    record
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid_data(format!("Beads record has missing or invalid {field}")))
}

fn parse_created_id(bytes: &[u8]) -> io::Result<String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| invalid_data(format!("Invalid bd JSON: {error}")))?;
    if !value.is_object() {
        return Err(invalid_data("Expected a JSON object from bd create"));
    }
    let id = required_string(&value, "id")?;
    validate_id(&id).map_err(|error| invalid_data(format!("Invalid created issue ID: {error}")))?;
    Ok(id)
}

fn parse_list(bytes: &[u8]) -> io::Result<Vec<Bead>> {
    parse_records(bytes)?
        .into_iter()
        .map(|data| {
            let id = required_string(&data, "id")?;
            let title = required_string(&data, "title")?;
            let status = required_string(&data, "status")?;
            let priority = match data.get("priority") {
                None | Some(Value::Null) => 2,
                Some(value) => value
                    .as_u64()
                    .ok_or_else(|| invalid_data("Beads record has invalid priority"))?,
            };
            Ok(Bead {
                id,
                title,
                status,
                priority,
                dependency_ready: false,
                blocked_by: Vec::new(),
                data,
            })
        })
        .collect()
}

fn parse_show(bytes: &[u8], id: &str) -> io::Result<Value> {
    let mut found = None;
    for record in parse_records(bytes)? {
        if required_string(&record, "id")? == id {
            if found.is_some() {
                return Err(invalid_data(format!("bd returned duplicate issue {id}")));
            }
            found = Some(record);
        }
    }
    found.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("bd did not return requested issue {id}"),
        )
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn kanban_moves_are_guarded_and_only_change_workflow_fields() {
        let live = serde_json::json!({"id":"test-1", "status":"closed", "labels":["team:core"], "metadata":{"keep":true}});
        for (target, status) in [
            ("open", "open"),
            ("ready", "open"),
            ("blocked", "blocked"),
            ("in_progress", "in_progress"),
            ("ready_to_merge", "in_progress"),
        ] {
            let args = super::move_args("test-1", target, &live).unwrap();
            assert!(args.contains(&"--if-status=closed".into()));
            assert!(args.contains(&format!("--status={status}")));
            for (label, enabled) in [
                (super::READY_LABEL, target == "ready"),
                (super::READY_TO_MERGE_LABEL, target == "ready_to_merge"),
            ] {
                assert!(args.contains(&format!(
                    "--{}-label={label}",
                    if enabled { "add" } else { "remove" }
                )));
            }
            assert!(!args.iter().any(|arg| arg.contains("metadata")
                || arg.contains("set-labels")
                || arg == "--force"));
            assert_eq!(&args[args.len() - 2..], ["--", "test-1"]);
        }
        let deferred = serde_json::json!({"id":"test-1", "status":"deferred"});
        assert!(
            super::move_args("test-1", "open", &deferred)
                .unwrap()
                .contains(&"--defer=".into())
        );
        assert!(super::move_args("test-1", "closed", &live).is_err());
        assert!(super::move_args("wrong", "open", &live).is_err());
        assert!(super::move_bead("--force", "closed").is_err());
        assert!(super::move_bead("test-1", "invalid").is_err());
    }

    #[test]
    fn ready_to_merge_is_exact_and_verified_closure_guards_the_live_state() {
        for status in ["open", "blocked", "deferred", "in_progress", "closed"] {
            for marked in [false, true] {
                let live = serde_json::json!({"id":"test-1", "status":status, "labels":if marked {vec![super::READY_TO_MERGE_LABEL]} else {vec![]}});
                let ready = status == "in_progress" && marked;
                assert_eq!(super::ready_to_merge_record(&live), ready);
                let args = super::close_verified_args("test-1", &live, &live).unwrap();
                assert!(args.contains(&format!("--if-status={status}")));
                assert!(args.contains(&"--status=closed".into()));
                assert!(!args.iter().any(|arg| arg == "--force"));
            }
        }
        let ready = serde_json::json!({"id":"test-1", "status":"in_progress", "labels":[super::READY_TO_MERGE_LABEL]});
        for changed in [
            serde_json::json!({"id":"test-1", "status":"open"}),
            serde_json::json!({"id":"test-1", "status":"in_progress"}),
        ] {
            assert!(super::close_verified_args("test-1", &ready, &changed).is_err());
        }
    }
    use super::*;
    use serde_json::json;

    fn browser_script(steps: Vec<(Vec<&str>, io::Result<Value>)>) -> io::Result<Vec<Bead>> {
        let mut steps = std::collections::VecDeque::from(steps);
        let result = list_browser_beads_with(|args| {
            let (expected, response) = steps.pop_front().expect("unexpected bd call");
            assert_eq!(args, expected);
            response.map(|value| serde_json::to_vec(&value).unwrap())
        });
        assert!(steps.is_empty(), "missing bd calls: {steps:?}");
        result
    }

    #[test]
    fn browser_repairs_unfinished_children_and_reloads_before_readiness() {
        for status in ["open", "in_progress", "blocked", "deferred"] {
            let parent = json!({"id":"parent", "title":"Parent", "status":"closed"});
            let child = json!({"id":"child;$(ignored)", "title":"Child", "status":status,
                "parent":"parent", "labels":[READY_LABEL],
                "dependencies":[{"depends_on_id":"other", "type":"blocks"}]});
            let mut detached = child.clone();
            detached.as_object_mut().unwrap().remove("parent");
            let beads = browser_script(vec![
                (LIST_ARGS.to_vec(), Ok(json!([parent, child]))),
                (
                    show_args("--id=child;$(ignored)").to_vec(),
                    Ok(json!([child])),
                ),
                (show_args("--id=parent").to_vec(), Ok(json!([parent]))),
                (
                    vec![
                        "--no-color",
                        "update",
                        "--parent=",
                        "--if-status",
                        status,
                        "--json",
                        "--",
                        "child;$(ignored)",
                    ],
                    Ok(json!([])),
                ),
                (LIST_ARGS.to_vec(), Ok(json!([parent, detached]))),
                (READY_ARGS.to_vec(), Ok(json!([]))),
                (
                    BLOCKED_ARGS.to_vec(),
                    Ok(json!([
                        {"id":"child;$(ignored)", "blocked_by":["other"]}
                    ])),
                ),
            ])
            .unwrap();
            assert_eq!(beads[1].data, detached);
            assert_eq!(beads[1].blocked_by, ["other"]);
            assert!(!beads[1].dependency_ready);
        }
    }

    #[test]
    fn browser_preserves_history_active_parents_and_unknown_parents() {
        let snapshot = json!([
            {"id":"closed-parent", "title":"Closed", "status":"closed"},
            {"id":"active-parent", "title":"Active", "status":"in_progress"},
            {"id":"history", "title":"History", "status":"closed", "parent":"closed-parent"},
            {"id":"active-child", "title":"Child", "status":"open", "parent":"active-parent"},
            {"id":"unknown-child", "title":"Child", "status":"open", "parent":"missing"},
            {"id":"root", "title":"Root", "status":"open"}
        ]);
        let beads = browser_script(vec![
            (LIST_ARGS.to_vec(), Ok(snapshot.clone())),
            (READY_ARGS.to_vec(), Ok(json!([]))),
            (BLOCKED_ARGS.to_vec(), Ok(json!([]))),
        ])
        .unwrap();
        assert_eq!(
            json!(beads.iter().map(|bead| &bead.data).collect::<Vec<_>>()),
            snapshot
        );
    }

    #[test]
    fn browser_revalidates_child_and_parent_before_detaching() {
        let parent = json!({"id":"parent", "title":"Parent", "status":"closed"});
        let child = json!({"id":"child", "title":"Child", "status":"open", "parent":"parent"});
        for change in [
            json!({"status":"closed"}),
            json!({"parent":"new-parent"}),
            json!({"parent":null}),
            json!({"parent_reopened":true}),
        ] {
            let mut current = child.clone();
            let reopened = change["parent_reopened"] == true;
            if !reopened {
                current
                    .as_object_mut()
                    .unwrap()
                    .extend(change.as_object().unwrap().clone());
            }
            let mut steps = vec![
                (LIST_ARGS.to_vec(), Ok(json!([parent, child]))),
                (show_args("--id=child").to_vec(), Ok(json!([current]))),
            ];
            if reopened {
                steps.push((
                    show_args("--id=parent").to_vec(),
                    Ok(json!([{"id":"parent", "status":"open"}])),
                ));
            }
            steps.extend([
                (READY_ARGS.to_vec(), Ok(json!([]))),
                (BLOCKED_ARGS.to_vec(), Ok(json!([]))),
            ]);
            browser_script(steps).unwrap();
        }
    }

    #[test]
    fn browser_repair_errors_reject_snapshot_and_next_refresh_retries() {
        let parent = json!({"id":"parent", "title":"Parent", "status":"closed"});
        let child = json!({"id":"child", "title":"Child", "status":"open", "parent":"parent"});
        let calls = [
            LIST_ARGS.to_vec(),
            show_args("--id=child").to_vec(),
            show_args("--id=parent").to_vec(),
            vec![
                "--no-color",
                "update",
                "--parent=",
                "--if-status",
                "open",
                "--json",
                "--",
                "child",
            ],
            LIST_ARGS.to_vec(),
        ];
        let responses = [
            json!([parent, child]),
            json!([child]),
            json!([parent]),
            json!([]),
            json!([]),
        ];
        for failure in 0..calls.len() {
            let mut steps = calls[..failure]
                .iter()
                .cloned()
                .zip(responses[..failure].iter().cloned().map(Ok))
                .collect::<Vec<_>>();
            steps.push((
                calls[failure].clone(),
                Err(io::Error::other("repair failed")),
            ));
            assert_eq!(
                browser_script(steps).unwrap_err().to_string(),
                "repair failed"
            );
        }
        let mut steps = calls
            .into_iter()
            .zip(responses.into_iter().map(Ok))
            .collect::<Vec<_>>();
        steps.extend([
            (READY_ARGS.to_vec(), Ok(json!([]))),
            (BLOCKED_ARGS.to_vec(), Ok(json!([]))),
        ]);
        assert!(browser_script(steps).unwrap().is_empty());
    }

    #[test]
    fn graph_readiness_is_authoritative_and_preserves_ready_intent() {
        let mut beads = parse_list(
            br#"[
            {"id":"waiting","title":"Waiting","status":"open","labels":["ontrack:ready"]},
            {"id":"free","title":"Free","status":"open","labels":["ontrack:ready"]},
            {"id":"deferred","title":"Later","status":"open","labels":["ontrack:ready"]}
        ]"#,
        )
        .unwrap();
        assert!(beads.iter().all(|bead| !bead.dependency_ready));
        apply_readiness(
            &mut beads,
            &[json!({"id":"free"})],
            &[json!({"id":"waiting","blocked_by":["base", "other"]})],
        )
        .unwrap();
        assert!(!beads[0].dependency_ready);
        assert_eq!(beads[0].blocked_by, ["base", "other"]);
        assert!(beads[1].dependency_ready);
        assert!(!beads[2].dependency_ready);
        assert!(beads.iter().all(is_ready));
        apply_readiness(
            &mut beads,
            &[json!({"id":"waiting"}), json!({"id":"free"})],
            &[],
        )
        .unwrap();
        assert!(beads[0].dependency_ready);
        assert!(beads[0].blocked_by.is_empty());
        // A dependency added between the ready and blocked reads wins.
        apply_readiness(
            &mut beads,
            &[json!({"id":"free"})],
            &[json!({"id":"free","blocked_by":["new"]})],
        )
        .unwrap();
        assert!(!beads[1].dependency_ready);
    }

    #[test]
    fn readiness_queries_are_readonly_uncapped_and_reject_invalid_records() {
        assert_eq!(
            READY_ARGS,
            [
                "--readonly",
                "--no-color",
                "ready",
                "--limit",
                "0",
                "--json"
            ]
        );
        assert_eq!(
            BLOCKED_ARGS,
            ["--readonly", "--no-color", "blocked", "--json"]
        );
        for bad in [
            json!({}),
            json!({"id":"x"}),
            json!({"id":"x","blocked_by":[null]}),
        ] {
            assert!(apply_readiness(&mut [], &[], &[bad]).is_err());
        }
        assert!(apply_readiness(&mut [], &[json!({})], &[]).is_err());
    }

    #[test]
    fn child_creation_keeps_parent_and_user_text_in_separate_arguments() {
        let args = create_child_args(
            &CreateBead {
                title: "$(touch /tmp/unwanted)".into(),
                description: "--priority=0\nsecond line".into(),
                issue_type: "task".into(),
                priority: 2,
                harness: None,
            },
            "ontrack-parent",
            &[],
        )
        .unwrap();
        assert!(args.contains(&"--parent=ontrack-parent".to_owned()));
        assert!(args.contains(&"--no-inherit-labels".to_owned()));
        assert!(args.contains(&"--title=$(touch /tmp/unwanted)".to_owned()));
        assert!(args.contains(&"--description=--priority=0\nsecond line".to_owned()));
        assert!(create_child_args(&create_input(), "--help", &[]).is_err());
        let inherited = vec!["team:core".to_owned()];
        let args = create_child_args(&create_input(), "ontrack-parent", &inherited).unwrap();
        assert!(args.contains(&"--labels=team:core".to_owned()));
        assert!(
            create_child_args(&create_input(), "ontrack-parent", &[READY_LABEL.into()]).is_err()
        );
        assert!(
            create_child_args(
                &create_input(),
                "ontrack-parent",
                &[UNASSIGNED_LABEL.into()]
            )
            .is_err()
        );
    }

    #[test]
    fn open_descendants_follow_beads_hierarchy_through_closed_intermediates() {
        let rows = json!([
            {"id":"parent","title":"Parent","status":"open"},
            {"id":"child","title":"Child","status":"closed","parent":"parent"},
            {"id":"grandchild","title":"Grandchild","status":"open","parent":"child"},
            {"id":"unrelated","title":"Elsewhere","status":"open"}
        ]);
        let beads = parse_list(&serde_json::to_vec(&rows).unwrap()).unwrap();
        assert_eq!(open_descendants_in(&beads, "parent"), vec!["grandchild"]);
    }

    fn create_input() -> CreateBead {
        CreateBead {
            title: "  A new issue  ".to_owned(),
            description: "First line\n\nSecond line\r\n  indented  \n".to_owned(),
            issue_type: "task".to_owned(),
            priority: 2,
            harness: None,
        }
    }

    #[test]
    fn ready_and_harness_labels_are_app_state_without_changing_beads_status() {
        let data = json!({"issue_type":"task", "labels":["team:core", READY_LABEL, "ontrack:agent:opencode"]});
        let bead = Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: "b1".into(),
            title: "Task".into(),
            status: "open".into(),
            priority: 2,
            data: data.clone(),
        };
        assert!(is_ready(&bead));
        assert_eq!(harness(&data), Some("opencode"));
        assert!(!is_ready(&Bead {
            status: "closed".into(),
            ..bead.clone()
        }));
        for issue_type in ["bug", "feature", "chore", "custom"] {
            let other = Bead {
                data: json!({"issue_type":issue_type, "labels":[READY_LABEL]}),
                ..bead.clone()
            };
            assert!(is_ready(&other));
            assert!(ready_record(&json!({
                "status":"open", "issue_type":issue_type, "labels":[READY_LABEL]
            })));
        }
        assert!(is_ready(&Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: "epic".into(),
            title: "Epic".into(),
            status: "open".into(),
            priority: 2,
            data: json!({"issue_type":"epic", "labels":[READY_LABEL]}),
        }));
        assert!(!ready_record(&json!({
            "status":"open", "issue_type":"epic", "labels":[READY_LABEL]
        })));
    }

    #[test]
    fn tree_ready_targets_include_nested_open_tasks_and_skip_existing_handoffs() {
        let rows = json!([
            {"id":"epic","title":"Epic","status":"open","issue_type":"epic"},
            {"id":"direct","title":"Direct","status":"open","issue_type":"task","parent":"epic"},
            {"id":"ready","title":"Ready","status":"open","issue_type":"task","parent":"epic","labels":[READY_LABEL]},
            {"id":"reserved","title":"Reserved","status":"open","issue_type":"task","parent":"epic","labels":[READY_LABEL]},
            {"id":"closed","title":"Closed","status":"closed","issue_type":"task","parent":"epic"},
            {"id":"group","title":"Group","status":"closed","issue_type":"feature","parent":"epic"},
            {"id":"nested","title":"Nested","status":"open","issue_type":"task","parent":"group"},
            {"id":"sent","title":"Sent","status":"open","issue_type":"task","parent":"epic","labels":[READY_LABEL]},
            {"id":"other","title":"Other","status":"open","issue_type":"task"}
        ]);
        let beads = parse_list(&serde_json::to_vec(&rows).unwrap()).unwrap();
        let handoffs = std::collections::HashSet::from(["sent".to_owned(), "reserved".to_owned()]);
        let accepted = std::collections::HashSet::from(["sent".to_owned()]);
        assert_eq!(
            tree_ready_targets_in(&beads, "epic", true, &handoffs, &accepted).unwrap(),
            ["epic", "direct", "nested"]
        );
        assert_eq!(
            tree_ready_targets_in(&beads, "epic", false, &handoffs, &accepted).unwrap(),
            ["epic", "ready", "reserved"]
        );
        assert_eq!(
            tree_ready_targets_in(&beads, "other", true, &handoffs, &accepted).unwrap(),
            ["other"]
        );
    }

    #[test]
    fn task_ready_cascades_across_types_and_preserves_handoffs() {
        let rows = serde_json::json!([
            {"id":"parent","title":"Parent","status":"open","issue_type":"task"},
            {"id":"child","title":"Child","status":"open","issue_type":"feature","parent":"parent"},
            {"id":"leaf","title":"Leaf","status":"open","issue_type":"bug","parent":"child"},
            {"id":"closed","title":"Closed","status":"closed","parent":"parent"},
            {"id":"under-closed","title":"Open","status":"open","parent":"closed"},
            {"id":"active","title":"Active","status":"in_progress","parent":"parent"},
            {"id":"deferred","title":"Deferred","status":"deferred","parent":"parent"},
            {"id":"sent","title":"Sent","status":"open","parent":"parent","labels":[READY_LABEL]},
            {"id":"pending","title":"Pending","status":"open","parent":"child","labels":[READY_LABEL]},
            {"id":"unrelated","title":"Unrelated","status":"open"}
        ]);
        let mut beads = parse_list(&serde_json::to_vec(&rows).unwrap()).unwrap();
        let known = std::collections::HashSet::from(["sent".into()]);
        let protected = std::collections::HashSet::from(["sent".into(), "pending".into()]);
        let marked = tree_ready_targets_in(&beads, "parent", true, &known, &protected).unwrap();
        assert_eq!(marked, ["parent", "child", "leaf", "under-closed"]);
        for bead in &mut beads {
            if marked.contains(&bead.id) {
                bead.data["labels"] = serde_json::json!([READY_LABEL]);
            }
        }
        assert_eq!(
            tree_ready_targets_in(&beads, "parent", false, &known, &protected).unwrap(),
            marked
        );
        for id in ["closed", "active", "deferred", "missing"] {
            assert!(tree_ready_targets_in(&beads, id, true, &known, &protected).is_err());
        }
    }

    #[test]
    fn large_epic_ready_cascade_includes_all_open_tasks() {
        let mut rows = vec![serde_json::json!({
            "id": "epic", "title": "Large Epic", "status": "open", "issue_type": "epic"
        })];
        rows.extend((0..100).map(|i| {
            serde_json::json!({
                "id": format!("task-{i}"), "title": "Task", "status": "open",
                "issue_type": "task", "parent": "epic"
            })
        }));
        let mut beads = parse_list(&serde_json::to_vec(&rows).unwrap()).unwrap();
        let empty = std::collections::HashSet::new();
        let marked = tree_ready_targets_in(&beads, "epic", true, &empty, &empty).unwrap();
        assert_eq!(marked.len(), 101);
        for bead in &mut beads {
            bead.data["labels"] = serde_json::json!([READY_LABEL]);
        }
        let cleared = tree_ready_targets_in(&beads, "epic", false, &empty, &empty).unwrap();
        assert_eq!(marked, cleared);
    }

    #[test]
    fn creation_and_edit_persist_harness_as_a_safe_label_argument() {
        let mut fields = create_input();
        fields.harness = Some("opencode".into());
        let args = create_args(&fields).unwrap();
        assert!(args.contains(&"--labels=ontrack:agent:opencode".into()));
        let update = UpdateBead {
            id: "b1".into(),
            fields,
            previous_harness: Some("codex".into()),
        };
        let args = update_args(&update).unwrap();
        assert!(args.contains(&"--remove-label=ontrack:agent:codex".into()));
        assert!(args.contains(&"--add-label=ontrack:agent:opencode".into()));
        assert!(args.contains(&"--remove-label=ontrack:unassigned".into()));
        assert_eq!(&args[args.len() - 2..], ["--", "b1"]);
    }

    #[test]
    fn unassigned_marker_distinguishes_new_choice_from_legacy_missing_label() {
        let legacy = json!({"labels": [READY_LABEL]});
        let unassigned = json!({"labels": [READY_LABEL, UNASSIGNED_LABEL]});
        assert!(!explicitly_unassigned(&legacy));
        assert!(explicitly_unassigned(&unassigned));
        assert!(harness(&legacy).is_none());
        assert!(harness(&unassigned).is_none());
        let args = create_args(&create_input()).unwrap();
        assert!(args.contains(&"--labels=ontrack:unassigned".into()));
        let mut selected = create_input();
        selected.harness = Some("codex".into());
        let args = update_args(&UpdateBead {
            id: "b1".into(),
            fields: selected,
            previous_harness: None,
        })
        .unwrap();
        assert!(args.contains(&"--remove-label=ontrack:unassigned".into()));
        assert!(args.contains(&"--add-label=ontrack:agent:codex".into()));
    }

    #[test]
    fn create_args_trim_title_and_preserve_multiline_description() {
        assert_eq!(
            create_args(&create_input()).unwrap(),
            [
                "--no-color",
                "create",
                "--title=A new issue",
                "--description=First line\n\nSecond line\r\n  indented  \n",
                "--type=task",
                "--priority=2",
                "--json",
                "--labels=ontrack:unassigned",
            ]
        );
    }

    #[test]
    fn planned_creation_preserves_acceptance_and_parent_as_arguments() {
        let item = crate::planning::Item {
            title: "Plan child".into(),
            description: "Do the work".into(),
            acceptance: "Check $(touch ignored)\nthen test".into(),
            priority: 1,
        };
        let args = planned_args(&item, "task", Some("epic-1")).unwrap();
        assert!(args.contains(&"--acceptance=Check $(touch ignored)\nthen test".into()));
        assert!(args.contains(&"--parent=epic-1".into()));
        assert!(args.contains(&"--labels=ontrack:unassigned".into()));
        assert!(planned_args(&item, "task", Some("-unsafe")).is_err());
    }

    #[test]
    fn create_keeps_flag_like_and_shell_text_in_single_arguments() {
        let mut input = create_input();
        input.title = "--force $(touch ignored); 'quoted'".to_owned();
        input.description = "--cascade\n\"; $(touch ignored)".to_owned();
        let args = create_args(&input).unwrap();
        assert_eq!(args.len(), 8);
        assert_eq!(args[2], format!("--title={}", input.title));
        assert_eq!(args[3], format!("--description={}", input.description));
        assert!(!args.iter().any(|arg| arg == "--readonly"));
    }

    #[test]
    fn update_targets_one_id_and_keeps_user_text_in_argument_values() {
        let mut fields = create_input();
        fields.title = "--force $(touch ignored); 'quoted'".into();
        fields.description = "First line\n--status=closed; $(touch ignored)".into();
        let input = UpdateBead {
            id: "ontrack-70t".into(),
            fields,
            previous_harness: None,
        };
        assert_eq!(
            update_args(&input).unwrap(),
            [
                "--no-color",
                "update",
                "--title=--force $(touch ignored); 'quoted'",
                "--description=First line\n--status=closed; $(touch ignored)",
                "--type=task",
                "--priority=2",
                "--json",
                "--add-label=ontrack:unassigned",
                "--",
                "ontrack-70t",
            ]
        );
    }

    #[test]
    fn update_rejects_invalid_id_and_title_before_running_bd() {
        let mut input = UpdateBead {
            id: "--force".into(),
            fields: create_input(),
            previous_harness: None,
        };
        assert_eq!(
            update_bead(&input).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        input.id = "ontrack-70t".into();
        input.fields.title = "first\nsecond".into();
        assert_eq!(
            update_bead(&input).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn create_accepts_all_core_types_and_priorities() {
        assert_eq!(
            BEAD_TYPES,
            [
                "task",
                "bug",
                "feature",
                "epic",
                "chore",
                "decision",
                "spike",
                "story",
                "milestone"
            ]
        );
        for issue_type in BEAD_TYPES {
            for priority in 0..=4 {
                let mut input = create_input();
                input.issue_type = (*issue_type).to_owned();
                input.priority = priority;
                input.description.clear();
                let args = create_args(&input).unwrap();
                assert_eq!(args[4], format!("--type={issue_type}"));
                assert_eq!(args[5], format!("--priority={priority}"));
                assert!(args.contains(&"--labels=ontrack:unassigned".into()));
            }
        }
    }

    #[test]
    fn create_rejects_invalid_inputs_before_running_a_process() {
        for title in [
            "",
            "  \t ",
            "first\nsecond",
            "first\rsecond",
            "title\n",
            "\n title",
            "a\0b",
            "a\u{85}b",
            "a\u{2028}b",
            "a\u{2029}b",
        ] {
            let mut input = create_input();
            input.title = title.to_owned();
            assert_eq!(
                create_bead(&input).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        for priority in [5, u8::MAX] {
            let mut input = create_input();
            input.priority = priority;
            assert_eq!(
                create_bead(&input).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        for issue_type in ["", "custom", "Task", "task ", "--force", "task\n--force"] {
            let mut input = create_input();
            input.issue_type = issue_type.to_owned();
            assert_eq!(
                create_bead(&input).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn create_parses_object_id_and_rejects_malformed_output() {
        assert_eq!(
            parse_created_id(br#"{"id":"ontrack-70t","title":"New issue","future":true}"#).unwrap(),
            "ontrack-70t"
        );
        for input in [
            "",
            "not json",
            "null",
            "[]",
            "{}",
            r#"{"id":null}"#,
            r#"{"id":3}"#,
            r#"{"id":""}"#,
            r#"{"id":"--force"}"#,
            r#"{"id":"a b"}"#,
            r#"{"id":"a\nb"}"#,
        ] {
            assert_eq!(
                parse_created_id(input.as_bytes()).unwrap_err().kind(),
                io::ErrorKind::InvalidData,
                "input: {input}"
            );
        }
    }

    #[test]
    fn verified_close_args_guard_and_target_only_one_issue_without_force() {
        let live =
            json!({"id":"ontrack-70t", "status":"in_progress", "labels":[READY_TO_MERGE_LABEL]});
        assert_eq!(
            close_verified_args("ontrack-70t", &live, &live).unwrap(),
            [
                "--no-color",
                "update",
                "--if-status=in_progress",
                "--status=closed",
                "--json",
                "--remove-label=ontrack:ready",
                "--remove-label=ontrack:ready-to-merge",
                "--",
                "ontrack-70t"
            ]
        );
    }

    #[test]
    fn delete_args_force_only_selected_issue_without_cascade() {
        assert_eq!(
            delete_args("ontrack-70t").unwrap(),
            [
                "--no-color",
                "delete",
                "--force",
                "--json",
                "--",
                "ontrack-70t"
            ]
        );
        for id in ["ontrack-70t", "ontrack-70t.1", "workspace:a-b", "a", "123"] {
            validate_id(id).unwrap();
        }
    }

    #[test]
    fn verified_close_delete_and_show_reject_unsafe_ids_before_running_a_process() {
        for id in [
            "", " ", "--force", "-a", "a b", "a\nb", "a\tb", "a\0b", "a\u{7f}b", "a\u{a0}b",
        ] {
            assert_eq!(
                close_verified_bead(id, &Value::Null).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "id: {id:?}"
            );
            assert_eq!(
                delete_bead(id).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "id: {id:?}"
            );
            assert_eq!(
                show_bead(id).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "id: {id:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn mutation_command_errors_and_create_success_use_injected_executor() {
        use std::os::unix::process::ExitStatusExt;

        let owned_args = create_args(&create_input()).unwrap();
        let create_args: Vec<&str> = owned_args.iter().map(String::as_str).collect();
        let delete_args = delete_args("ontrack-70t").unwrap();
        let live =
            json!({"id":"ontrack-70t", "status":"in_progress", "labels":[READY_TO_MERGE_LABEL]});
        let owned_close_args = close_verified_args("ontrack-70t", &live, &live).unwrap();
        let close_args: Vec<&str> = owned_close_args.iter().map(String::as_str).collect();
        for args in [
            create_args.as_slice(),
            delete_args.as_slice(),
            close_args.as_slice(),
        ] {
            let error = run_bd_with(args, |actual| {
                assert_eq!(actual, args);
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(1 << 8),
                    stdout: b"{}".to_vec(),
                    stderr: b"mutation rejected\n".to_vec(),
                })
            })
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert!(error.to_string().contains("exit status: 1"));
            assert!(error.to_string().contains("mutation rejected"));

            let error = run_bd_with(args, |_| {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "permission denied",
                ))
            })
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert!(error.to_string().contains("Could not run bd"));
        }
        let bytes = run_bd_with(&create_args, |_| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: br#"{"id":"ontrack-new"}"#.to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();
        assert_eq!(parse_created_id(&bytes).unwrap(), "ontrack-new");
    }

    #[test]
    fn list_args_are_read_only_uncapped_and_include_closed() {
        assert_eq!(
            LIST_ARGS,
            [
                "--readonly",
                "--no-color",
                "list",
                "--all",
                "--limit",
                "0",
                "--json"
            ]
        );
    }

    #[test]
    fn show_args_request_full_details_and_keep_id_in_one_argument() {
        assert_eq!(
            show_args("--id=ontrack-ow6"),
            [
                "--readonly",
                "--no-color",
                "show",
                "--id=ontrack-ow6",
                "--json",
                "--long",
                "--include-comments",
                "--include-dependents",
            ]
        );
    }

    #[test]
    fn list_preserves_full_records_and_defaults_priority() {
        let records = json!([
            {"id": "a", "title": "First", "status": "closed", "priority": 0,
             "future_metadata": {"revision": 3}},
            {"id": "b", "title": "Second", "status": "open"},
            {"id": "c", "title": "Third", "status": "in_progress", "priority": null}
        ]);
        let beads = parse_list(&serde_json::to_vec(&records).unwrap()).unwrap();
        assert_eq!(beads.len(), 3);
        assert_eq!(beads[0].id, "a");
        assert_eq!(beads[0].title, "First");
        assert_eq!(beads[0].status, "closed");
        assert_eq!(beads[0].priority, 0);
        assert_eq!(beads[0].data, records[0]);
        assert_eq!(beads[1].priority, 2);
        assert_eq!(beads[2].priority, 2);
        assert!(parse_list(b"[]").unwrap().is_empty());
    }

    #[test]
    fn list_rejects_malformed_json_and_records_without_panicking() {
        for bytes in [
            "",
            "not json",
            "{}",
            "null",
            "[null]",
            "[{}]",
            r#"[{"id":"","title":"x","status":"open"}]"#,
            r#"[{"id":"a","title":3,"status":"open"}]"#,
            r#"[{"id":"a","title":"x","status":""}]"#,
            r#"[{"id":"a","title":"x","status":"open","priority":-1}]"#,
            r#"[{"id":"a","title":"x","status":"open","priority":"high"}]"#,
        ] {
            assert_eq!(
                parse_list(bytes.as_bytes()).unwrap_err().kind(),
                io::ErrorKind::InvalidData,
                "input: {bytes}"
            );
        }
    }

    #[test]
    fn show_selects_requested_record_and_preserves_unknown_fields() {
        let records = json!([
            {"id": "other"},
            {"id": "wanted", "revision": 4, "dependency_count": 2,
             "metadata": {"future": true}, "comments": [{"text": "hello"}],
             "dependents": [{"id": "other"}]}
        ]);
        let result = parse_show(&serde_json::to_vec(&records).unwrap(), "wanted").unwrap();
        assert_eq!(result, records[1]);
    }

    #[test]
    fn show_rejects_missing_duplicate_or_malformed_records() {
        for input in ["[]", r#"[{"id":"other"}]"#] {
            assert_eq!(
                parse_show(input.as_bytes(), "wanted").unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
        }
        for input in [
            "",
            "{}",
            "[null]",
            "[{}]",
            r#"[{"id":3}]"#,
            r#"[{"id":"wanted"},{"id":"wanted"}]"#,
        ] {
            assert_eq!(
                parse_show(input.as_bytes(), "wanted").unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn show_rejects_empty_id_before_running_a_process() {
        for id in ["", " \n "] {
            assert_eq!(
                show_bead(id).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn missing_executable_keeps_not_found_kind_and_adds_context() {
        let error = run_bd_with(LIST_ARGS, |args| {
            assert_eq!(args, LIST_ARGS);
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "missing executable",
            ))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains("Could not run bd"));
        assert!(error.to_string().contains("missing executable"));
    }

    #[cfg(unix)]
    #[test]
    fn command_failure_includes_stderr_and_success_returns_stdout() {
        use std::os::unix::process::ExitStatusExt;

        let error = run_bd_with(LIST_ARGS, |_| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: b"[]".to_vec(),
                stderr: b"database unavailable\n".to_vec(),
            })
        })
        .unwrap_err();
        assert!(error.to_string().contains("exit status: 1"));
        assert!(error.to_string().contains("database unavailable"));

        let bytes = run_bd_with(LIST_ARGS, |_| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"[]".to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();
        assert_eq!(bytes, b"[]");
    }
}
