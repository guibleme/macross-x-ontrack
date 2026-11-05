//! Interactive planning session. Herdr carries the conversation; OnTrack owns issue creation.

use std::{io, path::Path, process::Command};

use serde_json::Value;

use crate::{beads, epic::Session, herdr};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    pub title: String,
    pub description: String,
    pub acceptance: String,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub kind: String,
    pub root: Item,
    pub tasks: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Message {
    Question { text: String, options: Vec<String> },
    Plan(Plan),
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn run(args: &[&str]) -> io::Result<Vec<u8>> {
    let output = Command::new("herdr").args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "Herdr exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn envelope(bytes: &[u8], expected: &str) -> io::Result<Value> {
    let value = herdr::envelope(bytes)?;
    if value["type"] != expected {
        return Err(invalid(format!("Expected Herdr {expected} response")));
    }
    Ok(value)
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_-".contains(&byte))
}

fn checked_session(session: &Session) -> io::Result<()> {
    if !safe_id(&session.workspace_id)
        || !safe_id(&session.tab_id)
        || !safe_id(&session.pane_id)
        || !session
            .tab_id
            .starts_with(&format!("{}:", session.workspace_id))
        || !session
            .pane_id
            .starts_with(&format!("{}:", session.workspace_id))
    {
        return Err(invalid("Unsafe planning session identity"));
    }
    Ok(())
}

pub fn launch(proposal: &str, marker: &str) -> io::Result<Session> {
    let cwd = std::env::current_dir()?.canonicalize()?;
    let source_id = std::env::var("HERDR_WORKSPACE_ID")
        .map_err(|_| invalid("Open OnTrack inside a Herdr workspace to plan"))?;
    launch_with(&cwd, &source_id, proposal, marker, run)
}

fn launch_with(
    cwd: &Path,
    source_id: &str,
    proposal: &str,
    marker: &str,
    mut command: impl FnMut(&[&str]) -> io::Result<Vec<u8>>,
) -> io::Result<Session> {
    if proposal.trim().is_empty() || !safe_id(source_id) || !safe_id(marker) {
        return Err(invalid(
            "Planning needs a proposal and valid Herdr identity",
        ));
    }
    let workspaces = envelope(&command(&["workspace", "list"])?, "workspace_list")?;
    if !workspaces["workspaces"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["workspace_id"].as_str() == Some(source_id))
    }) {
        return Err(invalid("Source Herdr workspace is no longer open"));
    }
    let supported = herdr::supported_agents(&command(&["agent", "start", "--help"])?)?;
    let available = herdr::available_agents(&command(&["integration", "status"])?, &supported)?;
    let agent = available
        .iter()
        .find(|agent| agent.as_str() == "codex")
        .unwrap_or(&available[0]);
    let trust = if agent == "codex" {
        Some(herdr::codex_startup_trust(cwd)?)
    } else {
        None
    };
    let cwd = cwd
        .to_str()
        .ok_or_else(|| invalid("Current checkout path is not UTF-8"))?;
    let created = command(&[
        "tab", "create", "--workspace", source_id, "--cwd", cwd, "--label", "OnTrack planning", "--no-focus",
    ])
    .and_then(|bytes| envelope(&bytes, "tab_created"))
    .map_err(|error| io::Error::other(format!(
        "Planning tab creation may have succeeded: {error}. Inspect workspace {source_id} before retrying"
    )))?;
    let field = |object: &Value, key: &str| -> io::Result<String> {
        object[key]
            .as_str()
            .filter(|value| safe_id(value))
            .map(str::to_owned)
            .ok_or_else(|| invalid(format!("Herdr response lacks safe {key}")))
    };
    let session = Session {
        workspace_id: field(&created["tab"], "workspace_id")?,
        tab_id: field(&created["tab"], "tab_id")?,
        pane_id: field(&created["root_pane"], "pane_id")?,
    };
    checked_session(&session)?;
    if session.workspace_id != source_id
        || created["root_pane"]["workspace_id"] != source_id
        || created["root_pane"]["tab_id"] != session.tab_id
    {
        return Err(invalid("Herdr returned inconsistent planning tab IDs"));
    }
    // Keep the marker's changing tail, while Herdr gets a lowercase alias of
    // at most 32 bytes. The response marker itself must remain untouched.
    let suffix = &marker[marker.len().saturating_sub(19)..];
    let name = format!(
        "ontrack-plan-{}",
        suffix.replace(':', "_").to_ascii_lowercase()
    );
    let mut args = vec![
        "agent",
        "start",
        &name,
        "--kind",
        agent,
        "--pane",
        &session.pane_id,
        "--timeout",
        "30000",
    ];
    if let Some(trust) = trust.as_deref() {
        args.extend([
            "--",
            "-c",
            trust,
            "-c",
            "check_for_update_on_startup=false",
            "--dangerously-bypass-hook-trust",
        ]);
    }
    let started = command(&args)
        .and_then(|bytes| envelope(&bytes, "agent_started"))
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "Agent start failed or its outcome is uncertain in Herdr tab {}: {error}. Inspect that tab before retrying",
                    session.tab_id
                ),
            )
        })?;
    check_agent(&started, &session, "agent_started")?;
    let prompt = format!(
        "You are planning new Beads for this workspace. Read AGENTS.md and CLAUDE.md and inspect relevant project context. Discuss the proposal with the user through OnTrack. Do not create, update, close, or implement Beads. Do not call an interactive question tool; ask the user by ending your turn with exactly one single-line JSON message prefixed by {marker} followed by one space. For a question use keys kind=question, text (string), options (array of 2-5 concise strings). For a final plan use kind=plan, issue_type=task or epic, root (object with title, description, acceptance, priority integer 0-4), tasks (array of objects with the same four fields; empty for a task, at least two for an epic). Keep JSON on one line, without a code fence. The root and each task must have actionable descriptions and testable acceptance. Ask clarifying questions when needed. If the user requests a revision, issue another question or plan. Finish your turn after the JSON. Never write Beads; OnTrack presents the plan for explicit confirmation and creates them.\n\nProposal:\n{proposal}"
    );
    submit_with(&session, &prompt, &mut command)?;
    Ok(session)
}

