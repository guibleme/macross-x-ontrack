//! Process-level owner and crash-recovery proofs. All commands use disposable repositories and fixtures.
#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};
struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    owner: Option<Child>,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        // macOS TMPDIR can exceed the Unix socket path limit before the
        // fixture name is appended. Keep this disposable fixture path short.
        let temporary = fs::canonicalize(std::env::temp_dir()).unwrap();
        let temporary = if temporary.as_os_str().len() > 32 {
            fs::canonicalize("/tmp").unwrap()
        } else {
            temporary
        };
        let root = loop {
            let candidate = temporary.join(format!(
                "otrt-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("fixture: {e}"),
            }
        };
        fs::create_dir(root.join("bin")).unwrap();
        let repo = root.join("project");
        fs::create_dir(&repo).unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "--initial-branch=main"]);
        run(&["config", "user.name", "Runtime Fixture"]);
        run(&["config", "user.email", "fixture@example.invalid"]);
        run(&["config", "commit.gpgsign", "false"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        fs::write(repo.join("base"), "base").unwrap();
        run(&["add", "base"]);
        run(&["commit", "-m", "fixture"]);
        let script = r##"#!/usr/bin/env python3
import json,sys,os,time
from pathlib import Path
root=Path(os.environ['RUNTIME_FIXTURE']);args=sys.argv[1:];p=root/'beads.json'
rows=json.loads(p.read_text()) if p.exists() else []
def val(flag,default=''):
 for a in args:
  if a.startswith(flag+'='):return a.split('=',1)[1]
 try:return args[args.index(flag)+1]
 except ValueError:return default
if 'create' in args:
 row={'id':'fx-'+str(len(rows)+1),'title':val('--title'),'description':val('--description'),'status':'open','priority':2,'labels':['ontrack:unassigned']};rows.append(row);p.write_text(json.dumps(rows));
 with (root/'create-count').open('a') as f:f.write('create\n')
 if (root/'delay-create').exists():time.sleep(20)
 print('malformed' if (root/'malformed-create').exists() else json.dumps(row))
elif 'list' in args or 'ready' in args or 'blocked' in args:print(json.dumps(rows if 'list' in args else []))
elif 'show' in args:
 id=next((a.split('=',1)[1] for a in args if a.startswith('--id=')),'');print(json.dumps([r for r in rows if r['id']==id]))
elif 'comments' in args:
 with (root/'comment-count').open('a') as f:f.write('comment\n')
 print(json.dumps({'id':'comment-1'}))
else:print('{}')
"##;
        fs::write(root.join("bin/bd"), script).unwrap();
        fs::set_permissions(root.join("bin/bd"), fs::Permissions::from_mode(0o700)).unwrap();
        let herdr = r##"#!/usr/bin/env python3
import json,sys,os
from pathlib import Path
args=sys.argv[1:];root=Path(os.environ['RUNTIME_FIXTURE'])
if args[:2]==['agent','get']:
 p=root/'native-agent.json'
 if not p.exists():sys.exit(1)
 print(p.read_text())
elif args[:2]==['agent','start']:print('  --kind <KIND>  [possible values: opencode, codex, copilot]')
elif args[:2]==['integration','status']:print('opencode: current (v1) (/fixture)')
else:print(json.dumps({'result':{'type':'workspace_list','workspaces':[]}}))
"##;
        fs::write(root.join("bin/herdr"), herdr).unwrap();
        fs::set_permissions(root.join("bin/herdr"), fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root,
            repo,
            owner: None,
        }
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ontrack"));
        c.current_dir(&self.repo)
            .env("ONTRACK_RUNTIME_DIR", self.root.join("sockets"))
            .env("RUNTIME_FIXTURE", &self.root)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        c
    }
    fn start(&mut self) {
        let child = self
            .command()
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.owner = Some(child);
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(30));
            if self.call("status", "runtime.status", json!({}))["result"]["state"] == "running" {
                return;
            }
        }
        panic!("runtime did not start");
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.owner.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    fn call(&self, id: &str, method: &str, params: Value) -> Value {
        let mut child = self
            .command()
            .args(["api", "--allow-writes"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let req = json!({"version":1,"id":id,"method":method,"params":params});
        serde_json::to_writer(child.stdin.as_mut().unwrap(), &req).unwrap();
        drop(child.stdin.take());
        let out = child.wait_with_output().unwrap();
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("response: {e}: {}", String::from_utf8_lossy(&out.stdout)))
    }
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.repo)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    fn create(&self, id: &str) -> Value {
        self.call(id,"task.create",json!({"title":"One wave","description":"One bounded outcome","harness":"opencode","externalRef":"wave-1"}))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn one_owner_serializes_clients_and_replays_operation_without_duplicate_create() {
    let mut f = Fixture::new();
    assert_eq!(
        f.call("off", "board.list", json!({}))["error"]["code"],
        "runtime_unavailable"
    );
    f.start();
    let mut second = f
        .command()
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut duplicate_exit = None;
    for _ in 0..100 {
        if let Some(status) = second.try_wait().unwrap() {
            duplicate_exit = Some(status);
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    if duplicate_exit.is_none() {
        let _ = second.kill();
        let _ = second.wait();
    }
    assert!(
        duplicate_exit.is_some_and(|status| !status.success()),
        "Duplicate owner must lose the lifetime lock"
    );

    let first = f.create("wave-create");
    assert_eq!(first["result"]["id"], "fx-1", "{first}");
    let again = f.create("wave-create");
    assert_eq!(first, again);
    assert_eq!(
        fs::read_to_string(f.root.join("create-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        f.call(
            "wave-create",
            "task.create",
            json!({"title":"Other","description":"Other"})
        )["error"]["code"],
        "request_id_conflict"
    );
    assert_eq!(
        f.call("missing", "operation.get", json!({"id":"absent"}))["error"]["code"],
        "operation_not_found"
    );
    let a = f.call("a", "board.list", json!({}));
    let b = f.call("b", "board.list", json!({}));
    assert_eq!(a["result"], b["result"]);
    assert_eq!(a["result"][0]["record"]["id"], "fx-1");
    f.stop();
    f.start();
    assert_eq!(f.create("wave-create")["result"]["id"], "fx-1");
    assert_eq!(
        fs::read_to_string(f.root.join("create-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}
#[test]
fn restart_preserves_unknown_intent_and_never_resends() {
    let mut f = Fixture::new();
    let dir = f.repo.join(".git/ontrack-runtime");
    fs::create_dir_all(&dir).unwrap();
    let request = json!({"version":1,"id":"uncertain","method":"task.create","params":{"title":"One wave","description":"One bounded outcome","harness":"opencode","externalRef":"wave-1"}});
    use sha2::{Digest, Sha256};
    let digest = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&json!({"method":request["method"],"params":request["params"]}))
                .unwrap()
        )
    );
    fs::write(dir.join("operations.json"),serde_json::to_vec(&json!({"version":1,"operations":{"uncertain":{"id":"uncertain","digest":digest,"state":"dispatched","method":"task.create","target":"wave-1","request":request}}})).unwrap()).unwrap();
    f.start();
    assert_eq!(
        f.call("inspect", "operation.get", json!({"id":"uncertain"}))["result"]["state"],
        "unknown"
    );
    assert_eq!(f.create("uncertain")["error"]["code"], "operation_unknown");
    assert!(!f.root.join("create-count").exists());
    assert!(
        f.call(
            "resolve",
            "operation.reconcile",
            json!({"id":"uncertain","failed":true,"actor":"human","evidence":["Checked"]})
        )
        .get("error")
        .is_some()
    );
    assert_eq!(
        f.call("inspect2", "operation.get", json!({"id":"uncertain"}))["result"]["state"],
        "unknown"
    );
}
fn managed_result(f: &Fixture) -> (PathBuf, String) {
    let base = f.git(&["rev-parse", "HEAD"]);
    let source = f.root.join("project.worktrees/result");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    f.git(&["worktree", "add", "-b", "result", source.to_str().unwrap()]);
    fs::write(source.join("work"), "implemented").unwrap();
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&source)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    run(&["add", "work"]);
    run(&["commit", "-m", "result"]);
    let result = run(&["rev-parse", "HEAD"]);
    let common = f.repo.join(".git").canonicalize().unwrap();
    let repo = f.repo.canonicalize().unwrap();
    let dir = common.join("ontrack-worktrees");
    fs::create_dir_all(&dir).unwrap();
    let session =
        json!({"agent":"opencode","source":"herdr:opencode","kind":"id","value":"fixture-session"});
    let entry = json!({"id":"run-1","bead_id":"fx-1","branch":"result","target_branch":"main","common_dir":common,"main_root":repo,"path":source,"phase":"active","base":base,"agent":"opencode","source_workspace":"w1","workspace_id":"wTask","pane_id":"wTask:p1","initial_lifecycle_seq":1,"initial_terminal_id":"terminal-1","agent_session":session});
    fs::write(
        dir.join("registry.json"),
        serde_json::to_vec(&json!({"version":1,"common_dir":common,"entries":[entry]})).unwrap(),
    )
    .unwrap();
    fs::write(f.root.join("beads.json"),serde_json::to_vec(&json!([{"id":"fx-1","title":"One wave","description":"One bounded outcome","priority":2,"status":"in_progress","labels":["ontrack:ready-to-merge"]}])).unwrap()).unwrap();
    fs::write(f.root.join("native-agent.json"),serde_json::to_vec(&json!({"result":{"type":"agent_info","agent":{"agent":"opencode","agent_status":"done","pane_id":"wTask:p1","workspace_id":"wTask","cwd":source,"foreground_cwd":source,"agent_session":session,"terminal_id":"terminal-1","state_change_seq":2}}})).unwrap()).unwrap();
    (source, result)
}
#[test]
fn review_is_pinned_to_exact_result_and_new_commit_invalidates_acceptance() {
    let mut f = Fixture::new();
    let (source, result) = managed_result(&f);
    f.start();
    let accepted = f.call(
        "accept",
        "review.accept",
        json!({"id":"run-1","resultRevision":result,"evidence":["Required check passed"]}),
    );
    assert_eq!(accepted["result"]["state"], "accepted", "{accepted}");
    fs::write(source.join("next"), "changed").unwrap();
    for args in [vec!["add", "next"], vec!["commit", "-m", "new result"]] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&source)
                .status()
                .unwrap()
                .success()
        );
    }
    let current = f.call("worktrees", "worktrees.list", json!({}));
    assert_eq!(
        current["result"][0]["review"]["state"], "invalidated",
        "{current}"
    );
    assert!(
        f.call("close", "delivery.close", json!({"id":"fx-1"}))
            .get("error")
            .is_some()
    );
}
#[test]
fn dirty_result_invalidates_acceptance_without_new_commit_and_does_not_silently_restore() {
    let mut f = Fixture::new();
    let (source, result) = managed_result(&f);
    f.start();
    let accepted = f.call(
        "accept",
        "review.accept",
        json!({"id":"run-1","resultRevision":result,"evidence":["Required checks passed"]}),
    );
    assert_eq!(accepted["result"]["state"], "accepted", "{accepted}");
    fs::write(source.join("work"), "uncommitted edit").unwrap();
    let rows = f.call("dirty", "worktrees.list", json!({}));
    assert_eq!(rows["result"][0]["resultRevision"], result);
    assert_eq!(
        rows["result"][0]["review"]["state"], "invalidated",
        "{rows}"
    );
    assert!(
        Command::new("git")
            .args(["checkout", "--", "work"])
            .current_dir(&source)
            .status()
            .unwrap()
            .success()
    );
    let rows = f.call("clean-again", "worktrees.list", json!({}));
    assert_eq!(rows["result"][0]["review"]["state"], "invalidated");
}
#[test]
fn unavailable_native_observation_preserves_historical_receipt_but_blocks_current_acceptance() {
    let mut f = Fixture::new();
    let (_, result) = managed_result(&f);
    f.start();
    let accepted = f.call(
        "accept",
        "review.accept",
        json!({"id":"run-1","resultRevision":result,"evidence":["Checks passed"]}),
    );
    assert_eq!(accepted["result"]["state"], "accepted", "{accepted}");
    fs::remove_file(f.root.join("native-agent.json")).unwrap();
    let rows = f.call("unavailable", "worktrees.list", json!({}));
    assert_eq!(rows["result"][0]["review"]["state"], "accepted", "{rows}");
    assert_eq!(rows["result"][0]["review"]["validity"], "unknown", "{rows}");
    assert!(
        f.call("close", "delivery.close", json!({"id":"fx-1"}))
            .get("error")
            .is_some()
    );
}
#[test]
fn dirty_or_working_result_cannot_be_accepted() {
    let mut f = Fixture::new();
    let (source, result) = managed_result(&f);
    f.start();
    fs::write(source.join("work"), "dirty").unwrap();
    assert!(
        f.call(
            "dirty-accept",
            "review.accept",
            json!({"id":"run-1","resultRevision":result,"evidence":["Not sufficient"]})
        )
        .get("error")
        .is_some()
    );
    assert!(
        Command::new("git")
            .args(["checkout", "--", "work"])
            .current_dir(&source)
            .status()
            .unwrap()
            .success()
    );
    let p = f.root.join("native-agent.json");
    let mut agent: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    agent["result"]["agent"]["agent_status"] = json!("working");
    fs::write(p, serde_json::to_vec(&agent).unwrap()).unwrap();
    assert!(
        f.call(
            "active-accept",
            "review.accept",
            json!({"id":"run-1","resultRevision":result,"evidence":["Not sufficient"]})
        )
        .get("error")
        .is_some()
    );
}

#[test]
fn confirmed_safe_cleanup_preserves_exact_accepted_result_but_external_removal_does_not() {
    let mut f = Fixture::new();
    let (source, result) = managed_result(&f);
    f.start();
    let accepted=f.call("accept","review.accept",json!({"id":"run-1","resultRevision":result,"evidence":["Reviewed exact commit and required checks"]}));
    assert_eq!(accepted["result"]["state"], "accepted", "{accepted}");
    f.stop();
    f.git(&["merge", "--ff-only", "result"]);
    f.git(&["worktree", "remove", source.to_str().unwrap()]);
    let registry_path = f.repo.join(".git/ontrack-worktrees/registry.json");
    let mut registry: Value = serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    registry["entries"][0]["workspace_closed"] = json!(true);
    registry["entries"][0]["checkout_removed"] = json!(true);
    registry["entries"][0]["cleanup_source_tip"] = json!(result);
    registry["entries"][0]["cleanup_pending"] = Value::Null;
    fs::write(&registry_path, serde_json::to_vec(&registry).unwrap()).unwrap();
    fs::remove_file(f.root.join("native-agent.json")).unwrap();
    f.start();
    let rows = f.call("archived", "worktrees.list", json!({}));
    assert_eq!(rows["result"][0]["status"], "checkout_removed", "{rows}");
    assert_eq!(rows["result"][0]["review"]["state"], "accepted", "{rows}");
    assert_eq!(rows["result"][0]["review"]["validity"], "current", "{rows}");
    f.stop();
    registry["entries"][0]["checkout_removed_externally"] = json!(true);
    fs::write(&registry_path, serde_json::to_vec(&registry).unwrap()).unwrap();
    f.start();
    let rows = f.call("external", "worktrees.list", json!({}));
    assert_eq!(rows["result"][0]["review"]["validity"], "unknown", "{rows}");
}

#[test]
fn unavailable_selected_agent_is_a_known_no_effect_precondition_failure() {
    let mut f = Fixture::new();
    f.start();
    let response = f.call(
        "invalid-agent-start",
        "session.start",
        json!({
            "frontId":"front-1", "agentId":"not-installed", "input":"No tool use", "context":{}
        }),
    );
    assert_eq!(response["error"]["code"], "precondition_failed");
    let operation = f.call(
        "inspect-no-effect",
        "operation.get",
        json!({"id":"invalid-agent-start"}),
    );
    assert_eq!(operation["result"]["state"], "failed");
    assert!(!f.repo.join(".git/ontrack-runtime/sessions").exists());
}

#[test]
fn tui_uncertain_backend_reply_is_not_journaled_as_confirmed_or_replayed() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let mut fixture = Fixture::new();
    fixture.start();
    fs::write(fixture.root.join("malformed-create"), "yes").unwrap();
    let send = |id: &str| {
        let socket = fs::read_dir(fixture.root.join("sockets"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "sock")
            })
            .unwrap();
        let mut stream = UnixStream::connect(socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let request = json!({"version":1,"id":id,"method":"_tui","params":{"request":{"Create":{"title":"One wave","description":"One outcome","issue_type":"task","priority":2,"harness":null}}}});
        let packet = json!({"protocol":1,"repository":fixture.repo.join(".git").canonicalize().unwrap(),"request":request});
        serde_json::to_writer(&mut stream, &packet).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        serde_json::from_slice::<Value>(&bytes).unwrap()["response"].clone()
    };
    let response = send("tui-create");
    assert!(
        response["result"][0]["Created"]["Err"].is_string(),
        "Typed response must reach TUI: {response}"
    );
    let operation = fixture.call("inspect-tui", "operation.get", json!({"id":"tui-create"}));
    assert_eq!(operation["result"]["state"], "unknown", "{operation}");
    assert_eq!(send("tui-create")["error"]["code"], "operation_unknown");
    assert_eq!(
        fs::read_to_string(fixture.root.join("create-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn runtime_cleanup_requires_separate_explicit_bead_closure() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let mut fixture = Fixture::new();
    let (source, result) = managed_result(&fixture);
    fixture.start();
    let accepted = fixture.call(
        "accept-before-cleanup",
        "review.accept",
        json!({"id":"run-1","resultRevision":result,"evidence":["Reviewed required checks"]}),
    );
    assert_eq!(accepted["result"]["state"], "accepted", "{accepted}");
    fixture.git(&["merge", "--ff-only", "result"]);
    let socket = fs::read_dir(fixture.root.join("sockets"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sock")
        })
        .unwrap();
    let mut stream = UnixStream::connect(socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(180)))
        .unwrap();
    let request = json!({"version":1,"id":"cleanup-before-close","method":"_tui","params":{"request":{"WorktreeAction":["run-1","RemoveCheckout"]}}});
    let packet = json!({"protocol":1,"repository":fixture.repo.join(".git").canonicalize().unwrap(),"request":request});
    serde_json::to_writer(&mut stream, &packet).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let response: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        response["response"]["error"]["code"], "precondition_failed",
        "{response}"
    );
    let operation = fixture.call(
        "inspect-cleanup",
        "operation.get",
        json!({"id":"cleanup-before-close"}),
    );
    assert_eq!(
        operation["result"]["state"], "failed",
        "Known precondition rejection must remain safe to retry: {operation}"
    );
    assert!(source.exists());
    let beads: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("beads.json")).unwrap()).unwrap();
    assert_eq!(beads[0]["status"], "in_progress");
}

