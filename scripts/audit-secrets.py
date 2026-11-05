#!/usr/bin/env python3
"""Run the dedicated secret scanner on selected source files or a built artifact."""
import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gitleaks", default="gitleaks")
    parser.add_argument("--path", action="append")
    args = parser.parse_args()
    root = Path.cwd()
    with tempfile.TemporaryDirectory(prefix="macross-source-scan-") as directory:
        staging = Path(directory) / "source"
        staging.mkdir()
        if args.path:
            for name in args.path:
                item = root / name
                if item.is_dir():
                    shutil.copytree(item, staging / item.name, symlinks=True)
                else:
                    shutil.copy2(item, staging / item.name, follow_symlinks=False)
        else:
            output = subprocess.check_output(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"])
            for name in output.split(b"\0"):
                if not name:
                    continue
                relative = Path(name.decode())
                source = root / relative
                if not source.is_file() or source.is_symlink():
                    continue
                destination = staging / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
        report = Path(directory) / "report.json"
        result = subprocess.run([args.gitleaks, "dir", str(staging), "--no-banner", "--redact=100",
                                 "--report-format", "json", "--report-path", str(report)],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if result.returncode not in (0, 1):
            print(json.dumps({"passed": False, "error": "secret_scanner_failed"}))
            return 2
        findings = json.loads(report.read_text()) if report.exists() else []
        print(json.dumps({"passed": result.returncode == 0, "scanner": "gitleaks",
                          "findings": [{"path": item.get("File"), "rule": item.get("RuleID")} for item in findings]}, indent=2))
        return result.returncode


if __name__ == "__main__":
    sys.exit(main())
