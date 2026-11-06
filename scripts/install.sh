#!/usr/bin/env bash
# Install the latest stable MXO release, optionally installing bd and Herdr.
set -euo pipefail

latest_release_page='https://github.com/guibleme/macross-x-ontrack/releases/latest'
release_download_base='https://github.com/guibleme/macross-x-ontrack/releases/download'
install_dir="${HOME:-}/.local/bin"
replace=false
scratch=''
staged=''

usage() {
  cat <<'EOF'
Usage: bash install.sh [--install-dir DIR] [--replace]

Install the latest stable MXO release into ~/.local/bin by default.
An existing destination requires --replace. If bd or Herdr is missing, the
installer offers to run that project's official installer.

Options:
  --install-dir DIR  Install mxo into DIR
  --replace          Replace an existing regular mxo binary (update)
  -h, --help         Show this help
EOF
}

fail() { printf 'mxo installer: %s\n' "$*" >&2; exit 1; }
log() { printf 'mxo installer: %s\n' "$*" >&2; }
cleanup() {
  if [[ -n "$staged" ]]; then rm -f -- "$staged"; fi
  if [[ -n "$scratch" ]]; then rm -rf -- "$scratch"; fi
}
trap cleanup EXIT
[[ -n "${HOME:-}" ]] || fail 'HOME must be set to choose the default install directory'

