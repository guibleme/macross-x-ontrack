//! Exercise the installed command dispatch without network or workspace setup.
#[cfg(unix)]
#[test]
fn update_runs_outside_herdr_and_beads_without_opening_the_browser() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    let directory = std::env::temp_dir().join(format!(
        "ontrack-update-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let curl = directory.join("curl");
    fs::write(
        &curl,
        format!(
            "#!/bin/sh\nprintf '%s' '{{\"tag_name\":\"v{}\"}}'\n",
            env!("CARGO_PKG_VERSION")
        ),
    )
    .unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_ontrack"))
        .arg("update")
        .current_dir(&directory)
        .env("PATH", &directory)
        .env_remove("HERDR_ENV")
        .output()
        .unwrap();
    fs::remove_dir_all(&directory).unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("MXO is up to date."), "{stdout}");
    assert!(!stdout.contains("No Beads workspace"));
    assert!(!stdout.contains("\u{1b}[?1049h"));
}
