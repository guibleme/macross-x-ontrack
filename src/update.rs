//! Explicit, verified replacement of the running MXO executable.
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const RELEASE_API: &str = "https://api.github.com/repos/guibleme/macross-x-ontrack/releases/latest";
const RELEASE_LATEST: &str = "https://github.com/guibleme/macross-x-ontrack/releases/latest";
const RELEASE_PREFIX: &str = "https://github.com/guibleme/macross-x-ontrack/releases/download/";
const RELEASE_PAGE_PREFIX: &str = "https://github.com/guibleme/macross-x-ontrack/releases/tag/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub page: String,
    pub asset_name: String,
    pub asset_url: String,
    pub checksum_url: String,
    pub digest: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Available(Available),
    Current,
    NoPublishedRelease,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn target() -> io::Result<&'static str> {
    if cfg!(all(target_os = "linux", not(target_env = "gnu"))) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Only glibc Linux release builds are available",
        ));
    }
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "No MXO release asset supports this operating system and architecture",
        )),
    }
}

pub fn parse_latest(body: &[u8], current: &str, target: &str) -> io::Result<Check> {
    let release: Value = serde_json::from_slice(body)?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| invalid("Release has no tag"))?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .map_err(|_| invalid("Release tag is not a semantic version"))?;
    if tag != format!("v{}.{}.{}", version.major, version.minor, version.patch) {
        return Err(invalid("Release tag must be vMAJOR.MINOR.PATCH"));
    }
    let installed = Version::parse(current.strip_prefix('v').unwrap_or(current))
        .map_err(|_| invalid("Running version is not a semantic version"))?;
    if release["draft"] == true || release["prerelease"] == true {
        return Err(invalid("Latest release is not stable"));
    }
    if version <= installed {
        return Ok(Check::Current);
    }
    let page = release["html_url"]
        .as_str()
        .ok_or_else(|| invalid("Release has no page"))?;
    if page != format!("https://github.com/guibleme/macross-x-ontrack/releases/tag/{tag}") {
        return Err(invalid("Release page is not the MXO repository"));
    }
    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| invalid("Release has no assets"))?;
    let names = [format!("mxo-{tag}-{target}.tar.gz")];
    for name in &names {
        let Some(asset) = assets.iter().find(|item| item["name"] == *name) else {
            continue;
        };
        let checksum_name = format!("{name}.sha256");
        let Some(checksum) = assets.iter().find(|item| item["name"] == checksum_name) else {
            continue;
        };
        let url = |item: &Value| -> io::Result<String> {
            let url = item["browser_download_url"]
                .as_str()
                .ok_or_else(|| invalid("Release asset has no download URL"))?;
            let expected = format!(
                "{RELEASE_PREFIX}{tag}/{}",
                item["name"]
                    .as_str()
                    .ok_or_else(|| invalid("Release asset has no name"))?
            );
            if url != expected {
                return Err(invalid(
                    "Release asset URL is not an official tagged download",
                ));
            }
            Ok(url.to_owned())
        };
        return Ok(Check::Available(Available {
            version: version.to_string(),
            page: page.to_owned(),
            asset_name: name.clone(),
            asset_url: url(asset)?,
            checksum_url: url(checksum)?,
            digest: asset["digest"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(|value| {
                    parse_digest(
                        value
                            .strip_prefix("sha256:")
                            .ok_or_else(|| invalid("GitHub asset digest is not SHA-256"))?,
                    )
                })
                .transpose()?,
        }));
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("Release {tag} has no {target} MXO archive and matching .sha256 asset"),
    ))
}

pub fn check() -> io::Result<Check> {
    let target = target()?;
    match fetch_small(RELEASE_API) {
        Ok(body) => parse_latest(&body, env!("CARGO_PKG_VERSION"), target),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Check::NoPublishedRelease),
        // GitHub's anonymous API quota is shared by public egress IPs. Resolve
        // the stable release through GitHub's redirect when that quota is spent.
        Err(_) => latest_from_redirect(env!("CARGO_PKG_VERSION"), target),
    }
}