fn check_agent(value: &Value, session: &Session, expected: &str) -> io::Result<()> {
    if value["type"] != expected
        || value["agent"]["pane_id"] != session.pane_id
        || value["agent"]["workspace_id"] != session.workspace_id
        || value["agent"]["tab_id"] != session.tab_id
    {
        return Err(invalid("Herdr returned a different planning agent"));
    }
    Ok(())
}

fn submit_with(
    session: &Session,
    message: &str,
    command: &mut impl FnMut(&[&str]) -> io::Result<Vec<u8>>,
) -> io::Result<()> {
    checked_session(session)?;
    let result = command(&[
        "agent",
        "prompt",
        &session.pane_id,
        message,
        "--wait",
        "--until",
        "working",
    ])
    .and_then(|bytes| envelope(&bytes, "agent_prompted"))
    .map_err(|error| {
        io::Error::other(format!(
            "Prompt may have reached planning pane {}; inspect it before retrying: {error}",
            session.pane_id
        ))
    })?;
    check_agent(&result, session, "agent_prompted")
}

pub fn reply(session: &Session, message: &str, marker: &str) -> io::Result<()> {
    if message.trim().is_empty() || !safe_id(marker) {
        return Err(invalid("Planning reply is empty or marker invalid"));
    }
    let prompt = format!(
        "User reply: {message}\nContinue planning. End this turn with one single-line JSON message prefixed by {marker} and one space. Use the question or plan shape specified earlier. Do not create Beads."
    );
    submit_with(session, &prompt, &mut run)
}

pub fn read(session: &Session, marker: &str) -> io::Result<Option<Message>> {
    read_with(session, marker, run)
}

