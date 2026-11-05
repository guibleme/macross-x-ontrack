# Pivot v1 verification

The implementation is a local review candidate on `feature/pivot-v1`, based on `ea8d43500540ddbca238a5afaf87353d7c30a1a6`.

## Completed checks

- Full Rust run: 469 unit tests and 22 integration tests passed. Later owner, queue and session parser changes also passed targeted regressions. All 12 process tests also passed with the normal macOS temporary-directory environment after the fixture socket path was shortened.
- Rust formatting, Clippy with warnings denied, and build passed.
- All 45 Node, connector and sidecar tests passed. TypeScript and the production UI build passed.
- The installer test passed after its Linux libc mock was corrected for execution on macOS. The original fixture also failed on the unchanged adapter branch.
- Neutral-source checks and Gitleaks passed for selected source and built UI assets. The dependency audit reported zero known vulnerabilities.
- Electron opened the application and reused the matching sidecar. Closing the window preserved the sidecar.
- The TUI connected to the existing owner, opened, quit, and restored its terminal flags and alternate screen.

## Live disposable pilot

A workfront started an OpenCode conversation before any task or worktree existed. Its answer and a continuation were preserved in the same native session. Two test providers separately verified agent substitution without changing the front identity.

One approved wave created one Bead, `pivot-2vm`. The shared owner dispatched it into a sibling worktree. After explicit correction of disposable Git setup, the same Bead continued. The agent committed only the requested probe file at `220489ae4d8687f94ff2a54119a6901da4dd9169` and marked the task ready for review.

The final engine observation reported `succeeded`, review `pending`, integration `not_integrated`, and `closed: false`. No human acceptance was fabricated. No merge, closure or cleanup occurred.

Backup and isolated restore preserved all three pilot fronts, plan and task bindings, native session references, conversation entries, and the immutable document digest. Runtime and application restarts preserved those identities.

## Review boundaries

OpenCode was verified live. Other providers passed contract tests with simulated adapters. This does not establish compatibility with every installed provider.

No historical or personal project data was migrated. The next rollout requires a human-selected personal front and explicit acceptance of the result. Implementation publication requires separate, explicit human authorization. Test fixture commits remain isolated in the disposable repository.

Audits cover selected source and production UI assets. No distributable desktop installer was produced. Debug executables and runtime evidence remain local. Scanner results do not establish legal redistribution authority.

Reproducible commands and recovery limits are in [pivot-v1.md](pivot-v1.md) and [ui-adapter.md](ui-adapter.md). Detailed local receipts are under ignored `state/evidence/`.
