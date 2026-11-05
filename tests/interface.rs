use std::io::Write;
use std::process::{Command, Stdio};

fn call(request: &str, writes: bool) -> (bool, serde_json::Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ontrack"));
    command.arg("api");
    if writes {
        command.arg("--allow-writes");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let response = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "Non-JSON stdout: {} / {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), response)
}

#[test]
fn capabilities_work_without_a_tty_or_herdr() {
    let (success, response) = call(
        r#"{"version":1,"id":"hello","method":"capabilities","params":{}}"#,
        false,
    );
    assert!(success);
    assert_eq!(response["id"], "hello");
    assert_eq!(response["version"], 1);
    assert_eq!(response["result"]["automatic_scheduler"], "shared-runtime");
    assert_eq!(response["result"]["transport"], "local-json-command");
}

#[test]
fn malformed_unknown_and_oversized_requests_return_structured_errors() {
    for input in [
        "{".to_owned(),
        r#"{"version":2,"id":"v","method":"capabilities"}"#.to_owned(),
        r#"{"version":1,"id":"m","method":"shell.exec","params":{"command":"whoami"}}"#.to_owned(),
        " ".repeat(65537),
    ] {
        let (success, response) = call(&input, false);
        assert!(!success);
        assert_eq!(response["error"]["code"], "invalid_request");
    }
}

#[test]
fn read_only_mode_rejects_mutations_before_backend_access() {
    let (success, response) = call(
        r#"{"version":1,"id":"write","method":"task.create","params":{"title":"Do not create","description":"No backend access"}}"#,
        false,
    );
    assert!(!success);
    assert_eq!(response["id"], "write");
    assert_eq!(response["error"]["code"], "writes_disabled");
}

#[test]
fn unknown_fields_and_wrong_parameter_types_are_rejected() {
    for input in [
        r#"{"version":1,"id":"a","method":"capabilities","params":{"cwd":"/"}}"#,
        r#"{"version":1,"id":"b","method":"task.ready","params":{"id":"x-1","ready":"false"}}"#,
        r#"{"version":1,"id":"c","method":"task.show","params":{"id":"--help"}}"#,
    ] {
        let (success, response) = call(input, true);
        assert!(!success);
        assert_eq!(response["error"]["code"], "invalid_request");
    }
}