fn latest_from_redirect(current: &str, target: &str) -> io::Result<Check> {
    let output = curl(RELEASE_LATEST)
        .args(["--output", "/dev/null", "--write-out", "%{url_effective}"])
        .output()
        .map_err(|error| io::Error::new(error.kind(), format!("Could not start curl: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() == Some(22) && stderr.contains("404") {
            return Ok(Check::NoPublishedRelease);
        }
        return Err(io::Error::other(format!(
            "Could not resolve the latest MXO release: {}",
            stderr.trim()
        )));
    }
    let page = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    parse_latest_page(&page, current, target)
}

fn parse_latest_page(page: &str, current: &str, target: &str) -> io::Result<Check> {
    let tag = page
        .strip_prefix(RELEASE_PAGE_PREFIX)
        .ok_or_else(|| invalid("Latest release redirect is not the MXO repository"))?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .map_err(|_| invalid("Latest release tag is not a semantic version"))?;
    if tag != format!("v{}.{}.{}", version.major, version.minor, version.patch) {
        return Err(invalid("Latest release tag must be vMAJOR.MINOR.PATCH"));
    }
    let installed = Version::parse(current.strip_prefix('v').unwrap_or(current))
        .map_err(|_| invalid("Running version is not a semantic version"))?;
    if version <= installed {
        return Ok(Check::Current);
    }
    let asset_name = format!("mxo-{tag}-{target}.tar.gz");
    Ok(Check::Available(Available {
        version: version.to_string(),
        page: page.to_owned(),
        asset_url: format!("{RELEASE_PREFIX}{tag}/{asset_name}"),
        checksum_url: format!("{RELEASE_PREFIX}{tag}/{asset_name}.sha256"),
        asset_name,
        digest: None,
    }))
}

fn curl(url: &str) -> Command {
    let mut cmd = Command::new("curl");
    cmd.args([
        "--fail",
        "--location",
        "--silent",
        "--show-error",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--connect-timeout",
        "10",
        "--max-time",
        "120",
        "--user-agent",
        "MXO-updater",
        "--header",
        "Accept: application/vnd.github+json",
    ])
    .arg(url);
    cmd
}

fn fetch_small(url: &str) -> io::Result<Vec<u8>> {
    let output = curl(url)
        .args(["--max-time", "15", "--max-filesize", "262144"])
        .output()
        .map_err(|e| io::Error::new(e.kind(), format!("Could not start curl: {e}")))?;
    if !output.status.success() {
        if url == RELEASE_API
            && output.status.code() == Some(22)
            && String::from_utf8_lossy(&output.stderr).contains("404")
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No published MXO release",
            ));
        }
        return Err(io::Error::other(format!(
            "Download failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    if output.stdout.len() > 2_000_000 {
        return Err(invalid("Release metadata or checksum is too large"));
    }
    Ok(output.stdout)
}

fn checksum_for(body: &[u8], name: &str) -> io::Result<[u8; 32]> {
    let text = std::str::from_utf8(body).map_err(|_| invalid("Checksum is not UTF-8"))?;
    let lines: Vec<_> = text
        .lines()
        .filter(|line| {
            let mut words = line.split_whitespace();
            let _ = words.next();
            words
                .next()
                .is_some_and(|file| file.trim_start_matches('*') == name)
        })
        .collect();
    if lines.len() != 1 {
        return Err(invalid(
            "Published checksum must contain exactly one entry for the selected asset",
        ));
    }
    let line = lines[0];
    let hex = line
        .split_whitespace()
        .next()
        .ok_or_else(|| invalid("Checksum is empty"))?;
    parse_digest(hex)
}

fn parse_digest(hex: &str) -> io::Result<[u8; 32]> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("Published SHA-256 checksum is malformed"));
    }
    let mut bytes = [0; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| invalid("Published SHA-256 checksum is malformed"))?;
    }
    Ok(bytes)
}

struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn stage_file(dir: &Path) -> io::Result<(Staged, File)> {
    for attempt in 0..16 {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = dir.join(format!(
            ".mxo-update-{}-{stamp}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((Staged(path), file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Could not reserve a staging file",
    ))
}

fn download_to(url: &str, file: File, cancel: &AtomicBool) -> io::Result<()> {
    let mut child = curl(url)
        .stdout(Stdio::from(file))
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("Could not start curl: {e}")))?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Update cancelled; existing executable is unchanged",
            ));
        }
        if child.try_wait()?.is_some() {
            let result = child.wait_with_output()?;
            return if result.status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "Download failed: {}",
                    String::from_utf8_lossy(&result.stderr).trim()
                )))
            };
        }
        thread::sleep(Duration::from_millis(75));
    }
}

