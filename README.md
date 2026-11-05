# Macross × OnTrack

A local workfront application and shared execution runtime, based on
[ThomasJRyan/OnTrack](https://github.com/ThomasJRyan/OnTrack).

Macross keeps conversations, evidence, decisions and versioned plans together.
A plan contains multiple waves. Explicit admission links each wave to one Bead.
The original terminal interface and the graphical application use the same runtime.

## Build and open

Use macOS or Linux, Rust 1.88+, Node 24.5+, and npm. Install
[Beads](https://beads.gascity.com) and [Herdr](https://herdr.dev) separately.
The selected agent executable must also be available to the Herdr environment.

From this checkout:

```sh
cargo build
export MACROSS_ONTRACK_BIN="$PWD/target/debug/ontrack"
export MACROSS_STATE_ROOT="$PWD/state"
npm --prefix ui ci
npm --prefix ui run build
npm --prefix ui start
```

Open [Macross](http://127.0.0.1:4280). For the optional Electron window, use another terminal
with the same environment and run `npm --prefix ui run desktop`.

In Settings, register a project repository and its permitted document folders.
Add and enable an OnTrack connection, select it as the project's executor, and start its runtime.
Enable an Agent sessions connection for conversations. Local documents use a separate connection.
A repository needs an initialized Beads workspace before task execution.

The application starts empty. It does not import existing application state or organization configuration.

## Work and acceptance

Choose an available agent explicitly. Detection does not prove live compatibility with every agent or operation.
Create a front before creating a Bead or worktree. Approve a plan, then dispatch individual eligible waves.
Review the exact result with criteria evidence. Delivery requires current human acceptance and verified integration.
Closure is a separate explicit action. Automatic merge, closure and cleanup are disabled in this fork's runtime.

Closing a window does not stop the runtime or its agents. A lost command response requires reconciliation, not blind replay.

## Terminal and integration

Run the built `ontrack` executable inside a Herdr project workspace for the terminal interface.
It connects to the same repository runtime as Macross. Run `ontrack serve` for a foreground owner.
Stop that owner with Ctrl+C or an operating-system signal. There is no `runtime.stop` API.

See the [setup, ownership and limits](docs/pivot-v1.md),
[local JSON interface](docs/ui-adapter.md), and [provenance record](docs/provenance.json).
Trello is a future official connector. Third-party plugin installation and autonomous delivery are outside v1.

## Updates

Build this fork from its checkout. The inherited installer and `ontrack update` target upstream OnTrack releases.
Using them can replace this fork's executable. A fork release channel and installable desktop bundle are not configured.

## Checks

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build
npm --prefix ui test
npm --prefix ui run build
```

Use the verification procedures in [pivot v1](docs/pivot-v1.md) before treating a build as accepted delivery.