fn read_with(
    session: &Session,
    marker: &str,
    mut command: impl FnMut(&[&str]) -> io::Result<Vec<u8>>,
) -> io::Result<Option<Message>> {
    checked_session(session)?;
    let bytes = command(&[
        "agent",
        "read",
        &session.pane_id,
        "--source",
        "recent-unwrapped",
        "--lines",
        "300",
    ])
    .or_else(|_| {
        command(&[
            "pane",
            "read",
            &session.pane_id,
            "--source",
            "recent-unwrapped",
            "--lines",
            "300",
        ])
    })?;
    parse_output(&String::from_utf8_lossy(&bytes), marker)
}

fn item(value: &Value) -> io::Result<Item> {
    let required = |key: &str| -> io::Result<String> {
        value[key]
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| invalid(format!("Plan item lacks {key}")))
    };
    let priority = value["priority"]
        .as_u64()
        .filter(|value| *value <= 4)
        .ok_or_else(|| invalid("Plan item priority must be P0–P4"))? as u8;
    let title = required("title")?;
    if title.contains(['\n', '\r']) {
        return Err(invalid("Plan item title must be one line"));
    }
    Ok(Item {
        title,
        description: required("description")?,
        acceptance: required("acceptance")?,
        priority,
    })
}

pub fn parse_output(output: &str, marker: &str) -> io::Result<Option<Message>> {
    if !safe_id(marker) {
        return Err(invalid("Invalid planning marker"));
    }
    let prefix = format!("{marker} ");
    let lines: Vec<_> = output.lines().collect();
    let Some((index, start)) = lines.iter().enumerate().rev().find_map(|(index, line)| {
        let offset = line.find(&prefix)?;
        let start = &line[offset + prefix.len()..];
        // The agent's submitted prompt also contains the marker in prose.
        // Until a JSON response appears, that echo is not an agent answer.
        start
            .trim_start()
            .starts_with('{')
            .then_some((index, start))
    }) else {
        return Ok(None);
    };
    // Herdr may return a visual terminal row break even with recent-unwrapped.
    // Join only while JSON is incomplete, so later agent/status text is ignored.
    let mut response = start.to_owned();
    let mut continuation = lines[index + 1..].iter();
    let value: Value = loop {
        match serde_json::from_str(&response) {
            Ok(value) => break value,
            Err(error) if error.is_eof() => {
                let Some(next) = continuation.next() else {
                    // A terminal snapshot can catch the agent midway through
                    // streaming its JSON. The next poll will read it again.
                    return Ok(None);
                };
                response.push_str(next);
            }
            Err(error) => {
                return Err(invalid(format!(
                    "Agent planning response is invalid JSON: {error}"
                )));
            }
        }
    };
    match value["kind"].as_str() {
        Some("question") => {
            let text = value["text"]
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| invalid("Agent question has no text"))?
                .to_owned();
            let options = value["options"]
                .as_array()
                .ok_or_else(|| invalid("Agent question has no options"))?
                .iter()
                .map(|option| {
                    option
                        .as_str()
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_owned)
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid("Agent question has an empty option"))?;
            if !(2..=5).contains(&options.len()) {
                return Err(invalid("Agent question must have 2–5 options"));
            }
            Ok(Some(Message::Question { text, options }))
        }
        Some("plan") => {
            let kind = value["issue_type"]
                .as_str()
                .filter(|kind| *kind == "task" || *kind == "epic")
                .ok_or_else(|| invalid("Plan must be a task or epic"))?
                .to_owned();
            let root = item(&value["root"])?;
            let tasks = value["tasks"]
                .as_array()
                .ok_or_else(|| invalid("Plan lacks tasks array"))?
                .iter()
                .map(item)
                .collect::<io::Result<Vec<_>>>()?;
            if (kind == "task" && !tasks.is_empty()) || (kind == "epic" && tasks.len() < 2) {
                return Err(invalid(
                    "Task plans need no children; Epic plans need at least two child Tasks",
                ));
            }
            Ok(Some(Message::Plan(Plan { kind, root, tasks })))
        }
        _ => Err(invalid("Agent planning response has unknown kind")),
    }
}

