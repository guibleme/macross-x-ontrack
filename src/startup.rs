//! Launch-context checks and workspace setup, completed before raw mode.
use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use crossterm::style::Stylize;

pub fn ensure_herdr_workspace() -> io::Result<()> {
    require_herdr_workspace(std::env::var_os("HERDR_ENV").as_deref())
}

fn require_herdr_workspace(marker: Option<&OsStr>) -> io::Result<()> {
    if marker == Some(OsStr::new("1")) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "MXO must be launched inside a Herdr workspace. Open a Herdr workspace and run mxo from its shell.",
    ))
}

pub fn ensure_workspace() -> io::Result<bool> {
    let directory = std::env::current_dir()?;
    let result = setup(
        io::stdout().is_terminal(),
        &mut io::stdout().lock(),
        |args| {
            Command::new("bd")
                .args(args)
                .current_dir(&directory)
                .output()
        },
    );
    result
        .and_then(|workspace| {
            if let Some(directory) = workspace {
                ensure_screenshots_ignored(&directory)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "Beads startup in {}: {error}. Check bd and workspace permissions, then retry",
                    directory.display()
                ),
            )
        })
}

fn setup(
    highlight: bool,
    output: &mut impl Write,
    mut execute: impl FnMut(&[&str]) -> io::Result<Output>,
) -> io::Result<Option<PathBuf>> {
    let discovery = execute(&["--no-color", "where", "--json"])?;
    if let Some(directory) = workspace_path(&discovery)? {
        return Ok(Some(directory));
    }
    let command = "bd init";
    if highlight {
        writeln!(
            output,
            "No Beads workspace found. Run {} in your project, then launch MXO again.",
            command.bold()
        )?;
    } else {
        writeln!(
            output,
            "No Beads workspace found. Run {command} in your project, then launch MXO again."
        )?;
    }
    output.flush()?;
    Ok(None)
}

fn workspace_path(output: &Output) -> io::Result<Option<PathBuf>> {
    let value = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok();
    if output.status.success() {
        // Discovery belongs to bd, including ancestors, redirects and worktrees.
        if let Some(path) = value
            .as_ref()
            .and_then(|value| value["path"].as_str())
            .filter(|path| !path.trim().is_empty())
        {
            let path = PathBuf::from(path);
            if path.is_absolute() && path.file_name() == Some(OsStr::new(".beads")) {
                return Ok(Some(path));
            }
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bd where returned invalid workspace JSON; expected an absolute .beads path",
        ));
    }
    if value.as_ref().and_then(|value| value["error"].as_str()) == Some("no_beads_directory") {
        return Ok(None);
    }
    Err(io::Error::other(format!(
        "Could not discover Beads workspace: {} {}",
        String::from_utf8_lossy(&output.stderr).trim(),
        String::from_utf8_lossy(&output.stdout).trim()
    )))
}

