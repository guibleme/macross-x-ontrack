//! One local repository owner. All clients share the scheduler and durable command journal.
use crate::{app, interface};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static OWNER: AtomicBool = AtomicBool::new(false);
static DOMAIN: Mutex<()> = Mutex::new(());
static SCHEDULER_STATUS: Mutex<Value> = Mutex::new(Value::Null);
static NONCE: AtomicU64 = AtomicU64::new(0);
const MAX: usize = 1024 * 1024;
pub(crate) fn is_owner() -> bool {
    OWNER.load(Ordering::SeqCst)
}
pub(crate) fn domain_lock() -> MutexGuard<'static, ()> {
    DOMAIN.lock().unwrap_or_else(|e| e.into_inner())
}
fn err(s: impl ToString) -> io::Error {
    io::Error::other(s.to_string())
}
pub(crate) fn digest(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("JSON value"))
    )
}
pub(crate) fn unique() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    )
}
pub(crate) fn git(args: &[&str]) -> io::Result<String> {
    let out = Command::new("git").args(args).output()?;
    if !out.status.success() {
        return Err(err("Repository identity is unavailable"));
    }
    String::from_utf8(out.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(err)
}
struct Location {
    repo: String,
    dir: PathBuf,
    socket: PathBuf,
}
fn private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(err("Runtime directory must not be a symlink"));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}
fn location() -> io::Result<Location> {
    let common = fs::canonicalize(git(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?)?;
    let repo = common
        .to_str()
        .ok_or_else(|| err("Repository path is not UTF-8"))?
        .to_owned();
    let dir = common.join("ontrack-runtime");
    let socket_dir = std::env::var_os("ONTRACK_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/state/ontrack/runtime")
        });
    let hash = digest(&json!(repo));
    Ok(Location {
        repo,
        dir,
        socket: socket_dir.join(format!("{}.sock", &hash[..24])),
    })
}
pub(crate) fn storage() -> io::Result<PathBuf> {
    let p = location()?.dir;
    private_dir(&p)?;
    Ok(p)
}
pub(crate) fn atomic_json(path: &Path, value: &Value) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| err("Missing state parent"))?;
    private_dir(parent)?;
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(err("State file must not be a symlink"));
    }
    let temp = parent.join(format!(".{}.tmp", unique()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    File::open(parent)?.sync_all()
}
fn load(path: &Path) -> io::Result<Value> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(err("State file must not be a symlink"));
    }
    match fs::read(path) {
        Ok(v) if v.len() <= 16 * MAX => {
            let journal: Value = serde_json::from_slice(&v).map_err(err)?;
            crate::command_state::validate(&journal)?;
            Ok(journal)
        }
        Ok(_) => Err(err("State size limit")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(json!({"version":1,"operations":{}})),
        Err(e) => Err(e),
    }
}
fn ops_path() -> io::Result<PathBuf> {
    Ok(storage()?.join("operations.json"))
}
pub(crate) fn has_unknown_target(target: &str) -> bool {
    if !is_owner() {
        return false;
    }
    target_has_unresolved_operation(ops_path().and_then(|path| load(&path)), target)
}
fn target_has_unresolved_operation(journal: io::Result<Value>, target: &str) -> bool {
    let Ok(journal) = journal else {
        return true;
    };
    let Some(operations) = journal["operations"].as_object() else {
        return true;
    };
    operations
        .values()
        .any(|operation| operation["state"] == "unknown" && operation["target"] == target)
}
fn response(id: &Value, code: &str, message: impl ToString) -> Value {
    json!({"version":1,"id":id,"error":{"code":code,"message":message.to_string()}})
}
pub(crate) fn capabilities() -> Value {
    json!({"transport":"local-json-command","runtime_transport":"unix-socket","automatic_scheduler":"shared-runtime","request_ids":"durable-operations","protocol":1,"network_listener":false,"methods":["capabilities","runtime.status","board.list","task.show","task.create","task.comment","task.ready","worktrees.list","worktree.continue","worktree.integrate","operation.get","operation.reconcile","agents.list","session.start","session.send","session.observe","review.accept","review.changes_requested","delivery.close"]})
}
fn is_write(method: &str) -> bool {
    !matches!(
        method,
        "capabilities"
            | "runtime.status"
            | "board.list"
            | "task.show"
            | "worktrees.list"
            | "operation.get"
            | "agents.list"
            | "session.observe"
    )
}
fn known(method: &str) -> bool {
    capabilities()["methods"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == method)
}
pub(crate) fn client_call(request: Value, writes: bool) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request["method"].as_str().unwrap_or("");
    if request["version"] != 1
        || !request.is_object()
        || request
            .as_object()
            .unwrap()
            .keys()
            .any(|k| !["version", "id", "method", "params"].contains(&k.as_str()))
        || id.as_str().is_none_or(|v| v.is_empty() || v.len() > 128)
        || !known(method)
        || request.get("params").is_some_and(|p| !p.is_object())
    {
        return response(
            &id,
            "invalid_request",
            "Invalid request, version, method or params",
        );
    }
    if let Err(e) = validate_params(&request) {
        return response(&id, "invalid_request", e);
    }
    if method == "capabilities" {
        if request["params"].as_object().is_some_and(|p| !p.is_empty()) {
            return response(&id, "invalid_request", "Unknown capability parameter");
        }
        let mut c = capabilities();
        c["writes_enabled"] = json!(writes);
        return json!({"version":1,"id":id,"result":c});
    }
    if is_write(method) && !writes {
        return response(&id, "writes_disabled", "Mutation requires --allow-writes");
    }
    // Validate existing public calls before touching the runtime.
    if interface::original_method(method) {
        let mut checked = request.clone();
        if let Some(p) = checked.get_mut("params").and_then(Value::as_object_mut) {
            p.remove("expectedRevision");
            p.remove("externalRef");
        }
        if let Err(e) = interface::validate(&checked) {
            return response(&id, "invalid_request", e);
        }
    }
    match send(&request) {
        Ok(value) => value,
        Err(e) => response(
            &id,
            if e.kind() == io::ErrorKind::ConnectionRefused || e.kind() == io::ErrorKind::NotFound {
                "runtime_unavailable"
            } else if is_write(method) {
                "operation_unknown"
            } else {
                "runtime_unavailable"
            },
            e,
        ),
    }
}
fn send(request: &Value) -> io::Result<Value> {
    let loc = location()?;
    let mut stream = UnixStream::connect(&loc.socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(180)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let packet = json!({"protocol":1,"repository":loc.repo,"request":request});
    serde_json::to_writer(&mut stream, &packet)
        .map_err(|_| err("Runtime submit outcome is unknown"))?;
    stream
        .write_all(b"\n")
        .map_err(|_| err("Runtime submit outcome is unknown"))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|_| err("Runtime submit outcome is unknown"))?;
    let mut data = Vec::new();
    stream
        .take((MAX + 1) as u64)
        .read_to_end(&mut data)
        .map_err(|_| err("Runtime response unavailable after submission; reconcile operation"))?;
    if data.len() > MAX {
        return Err(err("Runtime response exceeds limit"));
    }
    let envelope: Value = serde_json::from_slice(&data)?;
    if envelope["protocol"] != 1
        || envelope["repository"] != loc.repo
        || !envelope["epoch"].is_string()
    {
        return Err(err("Runtime identity mismatch"));
    }
    Ok(envelope["response"].clone())
}
pub(crate) fn ensure_started() -> io::Result<()> {
    if send(&json!({"version":1,"id":"probe","method":"runtime.status","params":{}})).is_ok() {
        return Ok(());
    }
    let mut child = Command::new(std::env::current_exe()?)
        .arg("serve")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    for _ in 0..100 {
        thread::sleep(Duration::from_millis(50));
        if send(&json!({"version":1,"id":"probe","method":"runtime.status","params":{}})).is_ok() {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            break;
        }
    }
    Err(err(
        "Runtime could not start; run ontrack serve to inspect the error",
    ))
}
pub(crate) fn tui_request(request: app::Request, out: &std::sync::mpsc::Sender<app::Response>) {
    if matches!(request, app::Request::ReadScreenshot(_)) {
        app::execute_clipboard(request, out);
        return;
    }
    let packet = json!({"version":1,"id":unique(),"method":"_tui","params":{"request":&request}});
    let result = send(&packet).and_then(|response| {
        if response.get("error").is_some() {
            return Err(err(format!(
                "{}: {}",
                response["error"]["code"]
                    .as_str()
                    .unwrap_or("runtime_error"),
                response["error"]["message"]
                    .as_str()
                    .unwrap_or("Inspect the operation before retrying")
            )));
        }
        Ok(response)
    });
    match result.and_then(|v| {
        serde_json::from_value::<Vec<app::Response>>(v["result"].clone()).map_err(err)
    }) {
        Ok(responses) => {
            for item in responses {
                let _ = out.send(item);
            }
        }
        Err(e) => {
            let _ = out.send(app::failure_response(
                request,
                format!("Runtime request failed: {e}"),
            ));
        }
    }
}
fn execute_tui(params: &Value) -> io::Result<Value> {
    let request: app::Request = serde_json::from_value(params["request"].clone())?;
    {
        let _guard = domain_lock();
        match &request {
            app::Request::Close(id) | app::Request::Move(id, _)
                if matches!(&request, app::Request::Close(_))
                    || matches!(&request,app::Request::Move(_,t) if t=="closed") =>
            {
                crate::worktrees::require_review_for_bead(id)?;
            }
            app::Request::WorktreeAction(id, action) => {
                crate::worktrees::require_review(id)?;
                if matches!(
                    action,
                    crate::worktrees::Action::CloseWorkspace
                        | crate::worktrees::Action::RemoveCheckout
                ) {
                    crate::worktrees::require_cleanup_bead_closed(id)?;
                }
            }
            _ => {}
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    app::execute_request(request, &tx);
    drop(tx);
    let responses: Vec<_> = rx.into_iter().collect();
    serde_json::to_value(responses).map_err(err)
}
fn dispatch(request: &Value) -> io::Result<Value> {
    let method = request["method"].as_str().unwrap_or("");
    let p = &request["params"];
    let string = |k: &str| {
        p[k].as_str()
            .filter(|s| !s.is_empty() && s.len() <= 32768)
            .ok_or_else(|| err(format!("Missing {k}")))
    };
    match method {
        "runtime.status" => Ok(
            json!({"state":"running","repository":location()?.repo,"protocol":1,"scheduler":SCHEDULER_STATUS.lock().unwrap_or_else(|e|e.into_inner()).clone()}),
        ),
        "agents.list" => Ok(
            json!({"agents":crate::herdr::installed_harnesses()?.into_iter().map(|id|json!({"id":id,"label":id,"available":crate::sessions::executable_available(&id),"verification":"discovered-not-live-verified","capabilities":["session.start","session.send","session.observe","task.execute"]})).collect::<Vec<_>>()}),
        ),
        "operation.get" => {
            let v = load(&ops_path()?)?;
            let o = &v["operations"][string("id")?];
            if o.is_null() {
                return Err(err("operation_not_found"));
            }
            Ok(o.clone())
        }
        "operation.reconcile" => {
            let target = string("id")?;
            let path = ops_path()?;
            let mut v = load(&path)?;
            let op = &mut v["operations"][target];
            if op["state"] != "unknown" {
                return Err(err("Only unknown operations need reconciliation"));
            }
            if p["actor"] != "human"
                || p["evidence"].as_array().is_none_or(|rows| {
                    rows.is_empty()
                        || rows.iter().any(|r| {
                            r.as_str()
                                .is_none_or(|s| s.trim().is_empty() || s.len() > 4096)
                        })
                })
            {
                return Err(err(
                    "Reconciliation requires explicit human identity and evidence",
                ));
            }
            if p["failed"] == true {
                return Err(err(
                    "No-effect cannot be proved here. Keep the operation unknown and inspect the backend; do not resend",
                ));
            }
            let result = p
                .get("result")
                .ok_or_else(|| err("Confirmed result required"))?;
            if op["method"] == "task.create" {
                let bead_id = result["id"]
                    .as_str()
                    .ok_or_else(|| err("Created Bead id required"))?;
                let bead = crate::beads::show_bead(bead_id)?;
                if bead["title"] != op["request"]["params"]["title"]
                    || bead["description"] != op["request"]["params"]["description"]
                {
                    return Err(err("Bead readback does not match the requested payload"));
                }
            } else {
                return Err(err(
                    "This operation needs backend-specific reconciliation; no automatic resolution is supported",
                ));
            }
            crate::command_state::advance(op, crate::command_state::Event::ConfirmReadback)?;
            op["result"] = result.clone();
            op["response"] = json!({"version":1,"id":target,"result":result});
            op["resolution"] = json!({"actor":"human","evidence":p["evidence"]});
            let result = op.clone();
            atomic_json(&path, &v)?;
            Ok(result)
        }
        "session.start" | "session.send" | "session.observe" => {
            crate::sessions::dispatch(method, p)
        }
        "review.accept" | "review.changes_requested" => crate::worktrees::record_review(
            string("id")?,
            string("resultRevision")?,
            method == "review.accept",
            p.get("evidence")
                .cloned()
                .unwrap_or_else(|| json!([p["reason"].clone()])),
        ),
        "worktree.integrate" => {
            let id = string("id")?;
            (|| {
                if crate::worktrees::result_revision(id)? != string("resultRevision")? {
                    return Err(err("Result revision changed"));
                }
                crate::worktrees::require_review(id)
            })()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
            // This enters the existing manual integration queue. It does not
            // merge directly, accept a result, close a Bead, or clean a checkout.
            let result =
                crate::worktrees::act(id, crate::worktrees::Action::RequestMerge).map_err(err)?;
            serde_json::to_value(result).map_err(err)
        }
        "delivery.close" => {
            let id = string("id")?;
            crate::worktrees::require_review_for_bead(id)?;
            crate::worktrees::close_integrated_bead(id)?;
            Ok(json!({"id":id,"closed":true}))
        }
        "_tui" => execute_tui(p),
        _ => {
            let mut clean = request.clone();
            if let Some(p) = clean.get_mut("params").and_then(Value::as_object_mut) {
                p.remove("expectedRevision");
                p.remove("externalRef");
            }
            let result = interface::respond_direct(&serde_json::to_vec(&clean)?, true);
            if result.get("error").is_some() {
                return Err(err(result["error"]["message"]
                    .as_str()
                    .unwrap_or("Backend error")));
            }
            Ok(result["result"].clone())
        }
    }
}
fn tui_variant(request: &Value) -> &str {
    let value = &request["params"]["request"];
    value
        .as_str()
        .or_else(|| {
            value
                .as_object()
                .and_then(|o| o.keys().next())
                .map(String::as_str)
        })
        .unwrap_or("")
}
fn tui_write(request: &Value) -> bool {
    !matches!(
        tui_variant(request),
        "List"
            | "Show"
            | "LoadEdit"
            | "Discover"
            | "DiscoverHarnesses"
            | "ListWorktrees"
            | "ObserveAgents"
            | "ReadScreenshot"
            | "PlanRead"
            | "ReconcileWorktrees"
            | "FocusWorkspace"
    )
}
fn mutation_target(request: &Value) -> String {
    if request["method"] == "_tui" {
        let value = &request["params"]["request"][tui_variant(request)];
        return value
            .as_str()
            .or_else(|| {
                value
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .to_owned();
    }
    request["params"]["id"]
        .as_str()
        .or_else(|| request["params"]["sessionId"].as_str())
        .or_else(|| request["params"]["externalRef"].as_str())
        .unwrap_or("")
        .to_owned()
}

fn handle(request: Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request["method"].as_str().unwrap_or("");
    if !known(method) && method != "_tui" {
        return response(&id, "invalid_request", "Unknown method");
    }
    if request["version"] != 1
        || id.as_str().is_none_or(|s| s.is_empty() || s.len() > 128)
        || !request.is_object()
        || request
            .as_object()
            .unwrap()
            .keys()
            .any(|k| !["version", "id", "method", "params"].contains(&k.as_str()))
        || request.get("params").is_some_and(|v| !v.is_object())
    {
        return response(&id, "invalid_request", "Invalid runtime request");
    }
    if interface::original_method(method) {
        let mut checked = request.clone();
        if let Some(p) = checked.get_mut("params").and_then(Value::as_object_mut) {
            p.remove("expectedRevision");
            p.remove("externalRef");
        }
        if let Err(e) = interface::validate(&checked) {
            return response(&id, "invalid_request", e);
        }
    }

    if let Err(e) = validate_params(&request) {
        return response(&id, "invalid_request", e);
    }
    // TUI and public API execute under the same domain owner. TUI work acquires
    // the domain mutex in execute_request; public calls acquire it here.
    let _guard = if method != "_tui" {
        Some(domain_lock())
    } else {
        None
    };
    let writes = if method == "_tui" {
        tui_write(&request)
    } else {
        is_write(method)
    };
    let path = match ops_path() {
        Ok(p) => p,
        Err(e) => return response(&id, "state_unavailable", e),
    };
    let mut journal = match load(&path) {
        Ok(v) => v,
        Err(e) => return response(&id, "state_unavailable", e),
    };
    if journal["version"] != 1 {
        return response(&id, "incompatible_state", "Unsupported operation journal");
    }
    let key = id.as_str().unwrap_or("");
    let fingerprint = digest(&json!({"method":method,"params":request["params"]}));
    if writes {
        if key.is_empty() || key.len() > 128 {
            return response(&id, "invalid_request", "Operation id required");
        }
        if let Some(old) = journal["operations"].get(key) {
            if old["digest"] != fingerprint {
                return response(
                    &id,
                    "request_id_conflict",
                    "Operation ID has another payload",
                );
            }
            if old["state"] == "unknown" {
                return response(
                    &id,
                    "operation_unknown",
                    "Reconcile the existing operation before retrying",
                );
            }
            return old.get("response").cloned().unwrap_or_else(|| {
                response(
                    &id,
                    "operation_unknown",
                    "Reconcile the existing operation before retrying",
                )
            });
        }
        let target = mutation_target(&request);
        if !target.is_empty()
            && journal["operations"].as_object().is_some_and(|ops| {
                ops.values()
                    .any(|o| o["target"] == target && o["state"] == "unknown")
            })
            && method != "operation.reconcile"
        {
            return response(
                &id,
                "operation_unknown",
                "Target has an unresolved operation",
            );
        }
        if let Some(expected) = request["params"].get("expectedRevision")
            && let Some(target_id) = request["params"]["id"].as_str()
        {
            let current = if method.starts_with("task.") {
                crate::beads::show_bead(target_id).map(|v| digest(&v))
            } else {
                crate::worktrees::result_revision(target_id)
            };
            if current
                .ok()
                .as_ref()
                .is_none_or(|v| expected.as_str() != Some(v))
            {
                return response(&id, "stale_revision", "Target revision changed");
            }
        }
        journal["operations"][key] = json!({"id":key,"digest":fingerprint,"method":method,"target":target,"state":"recorded","request":request});
        if let Err(e) = crate::command_state::advance(
            &mut journal["operations"][key],
            crate::command_state::Event::Dispatch,
        ) {
            return response(&id, "invalid_transition", e);
        }
        if let Err(e) = atomic_json(&path, &journal) {
            return response(&id, "state_unavailable", e);
        }
    }
    let result = dispatch(&request);
    // The TUI must still receive its exact typed error to clear its pending UI,
    // while the durable ledger retains an uncertain mutation outcome.
    let uncertain_tui =
        method == "_tui" && writes && result.as_ref().is_ok_and(tui_outcome_uncertain);
    let value = match &result {
        Ok(v) => json!({"version":1,"id":id,"result":v}),
        Err(e) => response(
            &id,
            if e.to_string() == "operation_not_found" {
                "operation_not_found"
            } else if matches!(
                method,
                "review.accept"
                    | "review.changes_requested"
                    | "session.start"
                    | "_tui"
                    | "worktree.integrate"
            ) && e.kind() == io::ErrorKind::InvalidInput
            {
                "precondition_failed"
            } else if writes {
                "operation_unknown"
            } else {
                "backend_error"
            },
            e,
        ),
    };
    if writes {
        // A backend error may follow an accepted external mutation. Keep it
        // unknown rather than interpreting an error or lost response as no-op.
        let fresh = load(&path);
        if let Ok(mut latest) = fresh {
            let event = if result.is_ok() && !uncertain_tui {
                crate::command_state::Event::Confirm
            } else if matches!(
                method,
                "review.accept"
                    | "review.changes_requested"
                    | "session.start"
                    | "_tui"
                    | "worktree.integrate"
            ) && result
                .as_ref()
                .is_err_and(|e| e.kind() == io::ErrorKind::InvalidInput)
            {
                crate::command_state::Event::RejectNoEffect
            } else {
                crate::command_state::Event::LoseOutcome
            };
            if let Err(e) = crate::command_state::advance(&mut latest["operations"][key], event) {
                return response(&id, "operation_unknown", e);
            }
            latest["operations"][key]["response"] = value.clone();
            if let Ok(v) = result {
                latest["operations"][key]["result"] = v;
            }
            if let Err(e) = atomic_json(&path, &latest) {
                return response(&id, "operation_unknown", e);
            }
        } else {
            return response(&id, "operation_unknown", "Cannot persist operation outcome");
        }
    }
    value
}
pub(crate) fn serve() -> io::Result<()> {
    let loc = location().map_err(|e| err(format!("Runtime repository identity failed: {e}")))?;
    private_dir(&loc.dir).map_err(|e| err(format!("Runtime state directory failed: {e}")))?;
    private_dir(loc.socket.parent().unwrap())
        .map_err(|e| err(format!("Runtime socket directory failed: {e}")))?;
    let lock_path = loc.dir.join("runtime.lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(err("Runtime lock is a symlink"));
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| err("Runtime already running for this repository"))?;
    if loc.socket.exists() {
        fs::remove_file(&loc.socket)?;
    }
    let listener = UnixListener::bind(&loc.socket)
        .map_err(|e| err(format!("Runtime socket bind failed: {e}")))?;
    fs::set_permissions(&loc.socket, fs::Permissions::from_mode(0o600))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| err(format!("Runtime listener configuration failed: {e}")))?;
    OWNER.store(true, Ordering::SeqCst);
    let path = ops_path()?;
    let mut journal = load(&path)?;
    if journal["version"] != 1 {
        return Err(err("Unsupported runtime journal"));
    }
    if let Some(ops) = journal["operations"].as_object_mut() {
        for op in ops.values_mut() {
            if op["state"] == "dispatched" || op["state"] == "recorded" {
                crate::command_state::advance(op, crate::command_state::Event::LoseOutcome)?;
            }
        }
    }
    atomic_json(&path, &journal)?;
    let epoch = unique();
    let mut scheduler = app::RuntimeScheduler::new();
    let mut next_diagnostics = Instant::now();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .is_err()
                {
                    // A disconnected client can reject socket options on macOS.
                    // Its failure must not terminate the repository owner.
                    continue;
                }
                let mut bytes = Vec::new();
                let read = BufReader::new(&stream)
                    .take((MAX + 1) as u64)
                    .read_until(b'\n', &mut bytes);
                let result = read.and_then(|_| {
                    if bytes.len() > MAX {
                        return Err(err("Runtime request exceeds limit"));
                    }
                    serde_json::from_slice::<Value>(&bytes).map_err(err)
                });
                let value = match result {
                    Ok(packet) if packet["protocol"] == 1 && packet["repository"] == loc.repo => {
                        handle(packet["request"].clone())
                    }
                    _ => response(&Value::Null, "invalid_request", "Invalid runtime envelope"),
                };
                let envelope =
                    json!({"protocol":1,"repository":loc.repo,"epoch":epoch,"response":value});
                let _ = serde_json::to_writer(&mut stream, &envelope);
                let _ = stream.flush();
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(30))
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::InvalidInput
                ) =>
            {
                thread::sleep(Duration::from_millis(30));
            }
            Err(e) => return Err(err(format!("Runtime listener accept failed: {e}"))),
        }
        scheduler.tick();
        if Instant::now() >= next_diagnostics {
            *SCHEDULER_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = scheduler.diagnostics();
            next_diagnostics = Instant::now() + Duration::from_secs(1);
        }
    }
}
pub(crate) mod wire_result {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<T: Serialize, S: Serializer>(
        value: &std::io::Result<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .as_ref()
            .map_err(ToString::to_string)
            .serialize(serializer)
    }
    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<std::io::Result<T>, D::Error> {
        let value: Result<T, String> = Deserialize::deserialize(deserializer)?;
        Ok(value.map_err(std::io::Error::other))
    }
}

