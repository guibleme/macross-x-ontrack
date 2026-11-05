//! Opt-in Epic planning in a dedicated tab of the current Herdr workspace.

use std::{io, path::Path, process::Command};

use serde_json::Value;

use crate::{beads, herdr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
}

trait Backend {
    fn herdr(&mut self, args: &[&str]) -> io::Result<Vec<u8>>;
    fn codex_trust(&mut self, checkout: &Path) -> io::Result<String>;
    fn show(&mut self, id: &str) -> io::Result<Value>;
    fn children(&mut self, id: &str) -> io::Result<Vec<String>>;
}

struct System;

impl Backend for System {
    fn codex_trust(&mut self, checkout: &Path) -> io::Result<String> {
        herdr::codex_startup_trust(checkout)
    }

    fn herdr(&mut self, args: &[&str]) -> io::Result<Vec<u8>> {
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

    fn show(&mut self, id: &str) -> io::Result<Value> {
        beads::show_bead(id)
    }

    fn children(&mut self, id: &str) -> io::Result<Vec<String>> {
        Ok(beads::list_beads()?
            .into_iter()
            .filter(|bead| bead.data["parent"].as_str() == Some(id))
            .map(|bead| bead.id)
            .collect())
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn id<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    let value = value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(format!("Herdr response lacks {key}")))?;
    if value.starts_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":_-".contains(&byte))
    {
        return Err(invalid(format!("Herdr response has unsafe {key}")));
    }
    Ok(value)
}

fn result(bytes: &[u8], expected: &str) -> io::Result<Value> {
    let value = herdr::envelope(bytes)?;
    if value["type"] != expected {
        return Err(invalid(format!("Expected Herdr {expected} response")));
    }
    Ok(value)
}

pub fn launch(epic_id: &str) -> io::Result<Session> {
    let cwd = std::env::current_dir()?;
    let source_id = std::env::var("HERDR_WORKSPACE_ID")
        .map_err(|_| invalid("Open MXO inside a Herdr workspace to plan an Epic"))?;
    launch_with(&mut System, &cwd, &source_id, epic_id)
}

