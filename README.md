# Macross × OnTrack

UI-adapter fork of [ThomasJRyan/OnTrack](https://github.com/ThomasJRyan/OnTrack). The original TUI remains available. See the [local JSON adapter](docs/ui-adapter.md) for integration from Macross or another local interface.

## Upstream OnTrack

**Turn your backlog into agent work, without leaving the terminal.**

OnTrack brings [Beads](https://beads.gascity.com) and [Herdr](https://herdr.dev) together in one keyboard-driven workspace. See what needs doing, give it to a coding agent, and follow the work through to integration.

> **Application screenshot coming soon.**
<!-- Replace the placeholder above with a screenshot of the Kanban board and detail drawer. -->

## From idea to implementation

Browse six Kanban columns: **Open, Ready, Blocked, In Progress, Ready To Merge, Closed**. Open a Bead's right-side detail drawer for Markdown descriptions, comments, and dependency charts. Create Tasks, Epics, and children; paste local PNG or JPEG screenshots with `Ctrl+V`.

Mark tasks **Ready** and let OnTrack dispatch eligible work to Herdr agents in separate Git worktrees, with a concurrency limit you control. Need to start something immediately? Hand the selected Bead to an agent with `h`.

Agents commit and check their work, then signal **Ready To Merge**, not Closed. OnTrack closes a tracked Bead only after verifying integration into its recorded target. The worktree drawer preserves workspace focus, confirmed merge and cleanup, continue (`a`), force removal (`Shift+X`, discards uncommitted files), and restart (`Shift+R`) actions. Automatic merging and cleanup are optional; verified integration still closes the Bead when both are disabled.

The board refreshes automatically, reflows into 3×2 and 2×3 grids on narrower terminals, and scrolls each column vertically. Children integrate into their parent branch before closing. Settings include [40 themes](docs/settings.md), a default agent, and a concurrent-agent limit. Run `ontrack update` to install a verified compatible release.

## Get started

OnTrack requires both commands on your `PATH`:

- **Beads (`bd`)** for issue tracking — [install Beads](https://beads.gascity.com/getting-started/installation).
- **Herdr (`herdr`)** for the shell and agent workspaces — [install Herdr](https://herdr.dev/docs/install/).

For macOS or Ubuntu 24.04-compatible Linux, with Bash, `curl`, and Python 3:

```sh
curl -fsSLo install-ontrack.sh https://raw.githubusercontent.com/ThomasJRyan/OnTrack/main/scripts/install.sh && bash install-ontrack.sh
```

The installer uses a compatible stable release and offers to install missing Beads and Herdr commands. Add `~/.local/bin` to your `PATH` if needed, open your project in a **Herdr shell**, then run:

```sh
ontrack
```

Use an interactive terminal with alternate-screen support. If the project has no Beads workspace yet, run `bd init` first. Building from this checkout instead? With Rust 1.88+ installed, run `cargo run` inside Herdr.

See [installation and updates](docs/installation.md) for supported architectures, prerequisites, updates, and removal. To remove the installed executable and OnTrack user settings, run `ontrack uninstall` and confirm the printed paths; Beads and Herdr remain installed.

## Keys to keep moving

| Key | Action |
| --- | --- |
| `↑` / `↓`; `←` / `→` | Select a card; switch columns, including empty ones |
| `Shift+←` / `Shift+→`, `m` | Move a card to an adjacent column or choose a destination |
| `Enter` | Open Bead details; arrows / Page keys scroll the drawer |
| `n` / `Shift+N`, `e` | Create a Bead / child task, or edit the selection |
| `h` | Hand the selected Bead to a Herdr agent |
| `w` | Open worktree drawer; `Enter` focuses the selected Herdr workspace |
| `s`, `Shift+?` | Open settings or context-specific hotkey help |
| `Esc`, `q` / `Ctrl+C` | Go back; quit when no dialog is open |

All six columns retain empty counts. Moving to Ready schedules eligible agents; other moves do not start or stop them, and individual moves never cascade to children. Closed moves and `c` require fresh tracked integration proof; `d` permanently deletes after confirmation. See the [user guide](docs/user-guide.md), [settings](docs/settings.md), [development guide](docs/development.md), [CI checks](docs/ci.md), and [release guide](docs/releases.md).