pub fn create(plan: &Plan) -> Result<Vec<String>, String> {
    create_with(plan, beads::create_planned)
}

fn create_with(
    plan: &Plan,
    mut create: impl FnMut(&Item, &str, Option<&str>) -> io::Result<String>,
) -> Result<Vec<String>, String> {
    let mut created = Vec::new();
    let root = create(&plan.root, &plan.kind, None).map_err(|error| {
        format!("Root creation failed or its outcome is uncertain: {error}. Inspect Beads before retrying")
    })?;
    created.push(root.clone());
    for task in &plan.tasks {
        match create(task, "task", Some(&root)) {
            Ok(id) => created.push(id),
            Err(error) => {
                return Err(format!(
                    "Created {}. Child creation failed: {error}. Do not retry this plan; inspect these Beads before continuing",
                    created.join(", ")
                ));
            }
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_launch_passes_trust_and_never_prompts_after_failed_or_mismatched_start() {
        let root = std::env::temp_dir().join(format!(
            "ontrack-planning-{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        assert!(
            Command::new("git")
                .arg("init")
                .arg("--quiet")
                .arg(&root)
                .status()
                .unwrap()
                .success()
        );
        let root = root.canonicalize().unwrap();
        let expected_trust = format!(
            "projects={{{}={{trust_level=\"trusted\"}}}}",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        );
        for mode in ["ready", "blocked", "workspace", "tab", "pane"] {
            let mut calls = Vec::<Vec<String>>::new();
            let result = launch_with(
                &root,
                "wSource",
                "Plan a release",
                "ONTRACK_PLAN_1",
                |args| {
                    calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
                    let response = match calls.len() {
                    1 => json!({"result":{"type":"workspace_list","workspaces":[{"workspace_id":"wSource"}]}}),
                    2 => return Ok(b"--kind <KIND> [possible values: codex, opencode]".to_vec()),
                    3 => return Ok(b"codex: current (v8) (/tmp/codex)\nopencode: current (v1) (/tmp/opencode)".to_vec()),
                    4 => json!({"result":{"type":"tab_created","tab":{"workspace_id":"wSource","tab_id":"wSource:t2"},"root_pane":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}),
                    5 => {
                        assert_eq!(&args[3..], ["--kind", "codex", "--pane", "wSource:p2", "--timeout", "30000", "--", "-c", &expected_trust, "-c", "check_for_update_on_startup=false", "--dangerously-bypass-hook-trust"]);
                        if mode == "blocked" {
                            return Err(io::Error::other("agent_not_ready: blocked during startup"));
                        }
                        let mut agent = json!({"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"});
                        match mode {
                            "workspace" => agent["workspace_id"] = json!("wOther"),
                            "tab" => agent["tab_id"] = json!("wSource:t3"),
                            "pane" => agent["pane_id"] = json!("wSource:p3"),
                            _ => {}
                        }
                        json!({"result":{"type":"agent_started","agent":agent}})
                    }
                    6 => json!({"result":{"type":"agent_prompted","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}),
                    _ => panic!("unexpected call: {args:?}"),
                };
                    Ok(response.to_string().into_bytes())
                },
            );
            if mode == "ready" {
                assert_eq!(result.unwrap().pane_id, "wSource:p2");
                assert_eq!(&calls[5][..3], ["agent", "prompt", "wSource:p2"]);
                assert!(calls[5][3].contains("Plan a release"));
                assert_eq!(calls.len(), 6);
            } else {
                let error = result.unwrap_err().to_string();
                if mode == "blocked" {
                    assert!(error.contains("wSource:t2"));
                    assert!(error.contains("Inspect that tab before retrying"));
                }
                assert_eq!(calls.len(), 5, "no prompt, retry, or tab close: {mode}");
            }
        }
    }

    #[test]
    fn launch_uses_herdr_alias_and_preserves_response_marker() {
        let marker = "ONTRACK_PLAN_1_0123456789ABCDEF";
        let mut calls = Vec::<Vec<String>>::new();
        let session = launch_with(
            Path::new("/tmp/ontrack-planning-test"),
            "wSource",
            "Plan a release",
            marker,
            |args| {
                let response = match calls.len() {
                    0 => json!({"result":{"type":"workspace_list","workspaces":[{"workspace_id":"wSource"}]}}).to_string(),
                    1 => "--kind <KIND> [possible values: opencode]".to_owned(),
                    2 => "opencode: current (v1) (/tmp/opencode)".to_owned(),
                    3 => json!({"result":{"type":"tab_created","tab":{"workspace_id":"wSource","tab_id":"wSource:t2"},"root_pane":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}).to_string(),
                    4 => {
                        let alias = args[2];
                        if alias.len() > 32
                            || !alias.starts_with(|c: char| c.is_ascii_lowercase())
                            || !alias.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
                        {
                            return Err(io::Error::other("agent name needs to be lowercase"));
                        }
                        json!({"result":{"type":"agent_started","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}).to_string()
                    }
                    5 => json!({"result":{"type":"agent_prompted","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}).to_string(),
                    _ => panic!("unexpected Herdr call"),
                };
                calls.push(args.iter().map(|arg| (*arg).to_owned()).collect());
                Ok(response.into_bytes())
            },
        )
        .unwrap();
        assert_eq!(session.tab_id, "wSource:t2");
        assert_eq!(calls.len(), 6);
        assert_eq!(&calls[4][..2], ["agent", "start"]);
        assert!(calls[4][2].ends_with("0123456789abcdef"));
        assert_eq!(
            &calls[4][3..],
            [
                "--kind",
                "opencode",
                "--pane",
                "wSource:p2",
                "--timeout",
                "30000"
            ]
        );
        assert_eq!(&calls[5][..3], ["agent", "prompt", "wSource:p2"]);
        assert!(calls[5][3].contains(marker));
        assert!(calls[5][3].contains("Plan a release"));
        assert_eq!(
            parse_output(
                &format!("{marker} {{\"kind\":\"question\",\"text\":\"Scope?\",\"options\":[\"A\",\"B\"]}}"),
                marker,
            )
            .unwrap(),
            Some(Message::Question {
                text: "Scope?".into(),
                options: vec!["A".into(), "B".into()],
            })
        );
    }

    #[test]
    fn parses_question_and_epic_and_rejects_incomplete_plan() {
        let q = r#"ONTRACK_x {"kind":"question","text":"Scope?","options":["A","B"]}"#;
        assert_eq!(
            parse_output(q, "ONTRACK_x").unwrap(),
            Some(Message::Question {
                text: "Scope?".into(),
                options: vec!["A".into(), "B".into()]
            })
        );
        let plan = r#"ONTRACK_x {"kind":"plan","issue_type":"epic","root":{"title":"Epic","description":"Why","acceptance":"Check","priority":2},"tasks":[{"title":"Child","description":"Do","acceptance":"Test","priority":1},{"title":"Second","description":"Do more","acceptance":"Check more","priority":2}]}"#;
        assert!(
            matches!(parse_output(plan, "ONTRACK_x").unwrap(), Some(Message::Plan(Plan { tasks, .. })) if tasks.len() == 2)
        );
        assert!(
            parse_output(
                &plan.replace("\"acceptance\":\"Test\"", "\"acceptance\":\"\""),
                "ONTRACK_x"
            )
            .is_err()
        );
        assert_eq!(parse_output(q, "ONTRACK_other").unwrap(), None);
    }

    #[test]
    fn parses_question_wrapped_across_herdr_terminal_rows() {
        let output = concat!(
            "› Planning prompt mentioning ONTRACK_x followed by JSON\n",
            "• ONTRACK_x {\"kind\":\"question\",\"text\":\"What scope should the TODO app have?\",",
            "\"options\":[\"Single-user app with task CRUD and storage\",\"Single-user app with\n",
            "  due dates, priorities, and filters too\",\"Multi-user app with accounts\"]}\n",
            "Worked for 34s\n",
        );
        assert_eq!(
            parse_output(output, "ONTRACK_x").unwrap(),
            Some(Message::Question {
                text: "What scope should the TODO app have?".into(),
                options: vec![
                    "Single-user app with task CRUD and storage".into(),
                    "Single-user app with  due dates, priorities, and filters too".into(),
                    "Multi-user app with accounts".into(),
                ],
            })
        );
        assert!(
            parse_output("ONTRACK_x {\"kind\":oops}", "ONTRACK_x")
                .unwrap_err()
                .to_string()
                .contains("invalid JSON")
        );
    }

    #[test]
    fn prompt_echo_and_streaming_response_keep_polling_pending() {
        let prompt = "› End your turn prefixed by ONTRACK_x followed by one space.";
        assert_eq!(parse_output(prompt, "ONTRACK_x").unwrap(), None);
        let partial = format!("{prompt}\n• ONTRACK_x {{\"kind\":\"question\",\"text\":\"Scope?\"");
        assert_eq!(parse_output(&partial, "ONTRACK_x").unwrap(), None);
        let complete = format!("{partial},\"options\":[\"A\",\"B\"]}}\nWorked for 2s");
        assert_eq!(
            parse_output(&complete, "ONTRACK_x").unwrap(),
            Some(Message::Question {
                text: "Scope?".into(),
                options: vec!["A".into(), "B".into()],
            })
        );
    }

    #[test]
    fn reads_recorded_agent_pane_as_plain_terminal_text() {
        let session = Session {
            workspace_id: "w1".into(),
            tab_id: "w1:t2".into(),
            pane_id: "w1:p2".into(),
        };
        let output =
            b"ONTRACK_x {\"kind\":\"question\",\"text\":\"Scope?\",\"options\":[\"A\",\"B\"]}";
        for agent_read_succeeds in [true, false] {
            let mut calls = Vec::new();
            let message = read_with(&session, "ONTRACK_x", |args| {
                calls.push(args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>());
                if args[0] == "agent" && !agent_read_succeeds {
                    Err(io::Error::other("agent read unavailable"))
                } else {
                    Ok(output.to_vec())
                }
            })
            .unwrap();
            assert_eq!(
                message,
                Some(Message::Question {
                    text: "Scope?".into(),
                    options: vec!["A".into(), "B".into()],
                })
            );
            assert_eq!(
                calls[0],
                [
                    "agent",
                    "read",
                    "w1:p2",
                    "--source",
                    "recent-unwrapped",
                    "--lines",
                    "300"
                ]
            );
            if agent_read_succeeds {
                assert_eq!(calls.len(), 1);
            } else {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[1][..3], ["pane", "read", "w1:p2"]);
            }
        }
    }

    #[test]
    fn creates_epic_then_children_and_reports_partial_failure() {
        let item = Item {
            title: "Work".into(),
            description: "Do work".into(),
            acceptance: "Test work".into(),
            priority: 2,
        };
        let plan = Plan {
            kind: "epic".into(),
            root: item.clone(),
            tasks: vec![item.clone(), item],
        };
        let mut calls = Vec::new();
        let result = create_with(&plan, |_, kind, parent| {
            calls.push((kind.to_owned(), parent.map(str::to_owned)));
            match calls.len() {
                1 => Ok("epic-1".into()),
                2 => Ok("child-1".into()),
                _ => Err(io::Error::other("offline")),
            }
        });
        assert_eq!(
            calls,
            [
                ("epic".into(), None),
                ("task".into(), Some("epic-1".into())),
                ("task".into(), Some("epic-1".into()))
            ]
        );
        assert!(result.unwrap_err().contains("epic-1, child-1"));
    }
}
