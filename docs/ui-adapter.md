# Local JSON interface v1

The terminal interface and Macross share one repository runtime. The `ontrack api`
command is a client. It reads one bounded JSON request from stdin and writes one response to stdout.
Commands use argument arrays. No shell interpolation, clipboard or terminal is needed for API calls.

## Runtime ownership

Run `ontrack serve` from the registered Git repository to start a foreground runtime.
The TUI and Macross can also start or connect to the owner. One lifetime lock identifies the owner per canonical Git common directory.
Clients verify repository identity, protocol and runtime epoch through a private Unix socket.
Closing a client does not stop that owner. Stop a foreground owner with Ctrl+C or use an operating-system signal.
No `runtime.stop` API exists. Stopping the owner does not imply that Herdr agents stopped.

Except for static `capabilities`, API calls require the owner. They return
`runtime_unavailable` when it cannot be reached. There is no direct mutation fallback.
Reconnecting clients read current snapshots. They must not replay uncertain writes.

The host selects the repository through the child process working directory.
Requests cannot select executable paths, environment variables or arbitrary shell commands.
The process has the local user's permissions. This is not a sandbox or a multiuser service.
Do not expose the command interface directly to a network.

## Envelope and operation identity

```json
{"version":1,"id":"read-capabilities","method":"capabilities","params":{}}
```

A request uses protocol version 1, a nonempty ID of at most 128 bytes, a method and object parameters.
The stdin request limit is 65,536 UTF-8 bytes. Unsupported methods, fields and parameter types are rejected.
Use `ontrack api --allow-writes` only for deliberately authorized mutations.

Success returns `{"version":1,"id":"read-capabilities","result":{...}}` and exit code 0.
Failure returns `{"version":1,"id":"read-capabilities","error":{"code":"...","message":"..."}}` and exit code 1.
Malformed input can return a null ID. stderr contains diagnostics. stdout contains only the response.

For mutations, the request ID identifies a durable operation. The runtime records its intent before external effects.
Repeating the same ID and payload returns the recorded result. Reusing that ID with different content fails.
A lost response or uncertain backend error leaves the operation unknown. Neither a timeout nor an error proves no effect occurred.

## Methods

| Method | Parameters | Result | Writes |
| --- | --- | --- | --- |
| `capabilities` | none | Methods, protocol, transport and runtime limits | no |
| `runtime.status` | none | Owner status and repository identity | no |
| `agents.list` | none | Detected agents, availability, capabilities and verification status | no |
| `board.list` | none | Beads records, board columns and record revisions | no |
| `task.show` | `id` | Beads record with `ontrackRevision` | no |
| `task.create` | `title`, `description`; optional `type`, `priority`, `harness`, `externalRef` | Created Bead ID | yes |
| `task.comment` | `id`, `text`; optional `expectedRevision` | Beads comment result | yes |
| `task.ready` | `id`, boolean `ready`; optional `expectedRevision` | Confirmed requested Ready state | yes |
| `worktrees.list` | none | Managed attempts, active identity, native state, result revision and review | no |
| `worktree.continue` | Managed attempt `id`; optional `expectedRevision` | Accepted continuation message | yes |
| `worktree.integrate` | Managed handoff `id`, exact `resultRevision` | Explicit integration request after current acceptance | yes |
| `session.start` | `agentId`, `frontId`, `input`; optional `context`, `workspaceId` | Provider session ID and request status | yes |
| `session.send` | `sessionId`, `input`; optional `context` | Submission status | yes |
| `session.observe` | `sessionId`; optional `cursor` | Pending status or bounded answer text | no |
| `operation.get` | Operation `id` | Stored request, state and confirmed result when available | no |
| `operation.reconcile` | Operation `id`, `actor:"human"`, nonempty `evidence`, confirmed `result` | Reconciled operation | yes |
| `review.accept` | Managed attempt `id`, `resultRevision`, nonempty `evidence` | Human review receipt | yes |
| `review.changes_requested` | Managed attempt `id`, `resultRevision`, `reason` or nonempty `evidence` | Human review receipt | yes |
| `delivery.close` | Bead `id` | Explicit closure result after acceptance and integration checks | yes |

Creation defaults to task type, priority 2 and no explicit harness. A missing harness does not authorize a vendor fallback.
Select the agent explicitly for Macross admission and conversation requests.
Titles allow 256 bytes, descriptions 32,768 bytes and comments 16,384 bytes.
`externalRef` identifies the operation target. It does not establish a native Beads external-reference index.
`expectedRevision` compares the current target revision before a supported mutation.

`task.ready` can trigger the shared scheduler. Successful submission or continuation does not mean execution completed.
Native session, checkout and uncertain-delivery checks remain in force.

## Review and state

Planning, execution, review, integration and command outcome are different dimensions.
`worktrees.list` preserves native details and supplies stable state codes.
An `active:false` attempt is historical. A current result must have one unambiguous active attempt.

Review returns its historical decision plus `validity:"current"` or `validity:"unknown"` when applicable.
A changed or dirty result invalidates acceptance. Missing observations do not establish current acceptance.
Delivery, dependency release and closure require current human acceptance and verified integration of the accepted result.
Acceptance alone does not initiate merge or cleanup.

## Recovery

Use `operation.get` with the original mutation ID after a lost response.
An absent operation returns `operation_not_found`. This differs from an unavailable runtime or an unknown effect.

The current `operation.reconcile` implementation resolves only uncertain `task.create` operations through explicit human evidence and Beads readback.
Provide `result:{"id":"the-created-bead"}`. The runtime verifies the recorded title and description against that Bead.
It does not accept arbitrary no-effect assertions or generic resolutions for every mutation.
Other uncertain operations remain blocked until their outcome can be established through a supported recovery path.
Never recreate a task merely because its response was lost.

Macross also records its own command intents and bindings. It can recover a confirmed provider session link or a confirmed wave admission.
It retains unknown outcomes when the engine cannot confirm them.

## Updates and limits

The runtime is local and uses Unix sockets. Installed agent detection is not a live compatibility certification.
Conversation restrictions in prompts are instructions. They are not a universal operating-system sandbox for every agent.
The inherited updater still targets upstream releases and can replace this fork's binary. Build this fork from its checkout instead.