pub(crate) fn current_result_revision(id: &str) -> io::Result<String> {
    let response = send(&json!({"version":1,"id":unique(),"method":"worktrees.list","params":{}}))?;
    response["result"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == id))
        .and_then(|row| row["resultRevision"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| err("Current result revision is unavailable"))
}

fn validate_params(request: &Value) -> io::Result<()> {
    let method = request["method"].as_str().unwrap_or("");
    let empty = serde_json::Map::new();
    let params = request
        .get("params")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let allowed: &[&str] = match method {
        "capabilities" | "runtime.status" | "board.list" | "worktrees.list" | "agents.list" => &[],
        "task.show" | "operation.get" | "delivery.close" => &["id", "expectedRevision"],
        "task.create" => &[
            "title",
            "description",
            "type",
            "priority",
            "harness",
            "externalRef",
            "expectedRevision",
        ],
        "task.comment" => &["id", "text", "expectedRevision"],
        "task.ready" => &["id", "ready", "expectedRevision"],
        "worktree.continue" => &["id", "expectedRevision"],
        "worktree.integrate" => &["id", "resultRevision"],
        "session.start" => &["agentId", "frontId", "context", "input", "workspaceId"],
        "session.send" => &["sessionId", "requestId", "context", "input"],
        "session.observe" => &["sessionId", "cursor"],
        "review.accept" => &["id", "resultRevision", "evidence"],
        "review.changes_requested" => &["id", "resultRevision", "reason", "evidence"],
        "operation.reconcile" => &["id", "actor", "evidence", "result", "failed"],
        "_tui" => &["request"],
        _ => return Err(err("Unknown method")),
    };
    if params.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(err("Unknown parameter field"));
    }
    for key in [
        "id",
        "sessionId",
        "requestId",
        "agentId",
        "frontId",
        "workspaceId",
        "externalRef",
    ] {
        if let Some(v) = params.get(key)
            && v.as_str().is_none_or(|s| {
                s.is_empty()
                    || s.len() > 256
                    || s.starts_with('-')
                    || s.chars().any(|c| c.is_control())
            })
        {
            return Err(err(format!("Invalid {key}")));
        }
    }
    if let Some(v) = params.get("expectedRevision")
        && v.as_str().is_none_or(|s| {
            ![40, 64].contains(&s.len()) || !s.bytes().all(|b| b.is_ascii_hexdigit())
        })
    {
        return Err(err("Invalid expected revision"));
    }
    if method.starts_with("session.") {
        for key in if method == "session.start" {
            vec!["agentId", "frontId", "input"]
        } else if method == "session.send" {
            vec!["sessionId", "input"]
        } else {
            vec!["sessionId"]
        } {
            if params
                .get(key)
                .and_then(Value::as_str)
                .is_none_or(|s| s.trim().is_empty() || s.len() > 32768 || s.contains('\0'))
            {
                return Err(err(format!("Invalid {key}")));
            }
        }
        if let Some(ctx) = params.get("context")
            && serde_json::to_vec(ctx)?.len() > 65536
        {
            return Err(err("Context exceeds limit"));
        }
    }
    Ok(())
}

