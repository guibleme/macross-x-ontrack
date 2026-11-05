//! Local JSON command adapter. No terminal, network listener, or second scheduler.
use crate::{beads, worktrees};
use serde_json::{Map, Value, json};
use std::io::{self, Read, Write};

const MAX_REQUEST: usize = 65536;
const METHODS: &[&str] = &[
    "capabilities",
    "board.list",
    "task.show",
    "task.create",
    "task.comment",
    "task.ready",
    "worktrees.list",
    "worktree.continue",
];

enum Call {
    Capabilities,
    Board,
    Show(String),
    Create(beads::CreateBead),
    Comment(String, String),
    Ready(String, bool),
    Worktrees,
    Continue(String),
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn keys(object: &Map<String, Value>, allowed: &[&str]) -> io::Result<()> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("Unknown request or parameter field"));
    }
    Ok(())
}

fn text(params: &Map<String, Value>, key: &str, max: usize) -> io::Result<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= max && !s.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| invalid("Missing, empty, or oversized text parameter"))
}

fn task_id(params: &Map<String, Value>) -> io::Result<String> {
    let id = text(params, "id", 256)?;
    beads::validate_id(&id)?;
    Ok(id)
}

fn decode(request: &Value) -> io::Result<(String, Call)> {
    let object = request
        .as_object()
        .ok_or_else(|| invalid("Request must be an object"))?;
    keys(object, &["version", "id", "method", "params"])?;
    if object.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(invalid("Unsupported protocol version"));
    }
    let id = text(object, "id", 128)?;
    let empty = Map::new();
    let params = match object.get("params") {
        None => &empty,
        Some(value) => value
            .as_object()
            .ok_or_else(|| invalid("params must be an object"))?,
    };
    let method = text(object, "method", 64)?;
    let call = match method.as_str() {
        "capabilities" => {
            keys(params, &[])?;
            Call::Capabilities
        }
        "board.list" => {
            keys(params, &[])?;
            Call::Board
        }
        "task.show" => {
            keys(params, &["id"])?;
            Call::Show(task_id(params)?)
        }
        "task.create" => {
            keys(
                params,
                &["title", "description", "type", "priority", "harness"],
            )?;
            let issue_type = params
                .get("type")
                .map_or(Some("task"), Value::as_str)
                .filter(|kind| beads::BEAD_TYPES.contains(kind))
                .ok_or_else(|| invalid("Unsupported task type"))?
                .to_owned();
            let priority = params
                .get("priority")
                .map_or(Some(2), Value::as_u64)
                .filter(|value| *value <= 4)
                .ok_or_else(|| invalid("Priority must be an integer from 0 to 4"))?
                as u8;
            let harness = match params.get("harness") {
                None => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .filter(|kind| beads::HARNESSES.contains(kind))
                        .ok_or_else(|| invalid("Unsupported harness"))?
                        .to_owned(),
                ),
            };
            Call::Create(beads::CreateBead {
                title: text(params, "title", 256)?,
                description: text(params, "description", 32768)?,
                issue_type,
                priority,
                harness,
            })
        }
        "task.comment" => {
            keys(params, &["id", "text"])?;
            Call::Comment(task_id(params)?, text(params, "text", 16384)?)
        }
        "task.ready" => {
            keys(params, &["id", "ready"])?;
            let ready = params
                .get("ready")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid("ready must be a boolean"))?;
            Call::Ready(task_id(params)?, ready)
        }
        "worktrees.list" => {
            keys(params, &[])?;
            Call::Worktrees
        }
        "worktree.continue" => {
            keys(params, &["id"])?;
            Call::Continue(text(params, "id", 256)?)
        }
        _ => return Err(invalid("Unknown method")),
    };
    Ok((id, call))
}

