#!/usr/bin/env python3
"""Acquire the pinned audit CLI from its official release and verify its checksum."""
import hashlib
import io
import platform
import sys
import tarfile
import urllib.request
from pathlib import Path

VERSION = "8.30.1"

def main():
    destination = Path(sys.argv[1] if len(sys.argv) > 1 else "state/tools/gitleaks")
    system = {"Darwin": "darwin", "Linux": "linux"}.get(platform.system())
    if system is None:
        raise SystemExit("Install Gitleaks from the official release for this platform.")
    machine = platform.machine().lower()
    arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x64", "amd64": "x64"}.get(machine)
    if arch is None:
        raise SystemExit("Unsupported scanner architecture.")
    name = f"gitleaks_{VERSION}_{system}_{arch}.tar.gz"
    base = f"https://github.com/gitleaks/gitleaks/releases/download/v{VERSION}/"
    checks = urllib.request.urlopen(base + f"gitleaks_{VERSION}_checksums.txt", timeout=30).read().decode()
    expected = next(line.split()[0] for line in checks.splitlines() if line.endswith(name))
    data = urllib.request.urlopen(base + name, timeout=30).read()
    if hashlib.sha256(data).hexdigest() != expected:
        raise SystemExit("Audit scanner checksum mismatch.")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        member = archive.getmember("gitleaks")
        if not member.isfile():
            raise SystemExit("Invalid audit scanner archive.")
        destination.write_bytes(archive.extractfile(member).read())
    destination.chmod(0o700)
    print(f"Gitleaks {VERSION}: checksum verified.")

if __name__ == "__main__":
    main()