fn launch_with(
    backend: &mut impl Backend,
    cwd: &Path,
    source_id: &str,
    epic_id: &str,
) -> io::Result<Session> {
    // Check the inherited ID against the live topology. Never use global focus.
    let workspaces = result(&backend.herdr(&["workspace", "list"])?, "workspace_list")?;
    if !workspaces["workspaces"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["workspace_id"].as_str() == Some(source_id))
    }) {
        return Err(invalid("Source Herdr workspace is no longer open"));
    }
    let live = backend.show(epic_id)?;
    if live["issue_type"] != "epic" {
        return Err(invalid("Selected Bead is no longer an Epic"));
    }
    let proposal = live["description"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| invalid("Epic proposal is empty; add a description before planning"))?;
    let supported = herdr::supported_agents(&backend.herdr(&["agent", "start", "--help"])?)?;
    let available =
        herdr::available_agents(&backend.herdr(&["integration", "status"])?, &supported)?;
    let agent = available
        .iter()
        .find(|agent| agent.as_str() == "codex")
        .unwrap_or(&available[0]);
    let cwd = cwd.canonicalize()?;
    // Planning shares this checkout; validate trust before creating anything.
    let trust_override = if agent == "codex" {
        Some(backend.codex_trust(&cwd).map_err(|error| {
            invalid(format!(
                "Codex repository trust validation failed; no planning tab created: {error}"
            ))
        })?)
    } else {
        None
    };
    let cwd = cwd
        .to_str()
        .ok_or_else(|| invalid("Current checkout path is not UTF-8"))?;
    let label = format!("Epic {epic_id} planning");
    let created = backend
        .herdr(&[
            "tab", "create", "--workspace", source_id, "--cwd", cwd, "--label", &label,
            "--no-focus",
        ])
        .and_then(|bytes| result(&bytes, "tab_created"))
        .map_err(|error| io::Error::other(format!(
            "Planning tab creation may have succeeded: {error}. Inspect Herdr workspace {source_id} before retrying"
        )))?;
    let workspace_id = id(&created["tab"], "workspace_id")?.to_owned();
    let tab_id = id(&created["tab"], "tab_id")?.to_owned();
    let pane_id = id(&created["root_pane"], "pane_id")?.to_owned();
    if workspace_id != source_id
        || created["root_pane"]["workspace_id"] != workspace_id
        || created["root_pane"]["tab_id"] != tab_id
        || !tab_id.starts_with(&format!("{workspace_id}:"))
        || !pane_id.starts_with(&format!("{workspace_id}:"))
    {
        return Err(invalid("Herdr returned inconsistent Epic tab and pane IDs"));
    }
    let session = Session {
        workspace_id,
        tab_id,
        pane_id,
    };
    let name = format!(
        "epic-{}",
        epic_id
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(16)
            .map(|ch| ch.to_ascii_lowercase())
            .collect::<String>()
    );
    let mut start_args = vec![
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
    if let Some(trust_override) = trust_override.as_deref() {
        start_args.extend([
            "--",
            "-c",
            trust_override,
            "-c",
            "check_for_update_on_startup=false",
            "--dangerously-bypass-hook-trust",
        ]);
    }
    let started = backend.herdr(&start_args);
    let started = started.and_then(|bytes| result(&bytes, "agent_started"));
    if started.as_ref().map_or(true, |value| {
        value["agent"]["pane_id"] != session.pane_id
            || value["agent"]["workspace_id"] != session.workspace_id
            || value["agent"]["tab_id"] != session.tab_id
    }) {
        return Err(io::Error::other(format!(
            "Agent start failed or identity changed in workspace {} tab {}. No planning prompt was sent. Inspect pane {} for login, policy, or other startup input and resolve it before retrying generation; the agent may still be running: {}",
            session.workspace_id,
            session.tab_id,
            session.pane_id,
            started
                .err()
                .map_or_else(|| "identity mismatch".to_owned(), |error| error.to_string())
        )));
    }
    let prompt = format!(
        "Create actionable direct child Beads for Epic {epic_id} from the proposal below. Read AGENTS.md and CLAUDE.md first. Use bd create with --parent={epic_id} for each child; give each task a useful description and acceptance criteria. Choose a Beads priority for each child on the P0-P4 scale (P0 highest, P4 lowest), comparing the tasks by impact, urgency, and which work enables other tasks. Pass --priority=N explicitly on every bd create. Make meaningful priority distinctions when the proposal supports them instead of assigning every child the same default priority. Equal priorities are fine when tasks are genuinely comparable or the proposal gives too little evidence; do not invent urgency just to vary the numbers. Do not implement the tasks. Verify at least one direct child exists. Report the child IDs and then finish your turn. The MXO parent process will verify and close this planning tab; if it is no longer running, close only this tab ({}) after verification and only if another tab remains. Keep its workspace ({}) open.\n\nProposal:\n{proposal}",
        session.tab_id, session.workspace_id
    );
    // Wait for the server to observe this prompt's working state before a
    // separate completion wait. This avoids matching the pre-prompt idle state.
    let prompted = backend.herdr(&[
        "agent",
        "prompt",
        &session.pane_id,
        &prompt,
        "--wait",
        "--until",
        "working",
    ]);
    let prompted = prompted.and_then(|bytes| result(&bytes, "agent_prompted"));
    if prompted.as_ref().map_or(true, |value| {
        value["agent"]["pane_id"] != session.pane_id
            || value["agent"]["workspace_id"] != session.workspace_id
            || value["agent"]["tab_id"] != session.tab_id
    }) {
        return Err(io::Error::other(format!(
            "Prompt may have been delivered in workspace {} tab {} pane {}. Inspect the agent before retrying; do not generate duplicate children: {}",
            session.workspace_id,
            session.tab_id,
            session.pane_id,
            prompted
                .err()
                .map_or_else(|| "identity mismatch".to_owned(), |error| error.to_string())
        )));
    }
    Ok(session)
}

pub fn finish(session: &Session, epic_id: &str) -> io::Result<Vec<String>> {
    finish_with(&mut System, session, epic_id).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "{error}. Inspect Herdr workspace {} tab {} and the Epic's children before retrying",
                session.workspace_id,
                session.tab_id
            ),
        )
    })
}