fn execute(call: Call, writes: bool) -> io::Result<Value> {
    match call {
        Call::Capabilities => Ok(json!({
            "transport":"local-json-command", "methods":METHODS,
            "writes_enabled":writes, "automatic_scheduler":"tui",
            "request_ids":"correlation-only", "network_listener":false,
        })),
        Call::Board => Ok(Value::Array(
            beads::list_beads()?
                .iter()
                .map(|bead| {
                    json!({
                        "column":beads::display_status(bead), "record":bead.data, "revision":crate::runtime::digest(&bead.data),
                    })
                })
                .collect(),
        )),
        Call::Show(id) => beads::show_bead(&id).map(|mut record| { let revision=crate::runtime::digest(&record);record["ontrackRevision"]=json!(revision);record }),
        Call::Create(input) => beads::create_bead(&input).map(|id| json!({"id":id})),
        Call::Comment(id, text) => beads::add_comment(&id, &text),
        Call::Ready(id, ready) => {
            beads::set_ready(&id, ready)?;
            Ok(json!({"id":id,"ready":ready}))
        }
        Call::Worktrees => Ok(Value::Array(
            worktrees::list()?
                .iter()
                .map(|item| {
                    json!({
                        "id":item.id, "active":worktrees::is_current_run(&item.id).unwrap_or(false), "bead_id":item.bead_id, "parent_bead_id":item.parent_bead_id,
                        "branch":item.branch, "target_branch":item.target_branch,
                        "workspace_id":item.workspace_id, "handoff_accepted":item.handoff_accepted,
                        "status_label":worktrees::status_label(item.status), "status":worktrees::status_code(item.status), "detail":item.detail,
                        "execution":worktrees::execution_code(item.status), "integration":worktrees::integration_state(&item.id,item.status),
                        "resultRevision":worktrees::result_revision(&item.id).ok(), "review":worktrees::review(&item.id).unwrap_or_else(|_| json!({"state":"pending"})),
                    })
                })
                .collect(),
        )),
        Call::Continue(id) => {
            worktrees::continue_task(&id).map(|message| json!({"message":message}))
        }
    }
}

fn error(id: Value, code: &str, message: String) -> Value {
    json!({"version":1,"id":id,"error":{"code":code,"message":message}})
}

pub(crate) fn respond_direct(input: &[u8], writes: bool) -> Value {
    if input.len() > MAX_REQUEST {
        return error(
            Value::Null,
            "invalid_request",
            "Request exceeds 65536 bytes".into(),
        );
    }
    let request: Value = match serde_json::from_slice(input) {
        Ok(value) => value,
        Err(e) => return error(Value::Null, "invalid_request", e.to_string()),
    };
    let (id, call) = match decode(&request) {
        Ok(parsed) => parsed,
        Err(e) => return error(Value::Null, "invalid_request", e.to_string()),
    };
    if !writes
        && !matches!(
            call,
            Call::Capabilities | Call::Board | Call::Show(_) | Call::Worktrees
        )
    {
        return error(
            json!(id),
            "writes_disabled",
            "Mutation requires --allow-writes".into(),
        );
    }
    match execute(call, writes) {
        Ok(result) => json!({"version":1,"id":id,"result":result}),
        Err(e) => error(json!(id), "backend_error", e.to_string()),
    }
}

/// Exactly one bounded request on stdin, one response on stdout, then exit.
pub fn run(writes: bool) -> io::Result<bool> {
    let mut input = Vec::new();
    io::stdin()
        .take((MAX_REQUEST + 1) as u64)
        .read_to_end(&mut input)?;
    let response = if input.len() > MAX_REQUEST {
        error(
            Value::Null,
            "invalid_request",
            "Request exceeds 65536 bytes".into(),
        )
    } else {
        match serde_json::from_slice::<Value>(&input) {
            Ok(request) => crate::runtime::client_call(request, writes),
            Err(e) => error(Value::Null, "invalid_request", e.to_string()),
        }
    };
    let success = response.get("error").is_none();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &response)?;
    writeln!(out)?;
    Ok(success)
}

pub(crate) fn validate(request: &Value) -> io::Result<()> {
    decode(request).map(|_| ())
}
pub(crate) fn original_method(method: &str) -> bool {
    METHODS.contains(&method)
}
