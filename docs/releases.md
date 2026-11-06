# Releases

MXO publishes versioned binaries from the [release workflow](../.github/workflows/release.yml). A pushed tag in the form `vMAJOR.MINOR.PATCH` must exactly match the `version` in `Cargo.toml`. Version `0.2.3` uses tag `v0.2.3`. A pushed tag starts the workflow; no GitHub Release is published until its checks and all builds pass. Users can follow the [installation guide](installation.md) for installation, updates, removal, and troubleshooting.

## Supported downloads

| System | CPU | Rust target |
| --- | --- | --- |
| Linux (Ubuntu 24.04 compatible) | x86-64 | `x86_64-unknown-linux-gnu` |
| Linux (Ubuntu 24.04 compatible) | ARM64 | `aarch64-unknown-linux-gnu` |
| macOS | Intel | `x86_64-apple-darwin` |
| macOS | Apple Silicon | `aarch64-apple-darwin` |

For each target, a release contains `mxo-vVERSION-TARGET.tar.gz` and a matching `mxo-vVERSION-TARGET.tar.gz.sha256`. The archive contains one executable named `mxo` at its root. The checksum file uses the standard `SHA256  filename` format and hashes the downloadable archive, not the unpacked binary. The `vVERSION` portion must match the release tag exactly. These names are the contract for the installer and `mxo update` command. Four targets produce eight downloadable files in total.

Download the archive and its `.sha256` file from the same [GitHub Release](https://github.com/guibleme/macross-x-ontrack/releases). In the download directory, verify before unpacking:

```sh
# Linux
sha256sum -c mxo-vVERSION-TARGET.tar.gz.sha256

# macOS
shasum -a 256 -c mxo-vVERSION-TARGET.tar.gz.sha256

tar -xzf mxo-vVERSION-TARGET.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 755 mxo "$HOME/.local/bin/mxo"
```

Replace `VERSION` and `TARGET` with the downloaded release values, and ensure `$HOME/.local/bin` is on your `PATH`. MXO also needs the [`bd` command](https://beads.gascity.com) to open a workspace. Its agent handoff features need a working [Herdr installation](https://herdr.dev/docs).

## Maintainer process

Update `Cargo.toml` and `Cargo.lock`, run the four Cargo checks in the [CI guide](ci.md), commit the version change, and confirm CI passes on the intended commit. Create the tag on that same commit, replacing the example version below with the package version:

```sh
git tag v0.2.3
git push origin v0.2.3
```

The [CI workflow](../.github/workflows/ci.yml) runs on pull requests and pushes to `main`. Its reported result is distinct from branch protection, which maintainers must configure separately if they want CI to block merges. The tagged release workflow repeats the Rust checks, packaging tests, and installer tests, so a passing tag is required even if the branch's earlier CI run passed.

The release workflow first verifies the tag and runs formatting, tests, Clippy, a normal build, release packaging tests, and the offline installer tests. Linux runners build both Linux targets. A macOS Intel runner builds both macOS targets, including an Apple Silicon cross-build. Each job uploads its archive and checksum. The publish job downloads all four sets, requires the exact expected filenames, recalculates every SHA-256 digest, and creates a GitHub Release with all eight assets. A failed check, build, upload, or verification prevents the publish job from running. GitHub CLI creates a draft while uploading assets and publishes it only after uploads succeed.

For local release checks, run:

```sh
python3 -m unittest discover -s scripts -p 'test_release_asset.py'
python3 scripts/release_asset.py validate-tag v0.2.3
bash tests/installer.sh
```

Use the current package version for the second command. GitHub Actions is the authority for actual native platform builds and publishing; local checks cannot validate those remote steps. After pushing, inspect the workflow run and the [release page](https://github.com/guibleme/macross-x-ontrack/releases) for all eight files. If a check or build fails, fix the cause in a new commit, rerun checks, and use a new version tag. Do not move a published tag: users and checksums may already refer to its original contents. A missing or mismatched checksum is a release defect; users should never bypass verification to install it.