#[test]
fn integration_requires_acceptance_of_the_exact_clean_result() {
    let mut fixture = Fixture::new();
    let (source, result) = managed_result(&fixture);
    fixture.start();
    let denied = fixture.call(
        "integration-unaccepted",
        "worktree.integrate",
        json!({"id":"run-1","resultRevision":result}),
    );
    assert_eq!(denied["error"]["code"], "precondition_failed", "{denied}");
    let accepted = fixture.call(
        "accept-integration",
        "review.accept",
        json!({"id":"run-1","resultRevision":result,"evidence":["Reviewed required checks"]}),
    );
    assert_eq!(accepted["result"]["state"], "accepted", "{accepted}");
    let stale = fixture.call(
        "integration-stale",
        "worktree.integrate",
        json!({"id":"run-1","resultRevision":"0".repeat(40)}),
    );
    assert_eq!(stale["error"]["code"], "precondition_failed", "{stale}");
    fs::write(source.join("work"), "changed after acceptance").unwrap();
    let dirty = fixture.call(
        "integration-dirty",
        "worktree.integrate",
        json!({"id":"run-1","resultRevision":result}),
    );
    assert_eq!(dirty["error"]["code"], "precondition_failed", "{dirty}");
    let registry: Value = serde_json::from_slice(
        &fs::read(fixture.repo.join(".git/ontrack-worktrees/registry.json")).unwrap(),
    )
    .unwrap();
    assert!(registry["entries"][0]["merge_request"].is_null());
    assert!(registry["entries"][0]["merge_queue"].is_null());
    assert_ne!(fixture.git(&["rev-parse", "HEAD"]), result);
}

#[test]
fn abandoned_and_malformed_clients_do_not_stop_the_owner() {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    let mut fixture = Fixture::new();
    fixture.start();
    let socket = fs::read_dir(fixture.root.join("sockets"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sock")
        })
        .unwrap();
    for payload in [b"".as_slice(), b"{invalid}\n".as_slice()] {
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream.write_all(payload).unwrap();
        drop(stream);
    }
    let status = fixture.call("still-running", "runtime.status", json!({}));
    assert_eq!(status["result"]["state"], "running", "{status}");
    assert!(
        fixture
            .owner
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none()
    );
}