fn verify(path: &Path, expected: &[u8; 32]) -> io::Result<()> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if digest.finalize().as_slice() != expected {
        return Err(invalid(
            "Downloaded asset does not match its published SHA-256 checksum",
        ));
    }
    Ok(())
}

fn extract(archive: &Path, output: File) -> io::Result<()> {
    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(archive)
        .output()
        .map_err(|error| io::Error::new(error.kind(), format!("Could not start tar: {error}")))?;
    if !listing.status.success() {
        return Err(invalid(
            "Verified release asset is not a readable tar.gz archive",
        ));
    }
    let names: Vec<_> = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|name| *name == "mxo" || *name == "./mxo")
        .map(str::to_owned)
        .collect();
    if names.len() != 1 {
        return Err(invalid(
            "Release archive must contain exactly one mxo executable at its root",
        ));
    }
    let result = Command::new("tar")
        .args(["-xOzf"])
        .arg(archive)
        .arg(&names[0])
        .stdout(Stdio::from(output))
        .output()?;
    if result.status.success() {
        Ok(())
    } else {
        Err(invalid(
            "Verified release archive does not contain an mxo executable",
        ))
    }
}

fn validate_binary(path: &Path) -> io::Result<()> {
    let mut magic = [0u8; 4];
    File::open(path)?
        .read_exact(&mut magic)
        .map_err(|_| invalid("Release archive contains a truncated executable"))?;
    let valid = if cfg!(target_os = "linux") {
        magic == [0x7f, b'E', b'L', b'F']
    } else if cfg!(target_os = "macos") {
        matches!(
            magic,
            [0xfe, 0xed, 0xfa, 0xce]
                | [0xce, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xcf, 0xfa, 0xed, 0xfe]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [0xbe, 0xba, 0xfe, 0xca]
        )
    } else {
        false
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "Release archive contains an invalid executable for this platform",
        ))
    }
}

fn replace(staged: &Path, executable: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(executable)?.permissions().mode() | 0o111;
        fs::set_permissions(staged, fs::Permissions::from_mode(mode))?;
    }
    File::open(staged)?.sync_all()?;
    fs::rename(staged, executable).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "Could not replace {}: {e}; existing executable is unchanged",
                executable.display()
            ),
        )
    })?;
    if let Some(parent) = executable.parent() {
        let _ = File::open(parent).and_then(|dir| dir.sync_all());
    }
    Ok(())
}