fn finish_with(
    backend: &mut impl Backend,
    session: &Session,
    epic_id: &str,
) -> io::Result<Vec<String>> {
    match backend.herdr(&[
        "agent",
        "wait",
        &session.pane_id,
        "--until",
        "done",
        "--until",
        "idle",
        "--until",
        "blocked",
    ]) {
        Ok(bytes) => {
            let waited = result(&bytes, "agent_info")?;
            if waited["agent"]["pane_id"] != session.pane_id
                || waited["agent"]["workspace_id"] != session.workspace_id
                || waited["agent"]["tab_id"] != session.tab_id
                || !matches!(
                    waited["agent"]["agent_status"].as_str(),
                    Some("done" | "idle")
                )
            {
                return Err(invalid(
                    "Herdr wait returned a different agent or unfinished state",
                ));
            }
        }
        Err(wait_error) => {
            // An agent may exit after completing the prompt, removing its name
            // before Herdr can return a settled state. Confirm that the same
            // pane is still present and has no replacement agent.
            let pane = result(
                &backend.herdr(&["pane", "get", &session.pane_id])?,
                "pane_info",
            )?;
            if pane["pane"]["pane_id"] != session.pane_id
                || pane["pane"]["workspace_id"] != session.workspace_id
                || pane["pane"]["tab_id"] != session.tab_id
                || !pane["pane"]["agent"].is_null()
                || !pane["pane"]["agent_status"].is_null()
            {
                return Err(io::Error::new(
                    wait_error.kind(),
                    format!(
                        "Herdr agent wait failed and the planning pane is not empty: {wait_error}"
                    ),
                ));
            }
        }
    }
    let children = backend.children(epic_id)?;
    if children.is_empty() {
        return Err(invalid(format!(
            "No direct child Beads found for {epic_id}; tab {} remains open for recovery",
            session.tab_id
        )));
    }
    let tabs = result(
        &backend.herdr(&["tab", "list", "--workspace", &session.workspace_id])?,
        "tab_list",
    )?;
    let Some(tabs) = tabs["tabs"].as_array() else {
        return Err(invalid(
            "Herdr tab list is missing; planning tab remains open",
        ));
    };
    if !tabs
        .iter()
        .any(|tab| tab["tab_id"] == session.tab_id && tab["workspace_id"] == session.workspace_id)
        || !tabs.iter().any(|tab| {
            tab["tab_id"] != session.tab_id && tab["workspace_id"] == session.workspace_id
        })
    {
        return Err(invalid(
            "Planning tab is missing or is the last tab in its workspace; it remains open for recovery",
        ));
    }
    result(&backend.herdr(&["tab", "close", &session.tab_id])?, "ok")?;
    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;

    struct Mock {
        responses: VecDeque<io::Result<Vec<u8>>>,
        calls: Vec<Vec<String>>,
        children: Vec<String>,
    }

    impl Mock {
        fn new(responses: Vec<Value>, children: Vec<&str>) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|value| Ok(json!({"result": value}).to_string().into_bytes()))
                    .collect(),
                calls: Vec::new(),
                children: children.into_iter().map(str::to_owned).collect(),
            }
        }
    }

    impl Backend for Mock {
        fn codex_trust(&mut self, _checkout: &Path) -> io::Result<String> {
            unreachable!()
        }

        fn herdr(&mut self, args: &[&str]) -> io::Result<Vec<u8>> {
            self.calls
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            self.responses
                .pop_front()
                .unwrap_or_else(|| Err(invalid("No mock response")))
        }

        fn show(&mut self, _id: &str) -> io::Result<Value> {
            Ok(json!({"issue_type":"epic", "description":"A large proposal"}))
        }

        fn children(&mut self, _id: &str) -> io::Result<Vec<String>> {
            Ok(self.children.clone())
        }
    }

    #[derive(Default)]
    struct LaunchMock {
        live: Value,
        step: usize,
        calls: Vec<Vec<String>>,
        no_codex: bool,
        trust_error: bool,
        trust_calls: Vec<std::path::PathBuf>,
        created_result: Option<Value>,
        start_result: Option<io::Result<Value>>,
        prompt_result: Option<Value>,
    }

    impl Backend for LaunchMock {
        fn codex_trust(&mut self, checkout: &Path) -> io::Result<String> {
            assert_eq!(self.calls.len(), 3, "trust must precede tab creation");
            self.trust_calls.push(checkout.to_path_buf());
            if self.trust_error {
                Err(invalid("metadata missing"))
            } else {
                Ok(r#"projects={"/verified/main root"={trust_level="trusted"}}"#.into())
            }
        }

        fn herdr(&mut self, args: &[&str]) -> io::Result<Vec<u8>> {
            self.calls
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            if self.step == 3
                && let Some(created) = self.created_result.take()
            {
                self.step += 1;
                return Ok(json!({"result": created}).to_string().into_bytes());
            }
            if self.step == 4
                && let Some(started) = self.start_result.take()
            {
                self.step += 1;
                return started.map(|value| json!({"result": value}).to_string().into_bytes());
            }
            if self.step == 5
                && let Some(prompted) = self.prompt_result.take()
            {
                self.step += 1;
                return Ok(json!({"result": prompted}).to_string().into_bytes());
            }
            let bytes = match self.step {
                0 => json!({"result":{"type":"workspace_list","workspaces":[{"workspace_id":"wSource"}]}}).to_string(),
                1 => "--kind <KIND> [possible values: codex, opencode]".to_owned(),
                2 if self.no_codex => "opencode: current (v1) (/tmp/opencode)".to_owned(),
                2 => "codex: current (v1) (/tmp/codex)\nopencode: not installed".to_owned(),
                3 => json!({"result":{"type":"tab_created","tab":{"workspace_id":"wSource","tab_id":"wSource:t2"},"root_pane":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}).to_string(),
                4 => json!({"result":{"type":"agent_started","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}).to_string(),
                5 => json!({"result":{"type":"agent_prompted","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}}).to_string(),
                _ => panic!("unexpected Herdr call"),
            };
            self.step += 1;
            Ok(bytes.into_bytes())
        }

        fn show(&mut self, _id: &str) -> io::Result<Value> {
            Ok(self.live.clone())
        }

        fn children(&mut self, _id: &str) -> io::Result<Vec<String>> {
            unreachable!()
        }
    }

    #[test]
    fn launch_revalidates_live_type_and_nonempty_proposal_before_creating_tab() {
        for live in [
            json!({"issue_type":"task", "description":"Proposal"}),
            json!({"issue_type":"epic"}),
            json!({"issue_type":"epic", "description":"  \n "}),
        ] {
            let mut backend = LaunchMock {
                live,
                step: 0,
                calls: Vec::new(),
                ..Default::default()
            };
            assert!(
                launch_with(
                    &mut backend,
                    &std::env::current_dir().unwrap(),
                    "wSource",
                    "epic-1"
                )
                .is_err()
            );
            assert_eq!(backend.calls.len(), 1);
        }
    }

    #[test]
    fn launch_uses_source_workspace_tab_and_parented_prompt_without_worktree() {
        let mut backend = LaunchMock {
            live: json!({"issue_type":"epic", "description":"A large proposal"}),
            step: 0,
            calls: Vec::new(),
            ..Default::default()
        };
        let cwd = std::env::current_dir().unwrap();
        let session = launch_with(&mut backend, &cwd, "wSource", "epic-1").unwrap();
        assert_eq!(session.workspace_id, "wSource");
        assert_eq!(session.tab_id, "wSource:t2");
        assert_eq!(backend.trust_calls, [cwd.canonicalize().unwrap()]);
        assert_eq!(
            backend.calls[4],
            [
                "agent",
                "start",
                "epic-epic1",
                "--kind",
                "codex",
                "--pane",
                "wSource:p2",
                "--timeout",
                "30000",
                "--",
                "-c",
                r#"projects={"/verified/main root"={trust_level="trusted"}}"#,
                "-c",
                "check_for_update_on_startup=false",
                "--dangerously-bypass-hook-trust",
            ]
        );
        assert_eq!(backend.calls.len(), 6);
        assert_eq!(&backend.calls[5][..3], ["agent", "prompt", "wSource:p2"]);
        assert_eq!(
            &backend.calls[3][..4],
            ["tab", "create", "--workspace", "wSource"]
        );
        assert_eq!(
            backend.calls[3][5],
            cwd.canonicalize().unwrap().to_str().unwrap()
        );
        assert!(!backend.calls.iter().any(|call| matches!(
            call.first().map(String::as_str),
            Some("worktree" | "workspace")
        )
            && call.get(1).is_some_and(|arg| arg == "create")));
        assert!(backend.calls[5][3].contains("--parent=epic-1"));
        assert!(backend.calls[5][3].contains("close only this tab (wSource:t2)"));
        assert!(backend.calls[5][3].contains("A large proposal"));
        assert!(backend.calls[5][3].contains("useful description and acceptance criteria"));
        assert!(backend.calls[5][3].contains("P0-P4 scale (P0 highest, P4 lowest)"));
        assert!(
            backend.calls[5][3].contains("impact, urgency, and which work enables other tasks")
        );
        assert!(backend.calls[5][3].contains("--priority=N explicitly on every bd create"));
        assert!(backend.calls[5][3].contains("meaningful priority distinctions"));
        assert!(backend.calls[5][3].contains("Equal priorities are fine"));
        assert_eq!(&backend.calls[5][4..], ["--wait", "--until", "working"]);
    }

    #[test]
    fn launch_normalizes_agent_name_without_changing_epic_id() {
        for epic_id in [
            "OCA_Genie-4ao",
            "OnTrack-7ma",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ-1234567890",
            "é_项目-ABC123",
            "___",
        ] {
            let mut backend = LaunchMock {
                live: json!({"issue_type":"epic", "description":"Proposal"}),
                ..Default::default()
            };
            launch_with(
                &mut backend,
                &std::env::current_dir().unwrap(),
                "wSource",
                epic_id,
            )
            .unwrap();
            let name = &backend.calls[4][2];
            assert!(name.len() <= 32, "{name}");
            assert!(name.as_bytes()[0].is_ascii_lowercase(), "{name}");
            assert!(
                name.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte)
                }),
                "{name}"
            );
            if epic_id == "OCA_Genie-4ao" {
                assert_eq!(name, "epic-ocagenie4ao");
            }
            assert_eq!(backend.calls[3][7], format!("Epic {epic_id} planning"));
            assert!(backend.calls[5][3].contains(&format!("for Epic {epic_id} ")));
            assert!(backend.calls[5][3].contains(&format!("--parent={epic_id} ")));
        }
    }

    #[test]
    fn fallback_agent_has_no_codex_overrides_or_trust_requirement() {
        let mut backend = LaunchMock {
            live: json!({"issue_type":"epic", "description":"Proposal"}),
            no_codex: true,
            trust_error: true,
            ..Default::default()
        };
        launch_with(
            &mut backend,
            &std::env::current_dir().unwrap(),
            "wSource",
            "epic-1",
        )
        .unwrap();
        assert!(backend.trust_calls.is_empty());
        assert_eq!(
            backend.calls[4],
            [
                "agent",
                "start",
                "epic-epic1",
                "--kind",
                "opencode",
                "--pane",
                "wSource:p2",
                "--timeout",
                "30000"
            ]
        );
        assert_eq!(&backend.calls[5][..3], ["agent", "prompt", "wSource:p2"]);
        assert_eq!(backend.calls.len(), 6);
    }

    #[test]
    fn codex_trust_failure_precedes_tab_creation() {
        let mut backend = LaunchMock {
            live: json!({"issue_type":"epic", "description":"Proposal"}),
            trust_error: true,
            ..Default::default()
        };
        let error = launch_with(
            &mut backend,
            &std::env::current_dir().unwrap(),
            "wSource",
            "epic-1",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("no planning tab created"), "{error}");
        assert!(error.contains("metadata missing"), "{error}");
        assert_eq!(backend.calls.len(), 3);
    }

    #[test]
    fn mismatched_created_tab_never_starts_agent() {
        for created in [
            json!({"type":"tab_created","tab":{"workspace_id":"wOther","tab_id":"wOther:t2"},"root_pane":{"workspace_id":"wOther","tab_id":"wOther:t2","pane_id":"wOther:p2"}}),
            json!({"type":"tab_created","tab":{"workspace_id":"wSource","tab_id":"wSource:t2"},"root_pane":{"workspace_id":"wSource","tab_id":"wSource:t3","pane_id":"wSource:p2"}}),
        ] {
            let mut backend = LaunchMock {
                live: json!({"issue_type":"epic", "description":"Proposal"}),
                created_result: Some(created),
                ..Default::default()
            };
            let error = launch_with(
                &mut backend,
                &std::env::current_dir().unwrap(),
                "wSource",
                "epic-1",
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("inconsistent Epic tab and pane IDs"),
                "{error}"
            );
            assert_eq!(backend.calls.len(), 4);
        }
    }

    #[test]
    fn startup_errors_and_identity_mismatches_never_send_planning_prompt() {
        for started in [
            Err(io::Error::other("readiness timed out: login required")),
            Ok(
                json!({"type":"agent_started", "agent":{"workspace_id":"wOther","tab_id":"wSource:t2","pane_id":"wSource:p2"}}),
            ),
            Ok(
                json!({"type":"agent_started", "agent":{"workspace_id":"wSource","tab_id":"wSource:t3","pane_id":"wSource:p2"}}),
            ),
            Ok(
                json!({"type":"agent_info", "agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}}),
            ),
        ] {
            let mut backend = LaunchMock {
                live: json!({"issue_type":"epic", "description":"Proposal"}),
                start_result: Some(started),
                ..Default::default()
            };
            let error = launch_with(
                &mut backend,
                &std::env::current_dir().unwrap(),
                "wSource",
                "epic-1",
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains("No planning prompt was sent"), "{error}");
            assert!(
                error.contains("wSource:t2") && error.contains("wSource:p2"),
                "{error}"
            );
            assert!(error.contains("startup input"), "{error}");
            assert_eq!(backend.calls.len(), 5);
        }
    }

    #[test]
    fn mismatched_prompt_identity_reports_uncertain_delivery() {
        let mut backend = LaunchMock {
            live: json!({"issue_type":"epic", "description":"Proposal"}),
            prompt_result: Some(
                json!({"type":"agent_prompted","agent":{"workspace_id":"wSource","tab_id":"wSource:t3","pane_id":"wSource:p2"}}),
            ),
            ..Default::default()
        };
        let error = launch_with(
            &mut backend,
            &std::env::current_dir().unwrap(),
            "wSource",
            "epic-1",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("Prompt may have been delivered"), "{error}");
        assert!(error.contains("wSource:t2"), "{error}");
        assert_eq!(backend.calls.len(), 6);
        assert_eq!(&backend.calls[5][..3], ["agent", "prompt", "wSource:p2"]);
    }

    #[test]
    fn finished_agent_closes_only_after_children_exist() {
        let session = Session {
            workspace_id: "wSource".into(),
            tab_id: "wSource:t2".into(),
            pane_id: "wSource:p2".into(),
        };
        let waited = json!({"type":"agent_info","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2","agent_status":"done"}});
        let tabs = json!({"type":"tab_list","tabs":[{"workspace_id":"wSource","tab_id":"wSource:t1"},{"workspace_id":"wSource","tab_id":"wSource:t2"}]});
        let closed = json!({"type":"ok"});
        let mut success = Mock::new(vec![waited.clone(), tabs.clone(), closed], vec!["epic-1.1"]);
        assert_eq!(
            finish_with(&mut success, &session, "epic-1").unwrap(),
            ["epic-1.1"]
        );
        assert_eq!(success.calls[1], ["tab", "list", "--workspace", "wSource"]);
        assert_eq!(success.calls[2], ["tab", "close", "wSource:t2"]);

        let mut empty = Mock::new(vec![waited.clone()], vec![]);
        assert!(finish_with(&mut empty, &session, "epic-1").is_err());
        assert_eq!(empty.calls.len(), 1);

        let blocked = json!({"type":"agent_info","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2","agent_status":"blocked"}});
        let mut blocked = Mock::new(vec![blocked], vec!["epic-1.1"]);
        assert!(finish_with(&mut blocked, &session, "epic-1").is_err());
        assert_eq!(blocked.calls.len(), 1);

        let mut last_tab = Mock::new(
            vec![
                waited,
                json!({"type":"tab_list","tabs":[{"workspace_id":"wSource","tab_id":"wSource:t2"}]}),
            ],
            vec!["epic-1.1"],
        );
        assert!(finish_with(&mut last_tab, &session, "epic-1").is_err());
        assert_eq!(last_tab.calls.len(), 2);

        let mut missing_tab = Mock::new(
            vec![
                json!({"type":"agent_info","agent":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2","agent_status":"done"}}),
                json!({"type":"tab_list","tabs":[{"workspace_id":"wSource","tab_id":"wSource:t1"}]}),
            ],
            vec!["epic-1.1"],
        );
        assert!(finish_with(&mut missing_tab, &session, "epic-1").is_err());
        assert_eq!(missing_tab.calls.len(), 2);
    }

    #[test]
    fn exited_agent_with_direct_children_closes_planning_tab() {
        let session = Session {
            workspace_id: "wSource".into(),
            tab_id: "wSource:t2".into(),
            pane_id: "wSource:p2".into(),
        };
        let empty_pane = json!({"type":"pane_info","pane":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}});
        let tabs = json!({"type":"tab_list","tabs":[{"workspace_id":"wSource","tab_id":"wSource:t1"},{"workspace_id":"wSource","tab_id":"wSource:t2"}]});
        let closed = json!({"type":"ok"});
        let mut backend = Mock::new(vec![empty_pane, tabs, closed], vec!["epic-1.1"]);
        backend
            .responses
            .push_front(Err(io::Error::other("agent exited")));

        assert_eq!(
            finish_with(&mut backend, &session, "epic-1").unwrap(),
            ["epic-1.1"]
        );
        assert_eq!(backend.calls[1], ["pane", "get", "wSource:p2"]);
        assert_eq!(backend.calls[2], ["tab", "list", "--workspace", "wSource"]);
        assert_eq!(backend.calls[3], ["tab", "close", "wSource:t2"]);
    }

    #[test]
    fn exited_agent_without_children_or_intact_empty_pane_stays_open() {
        let session = Session {
            workspace_id: "wSource".into(),
            tab_id: "wSource:t2".into(),
            pane_id: "wSource:p2".into(),
        };
        let empty_pane = json!({"type":"pane_info","pane":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2"}});
        let mut no_children = Mock::new(vec![empty_pane.clone()], vec![]);
        no_children
            .responses
            .push_front(Err(io::Error::other("agent exited")));
        assert!(finish_with(&mut no_children, &session, "epic-1").is_err());
        assert_eq!(no_children.calls.len(), 2);

        for pane in [
            json!({"type":"pane_info","pane":{"workspace_id":"wOther","tab_id":"wSource:t2","pane_id":"wSource:p2"}}),
            json!({"type":"pane_info","pane":{"workspace_id":"wSource","tab_id":"wSource:t3","pane_id":"wSource:p2"}}),
            json!({"type":"pane_info","pane":{"workspace_id":"wSource","tab_id":"wSource:t2","pane_id":"wSource:p2","agent":"codex","agent_status":"idle"}}),
        ] {
            let mut unsafe_pane = Mock::new(vec![pane], vec!["epic-1.1"]);
            unsafe_pane
                .responses
                .push_front(Err(io::Error::other("agent exited")));
            assert!(finish_with(&mut unsafe_pane, &session, "epic-1").is_err());
            assert_eq!(unsafe_pane.calls.len(), 2);
        }

        let mut missing_pane = Mock::new(vec![], vec!["epic-1.1"]);
        missing_pane
            .responses
            .push_back(Err(io::Error::other("agent exited")));
        missing_pane
            .responses
            .push_back(Err(io::Error::other("pane not found")));
        assert!(finish_with(&mut missing_pane, &session, "epic-1").is_err());
        assert_eq!(missing_pane.calls.len(), 2);
    }
}