while (($#)); do
  case "$1" in
    --install-dir)
      (($# >= 2)) || fail '--install-dir needs a directory'
      [[ -n "$2" ]] || fail '--install-dir needs a nonempty directory'
      [[ "$2" == /* ]] || fail '--install-dir must be an absolute path'
      install_dir=$2
      shift 2
      ;;
    --replace) replace=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown option: $1 (see --help)" ;;
  esac
done

case "$(uname -s)" in
  Linux) os=unknown-linux-gnu ;;
  Darwin) os=apple-darwin ;;
  *) fail 'unsupported operating system; supported: Linux and macOS' ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) fail 'unsupported CPU architecture; supported: x86_64 and aarch64' ;;
esac
if [[ "$os" == unknown-linux-gnu ]] && ! getconf GNU_LIBC_VERSION >/dev/null 2>&1; then
  fail 'this Linux build requires glibc; musl and other libc variants are unsupported'
fi
target="${arch}-${os}"

command -v bash >/dev/null 2>&1 || fail 'Bash is required'
command -v curl >/dev/null 2>&1 || fail 'curl is required'
command -v python3 >/dev/null 2>&1 || fail 'Python 3 is required for release and checksum verification'

confirm_dependency_install() {
  local name=$1 answer
  if [[ ! -r /dev/tty || ! -w /dev/tty ]]; then
    log "no terminal is available to ask; install $name manually or rerun this installer in a terminal"
    return 1
  fi
  printf 'Install %s now using its official installer? [y/N] ' "$name" >/dev/tty
  if ! IFS= read -r answer </dev/tty; then answer=''; fi
  [[ "$answer" =~ ^([Yy]|[Yy][Ee][Ss])$ ]]
}

ensure_dependency() {
  local binary=$1 name=$2 install_command=$3
  if command -v "$binary" >/dev/null 2>&1; then return 0; fi

  log "$name is not installed or is not on PATH"
  if ! confirm_dependency_install "$name"; then
    log "skipping $name; install it manually with: $install_command"
    return 1
  fi

  case "$binary" in
    bd)
      log 'running the official Beads installer'
      if ! curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash; then
        fail 'the official Beads installer failed'
      fi
      ;;
    herdr)
      log 'running the official Herdr installer'
      if ! curl -fsSL https://herdr.dev/install.sh | sh; then
        fail 'the official Herdr installer failed'
      fi
      ;;
  esac

  if [[ -n "${HOME:-}" && -d "$HOME/.local/bin" ]]; then
    PATH="$HOME/.local/bin:$PATH"
    export PATH
  fi
  hash -r
  if ! command -v "$binary" >/dev/null 2>&1; then
    fail "$name installer finished, but $binary is still not on PATH; restart your shell or add its install directory to PATH"
  fi
  log "$name is installed and available"
  return 0
}

dependencies_skipped=false
if ! ensure_dependency bd Beads 'curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash'; then
  dependencies_skipped=true
fi
if ! ensure_dependency herdr Herdr 'curl -fsSL https://herdr.dev/install.sh | sh'; then
  dependencies_skipped=true
fi
if [[ "$dependencies_skipped" == true ]]; then
  fail 'install the missing dependencies and ensure both bd and herdr are on PATH, then rerun this installer'
fi

destination="${install_dir%/}/mxo"
[[ "${install_dir//\//}" != '' ]] || fail 'the root directory is not a supported install location'
if [[ -L "$destination" ]]; then
  fail "refusing to replace symlink: $destination"
fi
if [[ -e "$destination" ]]; then
  [[ -f "$destination" ]] || fail "destination is not a regular file: $destination"
  [[ "$replace" == true ]] || fail "destination exists: $destination (pass --replace to update it)"
fi

scratch=$(mktemp -d) || fail 'could not create a temporary directory'
download() {
  local url=$1 output=$2 label=$3
  if ! curl --fail --location --silent --show-error --retry 3 --connect-timeout 10 --max-time 120 \
    --output "$output" "$url"; then
    fail "could not download $label from $url; check the network and release availability"
  fi
}

log "checking latest stable release for $target"
if ! latest_release_url=$(curl --fail --location --silent --show-error --retry 3 --connect-timeout 10 --max-time 120 \
  --output /dev/null --write-out '%{url_effective}' "$latest_release_page"); then
  fail "could not resolve the latest stable release from $latest_release_page; check network access and release availability"
fi
if [[ "$latest_release_url" =~ ^https://github\.com/guibleme/macross-x-ontrack/releases/tag/(v[0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
  release_tag=${BASH_REMATCH[1]}
else
  fail "GitHub returned an unexpected latest release URL: $latest_release_url"
fi
asset_name="mxo-${release_tag}-${target}.tar.gz"
asset_url="${release_download_base}/${release_tag}/${asset_name}"
checksum_url="${asset_url}.sha256"
expected_digest=''
download "$asset_url" "$scratch/asset" "$asset_name"
download "$checksum_url" "$scratch/checksum" 'release checksum'

if ! python3 - "$scratch/asset" "$scratch/checksum" "$scratch/mxo" "$asset_name" "$expected_digest" <<'PY'
import hashlib
import re
import shutil
import sys
import tarfile

source, checksum_path, output, name, api_digest = sys.argv[1:]
try:
    digest = hashlib.sha256()
    with open(source, "rb") as binary:
        for chunk in iter(lambda: binary.read(1024 * 1024), b""):
            digest.update(chunk)
    actual = digest.hexdigest()
    if api_digest and actual != api_digest.split(":", 1)[1].lower():
        raise ValueError(f"SHA-256 mismatch for {name} (GitHub asset digest)")
    with open(checksum_path, encoding="utf-8") as file:
        lines = file.read().splitlines()
    values = []
    for line in lines:
        match = re.fullmatch(r"([0-9a-fA-F]{64})\s+\*?(.+)", line.strip())
        if match and match[2] == name:
            values.append(match[1].lower())
    if len(values) != 1 or values[0] != actual:
        raise ValueError(f"SHA-256 mismatch or missing checksum entry for {name}")

    with tarfile.open(source, "r:gz") as archive:
        members = [member for member in archive.getmembers()
                   if member.isfile() and member.name in ("mxo", "./mxo")]
        if len(members) != 1:
            raise ValueError("archive must contain one regular mxo binary at its root")
        with archive.extractfile(members[0]) as binary, open(output, "wb") as result:
            shutil.copyfileobj(binary, result)
    with open(output, "rb") as result:
        if not result.read(1):
            raise ValueError("archive contains an empty mxo binary")
except (OSError, ValueError, tarfile.TarError) as error:
    print(f"mxo installer: verification or extraction failed: {error}", file=sys.stderr)
    sys.exit(1)
PY
then
  fail 'downloaded release was not installed'
fi

if ! mkdir -p -- "$install_dir"; then fail "cannot create install directory: $install_dir"; fi
staged=$(mktemp "$install_dir/.mxo.XXXXXXXX") || fail "cannot write to install directory: $install_dir"
if ! cp -- "$scratch/mxo" "$staged" || ! chmod 755 "$staged"; then
  fail "cannot prepare executable in install directory: $install_dir"
fi
# Recheck after downloading so a newly created file is never silently replaced.
if [[ -L "$destination" ]] || { [[ -e "$destination" ]] && [[ "$replace" != true ]]; }; then
  fail "destination changed during download: $destination"
fi
if ! mv -f -- "$staged" "$destination"; then fail "cannot install to: $destination"; fi
staged=''
log "installed $destination"
if [[ ":$PATH:" != *":$install_dir:"* ]]; then
  log "add $install_dir to PATH to run mxo by name"
fi
