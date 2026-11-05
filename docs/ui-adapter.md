# Macross × OnTrack: local UI adapter v1

This fork preserves the terminal application and adds a local, versioned JSON
command interface. A GUI host starts the binary with an argv array, writes one
request to stdin, closes stdin and reads one response from stdout. No terminal,
clipboard, shell interpolation or network listener is required for this mode.

```sh
printf '%s' '{"version":1,"id":"hello","method":"capabilities","params":{}}' | ontrack api
```

Use `ontrack api --allow-writes` only when the host intentionally enables writes.
The host chooses the project through the process working directory. A request
cannot change repositories, environment variables, executable paths or shell commands.
The process inherits its normal user permissions and credentials. This is not a
sandbox or a multiuser authentication service. Do not expose it directly to a network.

## Request and response

A request has `version: 1`, a nonempty string `id`, a `method`, and optional
object `params`. Unknown fields, methods and parameter types are rejected.
The request limit is 65,536 UTF-8 bytes. The request ID is a correlation value,
not an idempotency key or a durable job ID.

Success: `{"version":1,"id":"hello","result":{...}}`, exit code 0.
Failure: `{"version":1,"id":"hello","error":{"code":"...","message":"..."}}`,
exit code 1. Invalid requests may have a null response ID. stderr is diagnostic
output; stdout contains only the JSON response. An interrupted mutation can have
an unknown outcome. Inspect the task or managed handoff before retrying. In
particular, do not blindly retry task creation or comment submission.

## Methods

| Method | Params | Result | Write |
| --- | --- | --- | --- |
| `capabilities` | none | Methods, transport and runtime limits | no |
| `board.list` | none | Entries with `record` and shared TUI `column` | no |
| `task.show` | `id` | Complete Beads record, including comments | no |
| `task.create` | `title`, `description`, optional `type`, `priority`, `harness` | Created task ID | yes |
| `task.comment` | `id`, `text` | Beads comment result | yes |
| `task.ready` | `id`, boolean `ready` | Requested Ready state after backend confirmation | yes |
| `worktrees.list` | none | Existing managed handoffs and status labels | no |
| `worktree.continue` | Managed handoff `id` | Accepted continuation message | yes |

Creation defaults to type `task`, priority 2 and no explicit harness. Titles are
limited to 256 bytes, descriptions to 32,768 bytes and comments to 16,384 bytes.
Existing Beads validation owns task IDs and supported types/harnesses. Text is
passed as separate subprocess arguments and is never evaluated as shell input.
The adapter's board read uses the read-only Beads route, not the browser route
that can repair closed-parent relationships.

Ready may cause the existing running TUI scheduler to dispatch an eligible task.
A successful Continue response means submission was accepted, not that work
finished. Existing native session, terminal, checkout and uncertain-delivery
checks still apply. `worktrees.list` status is separate from human acceptance.
Merge, cleanup, deletion and closure are deliberately not exposed by this adapter.

## Ownership and current limit

Beads remains the task store. The OnTrack registry remains the managed handoff
store. The adapter and TUI use the same domain functions and status projection.
There is no mirrored GUI database and no second scheduler.

**The automatic scheduler still runs in the TUI event loop.** Capabilities reports
`automatic_scheduler: "tui"`. This adapter is the first integration boundary, not
a headless daemon or a completed Macross integration. Keep the TUI running for
its automatic scheduling. A later change must move scheduling into one shared
runtime before a GUI can replace the TUI as the only frontend.

The fork is based on [ThomasJRyan/OnTrack](https://github.com/ThomasJRyan/OnTrack).
The existing update command still targets upstream releases. Do not use it to
maintain a custom fork binary until a fork release channel is defined.

## Checks

Integration tests invoke the real binary without a TTY for capabilities,
structured failures, input bounds and write admission. A disposable-project smoke
must also create a task, post a comment and read both through the API. Existing
TUI, worktree and Continue tests remain part of the full suite. A live terminal
smoke verifies the same built executable still renders the TUI.