fn ensure_screenshots_ignored(beads_directory: &Path) -> io::Result<()> {
    let ignore = beads_directory
        .parent()
        .expect("absolute .beads path")
        .join(".gitignore");
    let contents = match fs::read(&ignore) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    let rule = b".beads/screenshots/";
    if contents
        .split(|&byte| byte == b'\n')
        .any(|line| line.trim_ascii() == rule)
    {
        return Ok(());
    }
    let mut file = OpenOptions::new().create(true).append(true).open(ignore)?;
    if !contents.is_empty() && !contents.ends_with(b"\n") {
        file.write_all(b"\n")?;
    }
    file.write_all(rule)?;
    file.write_all(b"\n")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn herdr_workspace_requires_the_managed_pane_marker() {
        assert!(require_herdr_workspace(Some(OsStr::new("1"))).is_ok());

        for marker in [
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("0")),
            Some(OsStr::new("true")),
        ] {
            let error = require_herdr_workspace(marker).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert!(error.to_string().contains("Open a Herdr workspace"));
        }
    }

    fn response(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn missing_workspace_prints_guidance_and_exits_without_other_commands() {
        for highlight in [false, true] {
            let mut calls = 0;
            let mut transcript = Vec::new();
            assert!(
                setup(highlight, &mut transcript, |args| {
                    calls += 1;
                    assert_eq!(args, &["--no-color", "where", "--json"]);
                    Ok(response(1, r#"{"error":"no_beads_directory"}"#, ""))
                })
                .unwrap()
                .is_none()
            );
            assert_eq!(calls, 1);
            let transcript = String::from_utf8(transcript).unwrap();
            assert!(transcript.contains("No Beads workspace found. Run "));
            assert!(transcript.contains("bd init"));
            assert!(transcript.ends_with("then launch MXO again.\n"));
            if highlight {
                assert!(transcript.contains("\x1b[1mbd init\x1b[0m"));
            } else {
                assert!(!transcript.contains('\x1b'));
            }
        }
    }

    #[test]
    fn existing_inherited_or_redirected_workspace_returns_its_beads_path() {
        for path in [
            "/project/.beads",
            "/ancestor/.beads",
            "/linked-worktree/.beads",
        ] {
            let mut calls = 0;
            let mut transcript = Vec::new();
            assert_eq!(
                setup(false, &mut transcript, |args| {
                    calls += 1;
                    assert_eq!(args, &["--no-color", "where", "--json"]);
                    Ok(response(
                        0,
                        &serde_json::json!({"path": path}).to_string(),
                        "",
                    ))
                })
                .unwrap(),
                Some(PathBuf::from(path))
            );
            assert_eq!(calls, 1);
            assert!(transcript.is_empty());
        }
    }

    #[test]
    fn malformed_successful_discovery_is_an_error() {
        for stdout in [
            "",
            "bad json",
            "{}",
            r#"{"path":" "}"#,
            r#"{"path":"relative/.beads"}"#,
            r#"{"path":"/project/other"}"#,
        ] {
            let mut transcript = Vec::new();
            assert_eq!(
                setup(false, &mut transcript, |_| Ok(response(0, stdout, "")))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
            assert!(transcript.is_empty());
        }
    }

    #[test]
    fn discovery_errors_do_not_print_initialization_guidance() {
        for (stdout, stderr) in [
            ("", "permission denied"),
            ("broken json", "bad redirect"),
            (r#"{"error":"database_error"}"#, "Dolt unavailable"),
        ] {
            let mut calls = 0;
            let mut transcript = Vec::new();
            let error = setup(true, &mut transcript, |_| {
                calls += 1;
                Ok(response(1, stdout, stderr))
            })
            .unwrap_err();
            assert_eq!(calls, 1);
            assert!(error.to_string().contains(stderr));
            assert!(transcript.is_empty());
        }
    }

    #[test]
    fn unavailable_cli_is_an_error() {
        let mut transcript = Vec::new();
        let error = setup(false, &mut transcript, |_| {
            Err(io::Error::new(io::ErrorKind::NotFound, "bd unavailable"))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(transcript.is_empty());
    }

    #[test]
    fn screenshot_ignore_is_created_once_and_preserves_existing_content() {
        let root = std::env::temp_dir().join(format!(
            "ontrack-ignore-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let beads = root.join(".beads");
        let ignore = root.join(".gitignore");

        ensure_screenshots_ignored(&beads).unwrap();
        assert_eq!(
            fs::read_to_string(&ignore).unwrap(),
            ".beads/screenshots/\n"
        );
        ensure_screenshots_ignored(&beads).unwrap();
        assert_eq!(
            fs::read_to_string(&ignore).unwrap(),
            ".beads/screenshots/\n"
        );

        fs::write(&ignore, "target/\n# keep this").unwrap();
        ensure_screenshots_ignored(&beads).unwrap();
        assert_eq!(
            fs::read_to_string(&ignore).unwrap(),
            "target/\n# keep this\n.beads/screenshots/\n"
        );
        ensure_screenshots_ignored(&beads).unwrap();
        assert_eq!(
            fs::read_to_string(&ignore).unwrap(),
            "target/\n# keep this\n.beads/screenshots/\n"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
