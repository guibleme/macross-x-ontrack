# Macross × OnTrack pivot v1

## Ownership and provenance

The desktop is a new implementation of the approved workfront workflow.
No previous application source, assets, history, deployment settings or runtime data are imported.
[provenance.json](provenance.json) records the baseline, exclusions and dependency basis.
It is technical evidence, not a legal certification.

Macross owns workfronts, conversation history, document references, plan revisions and wave definitions.
A wave may exist before a task. Explicit admission binds one Bead. Later attempts retain that binding.
Beads owns task records. OnTrack owns run journals, integration facts and result acceptance.
Herdr owns terminal sessions and agent observations. Connector caches are not another task store.

## Build and setup

Use macOS or Linux, Rust 1.88+, Node 24.5+ and npm.
The application uses built-in `node:sqlite`, without an ORM.
Install Beads and Herdr separately. Initialize Beads in a disposable project before the first execution test.

From this fork's checkout:

```sh
cargo build
export MACROSS_ONTRACK_BIN="$PWD/target/debug/ontrack"
export MACROSS_STATE_ROOT="$PWD/state"
npm --prefix ui ci
npm --prefix ui run build
npm --prefix ui start
```

Open [the local application](http://127.0.0.1:4280).
Run `npm --prefix ui run desktop` for the optional Electron window.
Keep the same environment and state root when sharing a server between browser and Electron.
The desktop rejects another service or state directory on its port.

| Host environment variable | Purpose |
| --- | --- |
| `MACROSS_ONTRACK_BIN` | Host-selected fork executable; an absolute path avoids another installed OnTrack binary |
| `MACROSS_STATE_ROOT` | Application database, artifacts, backups and connection configuration |
| `MACROSS_PORT` | Loopback HTTP port, default 4280 |
| `ONTRACK_RUNTIME_DIR` | Optional short private socket directory for the native runtime |

Executable paths are host settings. The browser cannot submit arbitrary commands or select a different executable.
The adapter appends the local user's `.local/bin` directory to its existing PATH.
Herdr must also run in an environment that can find the selected agent executable and required project tools.
Restarting only the browser cannot repair another process's missing environment.

In Settings:

1. Add a project with its repository and permitted document folders.
2. Add and enable an OnTrack connection for that project.
3. Select that connection as the project executor.
4. Start the shared runtime explicitly.
5. Add an Agent sessions connection for front conversations.
6. Add a Local documents connection for permitted file imports.
7. Discover agents and select one explicitly when starting work.

Testing a connection performs bounded reads. It does not start the runtime, create tasks or authorize execution.
Enabling a connector starts observation eligibility, not agent dispatch.

## States and command recovery

[The state contract](../contracts/pivot-v1.json) declares dimensions and invariants.
Planning, dependencies, execution, review, integration and command outcome remain separate.
Owning services enforce transitions. The UI displays server-provided actions and rejection reasons.

Approving a plan does not dispatch it. A completed agent does not imply acceptance.
A review binds an exact result revision and criteria evidence.
Changed or dirty results invalidate acceptance. Unavailable observations preserve historical evidence without proving its current validity.
Delivery and dependent-wave release require current human acceptance and verified integration.
Historical attempts cannot provide acceptance for a new active attempt.

An unadmitted wave can be withdrawn. Withdrawal does not delete a native task or cancel running work.
Merge, acceptance, closure and cleanup remain separate human actions.
Automatic merge, child merge, closure and cleanup are disabled in this fork's runtime.

Commands record intent before external effects. IDs bind their content and target.
Repeated identical IDs return existing operations. Conflicting reuse fails.
A lost response produces an unknown outcome when the effect cannot be confirmed.
SQLite transactions do not provide exactly-once external execution.

The native runtime supports explicit human reconciliation of uncertain task creation using Beads readback.
It does not support arbitrary resolution of every unknown operation.
Macross can reconstruct a confirmed session link or continue admission on an already confirmed Bead.
It must never create a second Bead to recover a lost response.
See [the API recovery contract](ui-adapter.md#recovery).

## Runtime, agents and connectors

One lifetime lock selects the runtime owner for each canonical repository.
The owner uses a private Unix socket. TUI and GUI connect to that owner.
The API never falls back to direct writes when the owner is unavailable.
Closing a window detaches a client. It does not stop the runtime or its agents.

Use `ontrack serve` from the project repository for a foreground owner.
Stop that owner with Ctrl+C or an operating-system signal.
There is no `runtime.stop` endpoint. Stopping the owner does not guarantee that Herdr sessions stopped.
After restart, the owner preserves uncertain operations and reconciles known state before new work.

A conversation can start before a Bead or worktree exists.
Agent IDs and provider session references are separate from the front identity.
Changing agents starts another session with selected canonical context. It does not transfer the native session.
The runtime reports detected agents and capabilities. Detection does not establish live compatibility with every installed agent.
No provider is selected silently. Unsupported operations remain unavailable.
Conversation restrictions are prompt instructions, not a universal operating-system sandbox.

The [version 1 connector contract](../contracts/connectors-v1.json) defines transport-independent operations for sources, executors, sessions and intelligence.
Descriptors declare protocol version 1 and their implemented operation sets. Incompatible versions are rejected before activation.
Interrupt and native resume remain optional. An absent intelligence provider is unavailable, with no implicit model fallback.
Bundled connector definitions declare capabilities. Instances belong to projects.
Configuration and health are separate. Disabling a connector blocks new Macross operations and rejects stale configuration generations.
It preserves evidence and accepted external work. Disconnecting does not cancel an agent.
Credentials belong in the host credential store. Configuration must contain references only, never credential values.

Trello remains a future official connector.
Third-party executable installation, remote multiuser access and autonomous acceptance or release are outside v1.
The development server uses port 4281. Production assets use the local server on port 4280.

## Backup and restore

The state root contains `macross.sqlite`, immutable artifact blobs and revisioned connector configuration.
The database contains Macross-owned records and operation receipts. It does not mirror native task or run state.
Backup uses SQLite's online backup operation and copies the referenced document blobs and configuration.
Do not copy an active database file alone.

Create a backup through the application. Backups stay under the configured state root.
Restore from the host command line into a new destination:

```sh
node ui/server/restore.mjs "$MACROSS_STATE_ROOT/backups/manual" "$PWD/restored-state"
```

Replace `manual` with the backup name. The destination must not exist.
Restore checks document digests and preserves the original application state.
It cannot restore native Git, Beads or Herdr state. Those sources remain separate authorities.

## Verification and distribution

Run Rust formatting, tests, strict Clippy and build. Run the UI tests and production build.

```sh
python3 -m unittest discover -s tests -p 'test_neutral_audit.py'
python3 scripts/audit-neutral.py
python3 scripts/audit-neutral.py --path ui/dist
```

Use a dedicated secret scanner with redacted output in addition to the neutral audit.
Neither lexical scanning nor synthetic fixtures prove redistribution authority or live agent compatibility.

Use a disposable repository to test owner exclusion, command recovery, current-result acceptance and manual closure.
Verify connector disable, client reconnection and backup restoration.
Record live-agent checks separately from simulated provider tests.
[project-plan.json](project-plan.json) maps the eight approved waves and their dependencies.

Build this fork from its checkout. The inherited installer and update command target upstream releases.
They can replace fork behavior. No fork release channel or installable desktop bundle is configured.
Original repositories and worktrees remain separate. Review readiness does not authorize publication, merge or deployment.
