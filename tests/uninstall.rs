use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ontrack-uninstall-{}-{nonce}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("config/ontrack")).unwrap();
        fs::write(root.join("config/ontrack/settings.json"), b"{}").unwrap();
        fs::write(root.join("bin/bd"), b"keep").unwrap();
        fs::write(root.join("bin/herdr"), b"keep").unwrap();
        let staged = root.join("bin/ontrack-stage");
        fs::copy(env!("CARGO_BIN_EXE_ontrack"), &staged).unwrap();
        fs::rename(staged, root.join("bin/ontrack")).unwrap();
        Self(root)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.0.join("bin/ontrack"));
        command
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .env("XDG_CONFIG_HOME", self.0.join("config"));
        command
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn mxo_executable_can_be_uninstalled_with_existing_settings() {
    let fixture = Fixture::new();
    fs::rename(fixture.0.join("bin/ontrack"), fixture.0.join("bin/mxo")).unwrap();
    let output = Command::new(fixture.0.join("bin/mxo"))
        .args(["uninstall", "--yes"])
        .current_dir(&fixture.0)
        .env("HOME", &fixture.0)
        .env("XDG_CONFIG_HOME", fixture.0.join("config"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.0.join("bin/mxo").exists());
    assert!(!fixture.0.join("config/ontrack").exists());
}

#[test]
fn confirmed_uninstall_removes_only_ontrack_installation() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["uninstall", "--yes"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let message = String::from_utf8(output.stdout).unwrap();
    assert!(message.contains("Remove MXO executable:"));
    assert!(message.contains("Remove MXO settings:"));
    assert!(!fixture.0.join("bin/ontrack").exists());
    assert!(!fixture.0.join("config/ontrack").exists());
    for name in ["bd", "herdr"] {
        assert_eq!(fs::read(fixture.0.join("bin").join(name)).unwrap(), b"keep");
    }
}

#[test]
fn refusal_or_bad_arguments_leave_installation_intact() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .arg("uninstall")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Uninstall cancelled."));
    let output = fixture
        .command()
        .args(["uninstall", "--unknown"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(fixture.0.join("bin/ontrack").exists());
    assert!(fixture.0.join("config/ontrack/settings.json").exists());
}

#[test]
fn typed_confirmation_uninstalls() {
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .arg("uninstall")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"yes\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(!fixture.0.join("bin/ontrack").exists());
    assert!(!fixture.0.join("config/ontrack").exists());
}

#[test]
fn unsafe_settings_path_stops_before_removing_binary() {
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.0.join("config/ontrack")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(fixture.0.join("bin"), fixture.0.join("config/ontrack"))
            .unwrap();
        let output = fixture
            .command()
            .args(["uninstall", "--yes"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(fixture.0.join("bin/ontrack").exists());
    }
    assert!(Path::new(&fixture.0.join("bin/bd")).exists());
}

#[test]
fn missing_settings_directory_does_not_block_uninstall() {
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.0.join("config/ontrack")).unwrap();
    let output = fixture
        .command()
        .args(["uninstall", "--yes"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.0.join("bin/ontrack").exists());
}
