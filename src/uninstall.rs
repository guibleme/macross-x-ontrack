//! Remove this user's MXO installation without opening a Beads workspace.
use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

struct Targets {
    executable: PathBuf,
    settings_dir: PathBuf,
}

impl Targets {
    fn current() -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let settings_dir = crate::settings::config_path()?
            .parent()
            .ok_or_else(|| invalid("MXO settings path has no parent"))?
            .to_path_buf();
        let targets = Self {
            executable,
            settings_dir,
        };
        targets.check()?;
        Ok(targets)
    }

    fn check(&self) -> io::Result<()> {
        if self
            .executable
            .file_name()
            .is_none_or(|name| name != "ontrack" && name != "mxo")
        {
            return Err(invalid(
                "Refusing to remove an executable not named mxo or ontrack",
            ));
        }
        let executable = fs::symlink_metadata(&self.executable)?;
        if !executable.file_type().is_file() {
            return Err(invalid(
                "Refusing to remove a symlink or non-file executable",
            ));
        }
        if self
            .settings_dir
            .file_name()
            .is_none_or(|name| name != "ontrack")
            || self
                .settings_dir
                .parent()
                .is_none_or(|base| !base.is_absolute() || base == Path::new("/"))
        {
            return Err(invalid("Refusing an unsafe MXO settings path"));
        }
        match fs::symlink_metadata(&self.settings_dir) {
            Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
            Ok(_) => Err(invalid(
                "Refusing to remove a symlink or non-directory MXO settings path",
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn remove(&self, original_executable: &fs::Metadata) -> io::Result<()> {
        // Recheck after confirmation, and remove the executable last so a
        // settings failure leaves the command available for a retry.
        self.check()?;
        self.check_executable(original_executable)?;
        match fs::remove_dir_all(&self.settings_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "Could not remove MXO settings at {}: {error}",
                        self.settings_dir.display()
                    ),
                ));
            }
        }
        // A concurrent replacement must not be removed silently.
        self.check_executable(original_executable)?;
        fs::remove_file(&self.executable).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "Could not remove MXO executable at {}: {error}",
                    self.executable.display()
                ),
            )
        })
    }

    fn check_executable(&self, original: &fs::Metadata) -> io::Result<()> {
        let metadata = fs::symlink_metadata(&self.executable)?;
        #[cfg(unix)]
        let same_file = {
            use std::os::unix::fs::MetadataExt;
            metadata.dev() == original.dev() && metadata.ino() == original.ino()
        };
        #[cfg(not(unix))]
        let same_file =
            metadata.len() == original.len() && metadata.modified()? == original.modified()?;
        if metadata.file_type().is_file() && same_file {
            Ok(())
        } else {
            Err(invalid(
                "Refusing to remove an executable changed since confirmation",
            ))
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

pub fn run(yes: bool) -> io::Result<()> {
    let targets = Targets::current()?;
    let original_executable = fs::symlink_metadata(&targets.executable)?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "Remove MXO executable: {}",
        targets.executable.display()
    )?;
    writeln!(
        output,
        "Remove MXO settings: {}",
        targets.settings_dir.display()
    )?;
    if !yes {
        write!(output, "Type yes to uninstall: ")?;
        output.flush()?;
        let mut response = String::new();
        io::stdin().lock().read_line(&mut response)?;
        if response.trim() != "yes" {
            writeln!(output, "Uninstall cancelled.")?;
            return Ok(());
        }
    }
    targets.remove(&original_executable)?;
    writeln!(output, "MXO uninstalled. Beads and Herdr were not changed.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_wrong_executable_and_settings_locations() {
        let root =
            std::env::temp_dir().join(format!("ontrack-uninstall-paths-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let binary = root.join("ontrack");
        fs::write(&binary, b"test").unwrap();
        assert!(
            Targets {
                executable: root.join("bd"),
                settings_dir: root.join("ontrack")
            }
            .check()
            .is_err()
        );
        assert!(
            Targets {
                executable: binary.clone(),
                settings_dir: root.clone()
            }
            .check()
            .is_err()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = root.join("config/ontrack");
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            symlink(&root, &link).unwrap();
            assert!(
                Targets {
                    executable: binary,
                    settings_dir: link
                }
                .check()
                .is_err()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