pub fn install(release: &Available, cancel: &AtomicBool) -> io::Result<()> {
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    if !fs::metadata(&executable)?.is_file() {
        return Err(invalid("Current executable is not a regular file"));
    }
    let dir = executable
        .parent()
        .ok_or_else(|| invalid("Current executable has no parent directory"))?;
    let checksum = checksum_for(&fetch_small(&release.checksum_url)?, &release.asset_name)?;
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Update cancelled; existing executable is unchanged",
        ));
    }
    let (download, file) = stage_file(dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Cannot stage update beside executable: {e}. Check directory permissions or install MXO in a user-owned location"),
        )
    })?;
    download_to(&release.asset_url, file, cancel)?;
    verify(&download.0, &checksum)?;
    if let Some(digest) = &release.digest {
        verify(&download.0, digest)?;
    }
    let (binary, output) = stage_file(dir)?;
    extract(&download.0, output)?;
    if fs::metadata(&binary.0)?.len() == 0 {
        return Err(invalid("Release executable is empty"));
    }
    validate_binary(&binary.0)?;
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Update cancelled; existing executable is unchanged",
        ));
    }
    replace(&binary.0, &executable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn fixture(tag: &str) -> Vec<u8> {
        let name = format!("mxo-{tag}-x86_64-unknown-linux-gnu.tar.gz");
        serde_json::to_vec(&json!({
            "tag_name":tag,
            "html_url":format!("https://github.com/guibleme/macross-x-ontrack/releases/tag/{tag}"),
            "assets":[
                {"name":name,"browser_download_url":format!("{RELEASE_PREFIX}{tag}/{name}"),"digest":format!("sha256:{}", "a".repeat(64))},
                {"name":format!("{name}.sha256"),"browser_download_url":format!("{RELEASE_PREFIX}{tag}/{name}.sha256")}
            ]
        })).unwrap()
    }
    #[test]
    fn selects_newer_compatible_release() {
        assert_eq!(
            parse_latest(&fixture("v1.2.0"), "1.2.0", "x86_64-unknown-linux-gnu").unwrap(),
            Check::Current
        );
        let found = parse_latest(&fixture("v1.2.1"), "1.2.0", "x86_64-unknown-linux-gnu").unwrap();
        let Check::Available(found) = found else {
            panic!("new version was not selected")
        };
        assert_eq!(found.digest, Some([0xaa; 32]));
        assert!(found.asset_name.contains("x86_64-unknown-linux-gnu"));
        assert_eq!(
            parse_latest(&fixture("v1.2.1"), "1.3.0", "x86_64-unknown-linux-gnu").unwrap(),
            Check::Current
        );
        assert!(parse_latest(&fixture("v1.2.1"), "1.2.0", "aarch64-unknown-linux-gnu").is_err());
    }

    #[test]
    fn selects_latest_release_from_github_redirect() {
        assert_eq!(
            parse_latest_page(
                "https://github.com/guibleme/macross-x-ontrack/releases/tag/v1.2.0",
                "1.2.0",
                "x86_64-apple-darwin"
            )
            .unwrap(),
            Check::Current
        );
        let Check::Available(found) = parse_latest_page(
            "https://github.com/guibleme/macross-x-ontrack/releases/tag/v1.2.1",
            "1.2.0",
            "aarch64-apple-darwin",
        )
        .unwrap() else {
            panic!("new version was not selected")
        };
        assert_eq!(found.asset_name, "mxo-v1.2.1-aarch64-apple-darwin.tar.gz");
        assert!(found.digest.is_none());
        assert!(
            parse_latest_page(
                "https://github.com/another/repo/releases/tag/v1.2.1",
                "1.2.0",
                "x86_64-apple-darwin"
            )
            .is_err()
        );
        assert!(
            parse_latest_page(
                "https://github.com/guibleme/macross-x-ontrack/releases/tag/v1.2.1-rc.1",
                "1.2.0",
                "x86_64-apple-darwin"
            )
            .is_err()
        );
    }
    #[test]
    fn checksum_rejects_mismatch_and_missing_name() {
        let name = "mxo-v1.2.1-x86_64-unknown-linux-gnu.tar.gz";
        let digest = Sha256::digest(b"safe bytes");
        let line = format!("{digest:x}  {name}\n");
        let expected = checksum_for(line.as_bytes(), name).unwrap();
        let dir = std::env::temp_dir();
        let (staged, mut file) = stage_file(&dir).unwrap();
        file.write_all(b"bad bytes").unwrap();
        assert!(verify(&staged.0, &expected).is_err());
        assert!(checksum_for(line.as_bytes(), "other").is_err());
        assert!(checksum_for(format!("{line}{line}").as_bytes(), name).is_err());
    }

    #[test]
    fn extracts_only_the_mxo_release_binary() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("mxo-update-extract-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        for (name, should_succeed) in [("mxo", true), ("ontrack", false)] {
            let member = dir.join(name);
            let archive = dir.join(format!("{name}.tar.gz"));
            fs::write(&member, b"release binary").unwrap();
            let output = Command::new("tar")
                .args(["-czf"])
                .arg(&archive)
                .args(["-C"])
                .arg(&dir)
                .arg(name)
                .output()
                .unwrap();
            assert!(output.status.success());

            let (staged, output) = stage_file(&dir).unwrap();
            let result = extract(&archive, output);
            assert_eq!(result.is_ok(), should_succeed);
            if should_succeed {
                assert_eq!(fs::read(&staged.0).unwrap(), b"release binary");
            }
        }

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_replacement_preserves_existing_file() {
        let dir = std::env::temp_dir();
        let (old, mut file) = stage_file(&dir).unwrap();
        file.write_all(b"old").unwrap();
        let (staged, mut candidate) = stage_file(&dir).unwrap();
        candidate.write_all(b"new").unwrap();
        assert!(replace(&staged.0, &dir.join("missing-parent").join("mxo")).is_err());
        assert_eq!(fs::read(&old.0).unwrap(), b"old");
    }

    #[test]
    fn successful_replacement_uses_staged_file() {
        let dir = std::env::temp_dir();
        let (old, mut existing) = stage_file(&dir).unwrap();
        existing.write_all(b"old").unwrap();
        let (new, mut candidate) = stage_file(&dir).unwrap();
        candidate.write_all(b"new").unwrap();
        replace(&new.0, &old.0).unwrap();
        assert_eq!(fs::read(&old.0).unwrap(), b"new");
        assert!(!new.0.exists());
    }
}