fn tui_outcome_uncertain(value: &Value) -> bool {
    serde_json::from_value::<Vec<app::Response>>(value.clone())
        .map_or(true, |responses| responses.iter().any(app::response_failed))
}

#[cfg(test)]
mod uncertainty_tests {
    use super::*;
    #[test]
    fn failed_journal_reads_block_scheduler_instead_of_claiming_no_unknown_operations() {
        assert!(target_has_unresolved_operation(
            Err(io::Error::other("corrupt journal")),
            "task-1"
        ));
        assert!(target_has_unresolved_operation(
            Ok(json!({"version":1})),
            "task-1"
        ));
        assert!(!target_has_unresolved_operation(
            Ok(json!({"version":1,"operations":{}})),
            "task-1"
        ));
        assert!(target_has_unresolved_operation(
            Ok(json!({"version":1,"operations":{"x":{"state":"unknown","target":"task-1"}}})),
            "task-1"
        ));
    }
    #[test]
    fn typed_tui_backend_failure_is_uncertain_and_keeps_its_wire_error() {
        let responses = vec![app::Response::Created(Err(io::Error::other(
            "response lost",
        )))];
        let value = serde_json::to_value(&responses).unwrap();
        assert!(tui_outcome_uncertain(&value));
        assert!(value[0]["Created"]["Err"].is_string());
        let successful =
            serde_json::to_value(vec![app::Response::Created(Ok("fixture-1".into()))]).unwrap();
        assert!(!tui_outcome_uncertain(&successful));
    }
}
