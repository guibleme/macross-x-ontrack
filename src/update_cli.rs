//! Explicit terminal command for checking and installing an MXO release.
use crate::update::{self, Available, Check};
use std::{
    io::{self, BufRead, Write},
    sync::atomic::AtomicBool,
};

pub fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    run_with(&mut stdin.lock(), &mut stdout, update::check, |release| {
        update::install(release, &AtomicBool::new(false))
    })
}

fn run_with(
    input: &mut impl BufRead,
    output: &mut impl Write,
    check: impl FnOnce() -> io::Result<Check>,
    install: impl FnOnce(&Available) -> io::Result<()>,
) -> io::Result<()> {
    writeln!(
        output,
        "Checking for MXO updates (running v{})…",
        env!("CARGO_PKG_VERSION")
    )?;
    match check()? {
        Check::Current => writeln!(output, "MXO is up to date."),
        Check::NoPublishedRelease => writeln!(output, "No public MXO release is available."),
        Check::Available(release) => {
            writeln!(
                output,
                "MXO v{} is available.\n{}",
                release.version, release.page
            )?;
            write!(output, "Install this verified release? [y/N] ")?;
            output.flush()?;
            let mut answer = String::new();
            input.read_line(&mut answer)?;
            if !answer.trim().eq_ignore_ascii_case("y")
                && !answer.trim().eq_ignore_ascii_case("yes")
            {
                return writeln!(
                    output,
                    "Update cancelled; existing executable is unchanged."
                );
            }
            writeln!(
                output,
                "Downloading and verifying MXO v{}…",
                release.version
            )?;
            install(&release)?;
            writeln!(output, "MXO updated. Restart any running MXO session.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, io::Cursor};

    fn available() -> Check {
        Check::Available(Available {
            version: "1.2.0".into(),
            page: "https://github.com/guibleme/macross-x-ontrack/releases/tag/v1.2.0".into(),
            asset_name: "asset.tar.gz".into(),
            asset_url: "https://github.com/guibleme/macross-x-ontrack/releases/download/v1.2.0/asset.tar.gz".into(),
            checksum_url: "https://github.com/guibleme/macross-x-ontrack/releases/download/v1.2.0/asset.tar.gz.sha256".into(),
            digest: None,
        })
    }

    #[test]
    fn reports_current_and_missing_release_without_installing() {
        for (status, expected) in [
            (Check::Current, "MXO is up to date"),
            (Check::NoPublishedRelease, "No public MXO release"),
        ] {
            let mut output = Vec::new();
            run_with(
                &mut Cursor::new(""),
                &mut output,
                || Ok(status),
                |_| panic!("installed"),
            )
            .unwrap();
            assert!(String::from_utf8(output).unwrap().contains(expected));
        }
    }

    #[test]
    fn installation_requires_explicit_yes() {
        for answer in ["", "n\n", "yesterday\n"] {
            let mut output = Vec::new();
            run_with(
                &mut Cursor::new(answer),
                &mut output,
                || Ok(available()),
                |_| panic!("installed without confirmation"),
            )
            .unwrap();
            assert!(
                String::from_utf8(output)
                    .unwrap()
                    .contains("Update cancelled")
            );
        }
        let installed = Cell::new(false);
        let mut output = Vec::new();
        run_with(
            &mut Cursor::new("yes\n"),
            &mut output,
            || Ok(available()),
            |release| {
                assert_eq!(release.version, "1.2.0");
                installed.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert!(installed.get());
        assert!(String::from_utf8(output).unwrap().contains("MXO updated"));
    }

    #[test]
    fn check_and_install_errors_are_returned() {
        let mut output = Vec::new();
        assert!(
            run_with(
                &mut Cursor::new(""),
                &mut output,
                || Err(io::Error::other("offline")),
                |_| panic!("installed")
            )
            .unwrap_err()
            .to_string()
            .contains("offline")
        );
        assert!(
            run_with(
                &mut Cursor::new("y\n"),
                &mut output,
                || Ok(available()),
                |_| Err(io::Error::other("checksum mismatch"))
            )
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch")
        );
    }
}
