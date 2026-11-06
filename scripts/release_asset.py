"""Build and verify the public release asset contract."""

import argparse
import hashlib
from pathlib import Path
import re
import tarfile
import tomllib


TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)
ROOT = Path(__file__).resolve().parents[1]


def validate_tag(tag: str) -> None:
    if re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag) is None:
        raise ValueError(f"invalid release tag: {tag!r}; expected vMAJOR.MINOR.PATCH")
    with (ROOT / "Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]
    if tag != f"v{version}":
        raise ValueError(f"tag {tag!r} does not match Cargo.toml version {version!r}")


def asset_name(tag: str, target: str) -> str:
    if target not in TARGETS:
        raise ValueError(f"unsupported target: {target!r}")
    return f"mxo-{tag}-{target}.tar.gz"


def checksum(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package(tag: str, target: str, binary: Path, output: Path) -> None:
    validate_tag(tag)
    name = asset_name(tag, target)
    if not binary.is_file() or not binary.stat().st_mode & 0o111:
        raise ValueError(f"release binary is missing or not executable: {binary}")
    output.mkdir(parents=True, exist_ok=True)
    archive = output / name
    with tarfile.open(archive, "w:gz") as bundle:
        bundle.add(binary, arcname="mxo", recursive=False)
    (output / f"{name}.sha256").write_text(f"{checksum(archive)}  {name}\n")


def verify(tag: str, output: Path) -> None:
    validate_tag(tag)
    expected = {asset_name(tag, target) for target in TARGETS}
    expected |= {f"{name}.sha256" for name in expected}
    actual = {path.name for path in output.iterdir()} if output.is_dir() else set()
    if actual != expected:
        raise ValueError(f"release assets differ: missing={sorted(expected - actual)}, extra={sorted(actual - expected)}")
    for target in TARGETS:
        name = asset_name(tag, target)
        recorded = (output / f"{name}.sha256").read_text()
        if recorded != f"{checksum(output / name)}  {name}\n":
            raise ValueError(f"checksum mismatch: {name}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    tag_parser = commands.add_parser("validate-tag")
    tag_parser.add_argument("tag")
    package_parser = commands.add_parser("package")
    package_parser.add_argument("tag")
    package_parser.add_argument("target")
    package_parser.add_argument("binary", type=Path)
    package_parser.add_argument("output", type=Path)
    verify_parser = commands.add_parser("verify")
    verify_parser.add_argument("tag")
    verify_parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.command == "validate-tag":
        validate_tag(args.tag)
    elif args.command == "package":
        package(args.tag, args.target, args.binary, args.output)
    else:
        verify(args.tag, args.output)


if __name__ == "__main__":
    main()
