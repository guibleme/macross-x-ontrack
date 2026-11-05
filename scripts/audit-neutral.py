#!/usr/bin/env python3
"""Audit source or release artifacts. Reports contain paths and categories, never matches."""
import argparse
import json
import re
import subprocess
import sys
import zipfile
from pathlib import Path

MAX_BYTES = 32 * 1024 * 1024
POLICY_FILES = {"scripts/audit-neutral.py", "tests/test_neutral_audit.py"}
RULES = {
    "organization": re.compile(r"\bcisco\b|ngdevx|miggbo|github\.cisco|\blaas\b|\bDEVX-\d+", re.I),
    "private-path": re.compile(r"/Users/[^\s/]+/|/home/[^\s/]+/|/nobackup/", re.I),
    "private-credential": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|\bgh[pousr]_[A-Za-z0-9_]{20,}|\bAKIA[A-Z0-9]{16}\b|\bsk-(?:proj-)?[A-Za-z0-9_-]{24,}"),
    "credential-url": re.compile(r"[a-z][a-z0-9+.-]*://[^\s/@:]+:[^\s/@]+@", re.I),
}


def inspect_bytes(label, value):
    if len(value) > MAX_BYTES:
        return [{"path": label, "category": "size-limit"}]
    text = value.decode("utf-8", errors="replace")
    return [{"path": label, "category": name} for name, pattern in RULES.items() if pattern.search(text)]


def inspect_file(root, path):
    label = path.relative_to(root).as_posix() if path.is_relative_to(root) else path.name
    if label in POLICY_FILES:
        return []
    if path.is_symlink():
        return [{"path": label, "category": "symlink-review"}]
    if path.stat().st_size > MAX_BYTES:
        return [{"path": label, "category": "size-limit"}]
    if zipfile.is_zipfile(path):
        findings = []
        total = 0
        with zipfile.ZipFile(path) as archive:
            for index, item in enumerate(archive.infolist()):
                total += item.file_size
                if index > 10000 or total > MAX_BYTES:
                    findings.append({"path": label, "category": "archive-limit"})
                    break
                if not item.is_dir():
                    findings.extend(inspect_bytes(f"{label}!{item.filename}", archive.read(item)))
        return findings
    return inspect_bytes(label, path.read_bytes())


def audit(root, paths=None):
    root = root.resolve()
    if paths:
        files = []
        for name in paths:
            path = (root / name).absolute()
            files.extend([p for p in path.rglob("*") if p.is_file() or p.is_symlink()] if path.is_dir() else [path])
    else:
        result = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=root,
                                check=True, capture_output=True)
        files = [root / name.decode() for name in result.stdout.split(b"\0") if name]
    findings = []
    checked = 0
    for path in sorted(set(files)):
        if not path.exists() and not path.is_symlink():
            continue
        checked += 1
        findings.extend(inspect_file(root, path))
    return {"version": 1, "checkedFiles": checked, "passed": not findings, "findings": findings,
            "limits": "Pattern checks do not establish source ownership or complete secret detection."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--path", action="append")
    args = parser.parse_args()
    result = audit(args.root, args.path)
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
