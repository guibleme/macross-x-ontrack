# Development guide

MXO is a single Rust 2024 binary requiring Rust 1.88 or newer. Runtime workspace integration uses the public Beads CLI; no direct database integration is needed. Browse operations are read-only. Create, edit, delete, and guarded board moves are user actions; the scheduler hands off eligible Ready issues through the tracked Herdr path. Bead closure requires freshly verified tracked Git integration and is reconciled independently of automatic merge/cleanup preferences. Consult the [official Beads documentation](https://beads.gascity.com) for CLI and workspace behavior and the [user guide](user-guide.md) for the board workflow.

Tagged builds and release asset naming are documented in the [release guide](releases.md); installer behavior and offline checks are in the [installation guide](installation.md#release-asset-contract-for-maintainers). The workflow uses `scripts/release_asset.py` for tag validation, archive creation, and checksum verification; keep its target list aligned with the native runner matrix in `.github/workflows/release.yml`.

## Modules

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Dispatches `update` and `uninstall` before checking the Herdr launch context and Beads workspace; initializes, runs, and restores the browser terminal. |
| `src/startup.rs` | Requires the Herdr pane marker, discovers the active workspace through Beads, ensures local screenshots are Git-ignored, and prints manual initialization guidance when no workspace exists. |
| `src/uninstall.rs` | Validates and removes the running executable and user settings after explicit CLI confirmation, before Beads discovery or terminal setup. |
| `src/settings.rs` | Loads and atomically saves theme, default-agent, agent-limit, animation, and worktree-cleanup preferences under the user's XDG config directory. |
| `src/theme.rs` | Defines the bundled theme catalog, stable IDs, display names, and semantic color palettes. |
| `src/beads.rs` | Builds issue CLI commands, validates JSON, preserves returned records, and exposes guarded per-Bead board moves. |
| `src/screenshots.rs` | Reads clipboard images, writes local screenshots into the shared Beads workspace, and parses description references for agent prompts. |
| `src/herdr.rs` | Discovers integrations, maps/focuses Herdr workspaces, creates tracked worktrees, and starts/prompts agents to signal Ready To Merge after checks and commits, never close before merge. |
| `src/planning.rs` and `src/epic.rs` | Retain legacy agent-planning code with no browser launch control. |
| `src/worktrees.rs` | Journals handoffs and merge lifecycle, verifies Git/Herdr evidence, closes integrated Beads, and guards cleanup. |
| `src/app.rs` | Holds board column/card selection, per-column scrolling, drawer/navigation state, forms, handoff toasts, cached data, errors, and hit tests; handles input and background worker requests/responses. |
| `src/event.rs` | Polls crossterm events with a timeout and recognizes quit keys. |
| `src/update.rs` | Reads the official GitHub release, selects a compatible asset, verifies SHA-256, and atomically replaces the executable. |
| `src/update_cli.rs` | Runs the explicit update command, reports release status, and confirms installation. |
| `src/hotkeys.rs` | Maps the active board, drawer, or dialog to its keyboard commands and recognizes modified help-key reports. |
| `src/ui.rs` | Renders splash, six-column board, right-side detail/worktree drawers, dialogs, and handoff toasts; maintains visibility and scroll bounds. |
| `src/detail.rs` | Presents complete issue records with Markdown rendering for narrative fields and readable structured fallbacks for metadata. |
| `src/terminal.rs` | Manages raw mode, alternate screen, mouse capture, and cleanup, including initialization failures and panic handling. |

The browser does not dispatch agent planning. Its `p` and `g` keys have no planning action, and the create form always uses ordinary Beads creation, including for Epics. Legacy planning handlers remain in source but have no UI entry point. Keep those controls absent from the keyboard reference, footer, and forms.

The board has exactly six status columns: Open, Ready, Blocked, In Progress, Ready To Merge, Closed. Keep headings and counts even when empty. Do not add Deferred/custom columns or collapse state: legacy `deferred` and unknown statuses map to Open for display only, with raw status retained in details and no bulk data rewrite. Left / Right changes columns including empty ones; Up / Down and the mouse wheel move one card within the focused column without wrapping. Shift+Left / Shift+Right moves the selected Bead adjacent; board `m` chooses a destination. Navigation, hit testing, and rendering must share card geometry. Cards use a five-row height and the available viewport reserves the heading and both column borders; never draw or register a partial card over a border. The renderer chooses a row-major 6×1 grid at board widths >=144 cells, 3×2 at 54–143, and 2×3 below 54, requiring 24 cells per column before using a single row while preserving the 54-cell narrow-layout breakpoint. Divide available width and height evenly and give remainder cells to the first columns/rows. Each column retains its own vertical viewport offset. Reconcile the focused selection against the current card capacity on every draw, including after shrinking and restoring tiny viewports. Rows below three cells skip content; rows below eight cells cannot fit a complete card. Resize guidance accounts for all grid rows plus the header/footer. Crossing a viewport edge adjusts only enough to keep selection visible, not back to the top. Empty-column focus must not retain an actionable Bead ID.

Keep the board's bottom border row outside the content viewport: headings, empty/resize messages, and cards must never paint over the status-colored rounded bottom edge. The compact-height path must preserve each grid cell and rebuild hit rectangles. `kanban_outer_bottom_borders_keep_status_colors_and_cards_stay_inside` checks empty and populated boards, compact and exact-fit heights, narrow layouts, and all bundled themes using the rendered buffer.

Kanban cards use `palette.panel` for their normal fill and border background, matching the enclosing columns. Preserve the selected interior's `selected_background` and `selected_foreground` and the issue-type border foreground.

Enter opens or closes the selected Bead's right-side detail drawer, restoring the same board selection; `w` opens the right-side worktree drawer with all-handoffs recovery access, where Enter focuses the selected workspace. Esc closes the drawer and restores board selection. Detail arrows and Page keys scroll rather than moving board selection. Keep create/edit, handoff, settings, and help actions and the worktree focus/merge/close-workspace/remove/continue/restart guards. Drawers use existing Ratatui `Clear`. `board_columns` shares the 144/54-cell breakpoints between board reflow and `drawer_width`: six columns use a 40% drawer, three use 70%, and two use 100%. Target widths retain a 36-cell minimum capped at the viewport; calculate the percentage in `u32` to avoid overflow. Both drawers use the same target before animation easing. Opening and closing share a 220 ms cubic easing transition driven by the existing redraw loop. `App::set_drawer` restores board input immediately on close while retaining the drawer kind and interpolated width for rendering until it reaches zero. Reversals start from the current fraction, and widths follow terminal resizing. Disabling `animate_drawers` skips both transitions; no overlay dependency or separate timer is required.

`src/ui.rs` renders the supplied centered ONTRACK wordmark and a terminal-drawn anime girl during the 1.2-second startup splash. `girl_frame` chooses the blink and wave pose from elapsed time supplied by `app::run`'s existing redraw loop, so the animation needs no separate timer or state. If the full wordmark and character do not fit, the renderer uses the compact ONTRACK title with the character when possible, then a text-only fallback. Width checks use terminal cell widths; preserve the wordmark verbatim, keep the splash centered and theme-aware, and ensure it yields to the normal browser view after startup.

The hidden egg is entered from normal board, detail, or worktree browsing with Up, Up, Down, Down, Left, Right, Left, Right, `b`, `a`. The input sequence is isolated from forms, help, and mutation-locked states and resets when it is interrupted or those contexts are entered. `Esc` returns to browsing; quit shortcuts remain active, while other keys, mouse input, and paste are ignored on the egg screen. It is an exclusive render mode: draw it before tiny-terminal, splash, and board paths, using only the active theme and centered Braille art. The supplied 3,315-cell Braille art is a fixed 65-column by 51-row image; show it only when the terminal has at least 65 columns and 53 rows (including the return hint), otherwise use `(o)`. Keep its compact fallback safe even at zero-sized terminals, clear board hit targets as usual, and do not draw help, toasts, or browser content over it. Keep the trigger out of normal help and README content.

Every `Block` in `src/ui.rs` that draws a full border sets `BorderType::Rounded`. This includes board columns, drawers, modal frames, form fields, and help. Keep borderless backgrounds and compact-terminal fallbacks borderless; preserve titles and colours when adding a bordered widget.

`mxo update` handles release discovery before Herdr, Beads, or terminal browser initialization. Browser startup does not make a release request.

In the worktree drawer, Enter queues `herdr workspace focus WORKSPACE_ID` on the existing background worker. The ID comes from the selected OnTrack handoff, never global focus. The adapter passes arguments separately, rejects unsafe IDs, and requires Herdr's `workspace_focused` result to repeat the selected ID. Closed workspaces and removed checkouts are rejected before dispatch. Focus success and errors appear in the drawer without changing Git, Beads, or the registry.

Worktree discovery has no progress placeholder in `ui::render`: empty-state list guidance is gated on an empty list and `!app.worktrees_loading`. Existing rows render during refresh, and header errors and action feedback render independently of loading state.

`theme::worktree_status_color` maps every `WorktreeStatus` to a semantic palette role. `ui::worktree_status_style` applies that colour to the label's foreground characters and leaves the pane background intact; the surrounding selected-row highlight remains separate. Keep list and detail labels in sync when adding a worktree state. Theme tests enforce that pending and attention colours differ from integrated in every bundled theme, and UI buffer tests check all list labels and selected details across bundled themes.

`worktrees::inspect` recognizes a source checkout's `MERGE_HEAD` only when it equals the verified target HEAD, the target checkout is clean, and no OnTrack merge request is active. It displays `MERGE IN PROGRESS` without changing `verify` or action eligibility. `mxo status <working|merging|blocked|clear> [detail]` writes an advisory `agent_report` to the existing locked registry after matching the caller's canonical Git top level to exactly one active managed checkout and verifying its identity. Reports expire from display after 15 minutes and are skipped for completed or identity-error states. `list_with` and reconciliation append them only to display details; merge and cleanup paths never consume them. Tests should keep these boundaries explicit.

`theme::status_color` gives synthetic Ready a readable blue for light and dark themes. The same mapping colours the Ready column border and card status text; it does not change underlying Beads status or worktree colours. Preserve contrast coverage and rendered-buffer checks across bundled themes.

Board and drawer footers show context-specific shortcut hints without consuming errors, notices, focus feedback, or confirmation text. Header feedback remains separate and confirmations stay in dialogs. Worktree selection and details belong to the drawer, not a second full-screen browser. Keep hints within terminal cell width and preserve focus, merge, cleanup, continue, restart, detail scrolling, and help controls. Update `hotkeys` alongside input handling so board `m` (move chooser) and drawer `m` (merge request) are unambiguous.

The catalog now includes 40 themes, with 28 additional choices adapted from
MMOGame-TUI. Keep imported fixed colors readable using the catalog contrast
checks; System and Lucent Orng use terminal-default surfaces, whose contrast
cannot be measured without the terminal palette. Tests verify those surface
roles explicitly instead. Keep the OpenCode license notice with redistributed
palettes; provenance and semantic role adaptations are in [settings](settings.md).

Theme is explicit app state (`App::theme`) threaded into rendering; do not introduce global or thread-local palette state. Keep Dark as the existing default look. New colors need readable Light equivalents for surfaces, text, semantic metadata, startup splash, details, and dialogs. Settings changes immediately attempt persistence; update `App::settings` and the active theme only after `settings::save_to` succeeds. Failed writes retain the candidate for retry without changing the active theme. Tests should use isolated settings paths rather than changing process-wide environment variables.

Choice controls use the shared `cycle_choice` helper for Left / Right navigation. Both directions wrap at the ends; a closed focused dropdown changes immediately, while an open dropdown previews changes until Enter accepts or Esc restores its opening value. Keep this consistent for future dropdown controls. Up / Down behavior remains control-specific.

The create/edit Description field uses `description_layout` to insert visual line breaks before whole words and map its byte cursor to display cells; words wider than the field still wrap by character. `wrapped_description` renders that layout, and `move_description_vertical` uses its byte-boundary positions so Up / Down follows the same visual rows at the form's fixed 64-cell text width. Vertical movement retains a preferred display column across shorter rows, clamps at the top and bottom, and leaves the stored text untouched. Other cursor movement or editing clears that preference. Explicit newlines remain in the stored text. While Description has focus, Shift+Enter inserts a newline and plain Enter focuses the form's action without submitting. Terminal setup enables Crossterm's keyboard enhancement protocol so supported terminals can distinguish modified Enter; restoration pops that mode on normal exit, initialization failure, and panic. Terminals without modified-key reporting cannot distinguish Shift+Enter from Enter.

The hotkey reference is a separate `App::help` overlay, not a modal replacement. `App::key` handles its toggle and scroll keys before dispatching to the board, drawer, or dialog; the mutation lock still takes precedence. `hotkeys::Context` captures that context when opened and owns its key/action list. Update it alongside input handling. `ui::render_help` wraps actions, clamps scrolling, and renders over the board/drawers or forms. The matcher accepts `?` with no modifier or Shift, and `/` with Shift; Ctrl/Alt combinations are excluded. This global shortcut also takes precedence over text input in forms. Plain slash and Backspace retain their existing behavior.

The `s` settings dialog saves each theme/preference change immediately, writing the complete Settings value. Up / Down navigates rows, Left / Right changes theme, default agent, or agent limit, and Enter / Space toggles preferences. Drawer animation defaults on and older settings files default it on. Failed saves must not update committed settings. The default-agent setting uses stable supported kind IDs; `automatic` preserves the old Codex/first-available form behavior, `unassigned` selects no harness, and an unavailable preferred kind falls back only for the current form. Agent limit controls automatic Ready-task scheduling. See the [settings guide](settings.md) for details.
Settings rows in `src/ui.rs` share a two-thirds title column and one-third
value column. `settings_row` aligns titles to the left and centers controls
by terminal cell width; keep dropdown entries on the same value boundary and
retain the existing compact-terminal fallback when changing this layout.

The shared modal path dims the already-rendered board and drawers by darkening
their cell foreground and background colors before drawing the dialog. Keep
the modal's Clear/frame/content after this pass so its theme styles stay intact;
help and toast overlays alone must not invoke it. Tiny-terminal modal guidance
remains readable and does not run through the standard shared backdrop path.
The legacy fullscreen planning editor returns before this shared modal path and
is intentionally excluded.

## Beads CLI contract

For an interactive browser launch, `main` first calls `startup::ensure_herdr_workspace`. It accepts only `HERDR_ENV=1`, the marker Herdr supplies to managed pane processes. Missing or other values return an error with launch guidance before any `bd` command, raw mode, or splash. Help and uninstall dispatch before this guard. The guard checks the inherited marker and does not query Herdr's socket or infer a workspace from global focus.

Before terminal initialization, `startup::ensure_workspace` runs `bd --no-color where --json`. Successful discovery uses its absolute `.beads` path to ensure the adjacent `.gitignore` contains `.beads/screenshots/`, creating or appending only when needed; Beads configuration is untouched. Only the structured `no_beads_directory` error prints manual initialization guidance and returns `Ok(false)`; `main` then exits successfully before raw mode, the splash, or browser startup. Permission, malformed-output, ignore-file, and other discovery failures remain errors. Beads owns ancestor, worktree, and redirect discovery.

Missing-workspace guidance asks the user to run the literal command `bd init`, as requested by the startup UX contract. The command is bold on terminal stdout and plain in redirected output. Startup reads no stdin and never invokes init, init help, or configuration writes. Existing workspace settings, including JSONL auto-export, are preserved; `.beads/issues.jsonl` remains optional interchange data, never the source of truth. Global OnTrack preferences remain separate.

Focused `cargo test startup::tests` covers accepted and rejected Herdr markers, then injects command execution and captures output to cover missing-workspace early exit, terminal highlighting, plain redirected output, the single discovery command, existing/inherited/redirected workspace paths, malformed discovery, other discovery errors, and an unavailable CLI. A temporary directory test covers ignore-file creation, preservation, and repeated startup. No live project initialization occurs in unit tests. For a manual terminal check, run the built binary from a disposable directory with no ancestor Beads workspace inside Herdr: verify the highlighted instruction, immediate successful exit, and absence of setup prompts, alternate-screen entry, or workspace files. Redirect stdout to verify plain output. Launch from an existing workspace to verify normal splash/browser startup and terminal cleanup on quit.

`mxo uninstall` is dispatched before workspace discovery and terminal setup. It uses the running executable path and `settings::config_path` to select only the current installation and MXO user settings; preflight rejects unexpected file types and symlinks. The executable is removed last and checked again against its original file identity. `cargo test --test uninstall` copies the built binary into disposable directories and verifies confirmation, cancellation, missing settings, unsafe paths, and preservation of sibling `bd` and Herdr fixtures.

### Board states and agent harness labels

Ready is `open` plus `ontrack:ready`; Ready To Merge is `in_progress` plus `ontrack:ready-to-merge`. These labels reuse Beads statuses rather than inventing new ones. Open, Blocked, In Progress, and Closed correspond to their Beads states; legacy deferred/custom statuses display under Open until explicitly moved. Moves must update lifecycle markers consistently while preserving unrelated labels and raw record fields.

`beads::move_bead(id, target)` is the guarded per-Bead board-mutation API. It validates the target and current record, changes only that Bead, and never cascades to descendants. Closed routes through `worktrees::close_integrated_bead(id)`, which freshly verifies integration for the recorded tracked handoff before a guarded Beads status update. Untracked or unverified work cannot close via a board move or `c`; there is no unguarded close adapter. Keep CLI values in separate `Command` arguments and surface partial failures with a refresh; exact move argv belongs to the adapter.

Moving into Ready permits the existing scheduler to launch eligible work. Other moves do not start or stop agents, authorize Git operations, or clear durable handoff/recovery evidence. The accepted `· sent` marker is not a lock against manual backward board moves. The `r` / `Shift+R` tree-Ready shortcut is a separate operation with its existing descendant/handoff guards, not the implementation of individual board movement.

Creation writes `ontrack:agent:KIND` or explicit `ontrack:unassigned`; editing preserves lifecycle markers and unrelated labels. Legacy Ready issues without either harness label use the configured default when available, then Codex/first-available fallback. Explicit Unassigned and unsupported kinds do not launch. On form open, background discovery intersects `herdr agent start --help`, `herdr integration status`, and known harness labels without requiring workspace mapping. The form starts Unassigned and blocks submission until discovery finishes; failure leaves Unassigned available with an error. Sequence-tagged responses cannot change a reopened form. Edit retains the stored harness choice, while launch rechecks current availability.

`App::poll_scheduler` runs after responses and worktree polling, waiting for a successful managed-worktree scan. It prepares Ready parents with eligible Ready non-Epic descendants before checking agent capacity; a reservation does not start an agent or consume a slot. Immediate-parent checkout requirements prepare nested checkouts root-first, while agents work child-first. A parent resumes only after open descendants and pending child integration clear, even for legacy children already closed before merge. Non-Epic selection uses priority then ID, one choice per poll; distinct launches may run concurrently within capacity. Attempted IDs and journaled handoffs, including uncertain ones, prevent duplicate retries. Launch/preparation leaves an open Ready Bead Ready; status `in_progress` maps to In Progress unless `ontrack:ready-to-merge` makes it Ready To Merge. Failure retains Ready for explicit recovery, and scan failure pauses dispatch. Scheduled launches recheck live `bd show` for status, Ready label, and harness before mutation. An Epic or dependency-blocked parent may reserve a checkout for children without an agent; actual launches reject Epics and recheck uncapped live `bd ready`. Missing eligibility or lookup failure stops before Herdr mutations.

`beads::list_browser_beads` reads the uncapped issue list, dependency-aware `bd ready --limit 0`, and `bd blocked` as one browser refresh. `Bead.dependency_ready` defaults to false and is set only for IDs in the ready result without a conflicting blocker result. `Bead.blocked_by` holds the authoritative `blocked_by` IDs for the status-row waiting tag; neither field changes the raw issue data, labels, or Beads status. Any subprocess or parse error rejects the refresh, retains the previous display, and pauses scheduler dispatch through `list_error` until a successful refresh. Graph-blocked candidates are skipped without entering `scheduler_attempted`, so they become eligible after blockers clear. Separate CLI reads are not an atomic graph snapshot; the launch-time check protects against dependencies added after refresh. Existing launch failures still use the explicit retry behavior described above.

The subprocess inherits the application's working directory. Commands use argument vectors, not shell strings. Closure uses `bd update --if-status` only after tracked-integration verification; unlike `bd close`, this supports an atomic status precondition while retaining normal dependency guards. Move and closure arguments are built by the adapter:

```sh
bd --readonly --no-color list --all --limit 0 --json
bd --readonly --no-color ready --limit 0 --json
bd --readonly --no-color blocked --json
bd --readonly --no-color show --id=ISSUE_ID --json --long --include-comments --include-dependents
bd --no-color create --title=TITLE --description=DESCRIPTION --type=task --priority=2 --json
bd --no-color create --title=TITLE --description=DESCRIPTION --type=task --priority=2 --json --labels=ontrack:agent:codex
bd --no-color create --title=TITLE --description=DESCRIPTION --type=task --priority=2 --json --parent=PARENT_ID --no-inherit-labels --labels=ontrack:agent:codex
bd --no-color update --title=TITLE --description=DESCRIPTION --type=task --priority=2 --json -- ISSUE_ID
bd --no-color update --add-label ontrack:ready --if-status open --status open -- ISSUE_ID
bd --no-color update --remove-label ontrack:ready -- ISSUE_ID
bd --no-color delete --force --json -- ISSUE_ID
```

Create returns a JSON object containing the created ID; update, close, and delete are successful on a successful command exit. Mutations are not passed `--readonly`; update sends the form fields and changes the harness label only when its choice changes, and delete uses `--force` for the selected issue only and never requests cascading deletion. Each worker queue preserves its own request order. Board moves and guarded closure share the managed queue with reconciliation and cleanup; browser reads and Ready cascades remain independent. Edit opening fetches the current `bd show` record on the browser worker, validates the editable type and priority, and ignores stale load responses using a request sequence. A failed update leaves the draft open; success triggers an immediate refresh and reselects the same ID.

The board `c` shortcut captures the selected non-closed ID/title for confirmation and queues guarded `worktrees::close_integrated_bead(id)` off the terminal loop. Fresh tracked integration proof is mandatory, not merely a displayed Integrated state, label, or agent observation. No tracked handoff means no proof and no close. Success refreshes the board/details; failure retains target/error for retry. Beads close validates the ID, uses no force/cascade flags, and remains subject to Beads guards. Empty columns and closed cards have no close action. Worktree drawer `c` still closes the Herdr workspace, not the Bead.


Card rendering must preserve issue identity, type, priority, title, and useful parent cues within legible column widths. Rounded card outlines use `theme::issue_type_color` for selected and unselected cards; selected cards keep their highlighted fill. Bug is danger red, feature/story use the theme's blue Ready color, and unknown/missing types are muted. Preserve the status-coloured column outlines/status labels independently. Hierarchy does not change column membership or make per-Bead moves cascade. Keep selection and mouse hit testing keyed to the same card geometry as rendering; empty columns must not inherit a stale detail/delete identity.

`Shift+N` opens the create form with an immutable selected parent ID. Child creation passes `--parent=PARENT_ID` as its own argument, so Beads records the hierarchy and list/show JSON exposes the child's `parent` field. It fetches parent labels, uses `--no-inherit-labels`, and carries over ordinary labels, excluding Ready, Ready To Merge, harness, and explicit Unassigned markers so the child starts Open with its own harness. OnTrack reads the uncapped live list to find open descendants before a parent starts or merges. Beads rejects a `blocks` dependency from parent to descendant because it would also block that child; OnTrack therefore enforces its own child-first gate.

`--all` includes closed issues and `--limit 0` removes the default list cap. `--id=ISSUE_ID` is a single argument. The list and show responses must be JSON arrays. List parsing requires nonempty string `id`, `title`, and `status`; a missing or null priority defaults to 2, and supplied priority must be an unsigned integer. Each `Bead` retains the entire JSON object as well as its summary fields.

The Ready scheduler counts accepted managed handoffs with `InProgress` or
`DirtyCheckout` display status plus all pending agent launches before selecting
another task. Pending manual launches count too; preparing an Epic checkout with
open children does not reserve an agent slot. A successful agent start retains a
capacity reservation until a fresh durable worktree scan acknowledges it, even
if an older scan was already in flight. Reserved parent worktrees and finished or cleanup states do not take a
slot. The setting is global to the user and defaults to five; Unlimited bypasses
the count. Explicit `h` launches bypass the gate but count while active. A full
limit leaves Ready labels and the existing priority and ID ordering untouched.

Board display ordering is independent of scheduler priority/ID ordering and never changes underlying records. Preserve useful parent-before-child ordering within a non-Closed column where possible; parents in other columns remain separate. Closed retains newest completion-time ordering with update/creation fallback. Refresh must preserve selection by Bead ID across moves, the active column when empty, and bounded per-column scroll positions.

Detail parsing selects the requested ID, rejects duplicate matching records, and reports a missing ID. It retains the returned JSON intact, including comments, relationships, metadata, and fields unknown to MXO. The detail renderer shows narrative fields, comments, and supported dependency links, omitting miscellaneous metadata and relationship extras from display for both cached list records and full show records. Process-start errors, unsuccessful exit statuses with stderr, and invalid JSON/records are surfaced as errors rather than panics.

## Herdr launch adapter

`src/herdr.rs` exposes `discover() -> io::Result<LaunchContext>` and `launch(context: &LaunchContext, agent: &Agent, bead: &Bead) -> io::Result<LaunchOutcome>` for background-worker use. `LaunchContext` contains a `Workspace` and available `Agent` records (each with string `id` and `label`); `LaunchOutcome.message` carries the handoff result and `LaunchOutcome.reserved` distinguishes a prepared parent from a started agent. These public records derive `Debug`, `Clone`, `PartialEq`, and `Eq`. The app queues the full selected Bead and typed context/agent; the backend still captures authoritative `bd show` JSON before mutation and revalidates workspace/agent availability.

Force removal is `Action::ForceRemoveCheckout`, queued through the existing managed FIFO after the drawer's `Shift+X` confirmation captures the handoff ID. `force_remove` requires an active handoff with no pending mutations or tracked child checkouts. It reuses `verify_identity` and `verify_open_workspace`, rechecks Git identity after the external query, and refuses the process's own checkout. Unlike normal cleanup it permits dirty/unmerged work and never calls Beads or launches an agent. Before `herdr worktree remove --workspace ID --force`, it persists `cleanup_pending: "force-remove"`; it records the response and reuses `confirm_cleanup` to check Herdr workspace/path absence, Git worktree absence, and the retained branch. This operation skips integration requirements only for that explicit journal value. Confirmed removal sets `force_removed` so historical inspection does not claim integration. Reconciliation can confirm an uncertain removal read-only, but never redispatches it; failed confirmation remains Needs Attention. Normal cleanup retains its integration and closure gates. Tests use disposable Git worktrees and a fake Herdr executor.

Restart is a separate confirmed worker request for a selected tracked handoff. The backend requires an active journal entry, verifies the main checkout identity and clean current target branch, checks the recorded checkout path/branch and Herdr ownership, and fetches the live Bead before mutation. It also checks that the recorded agent integration and source workspace are available. It saves a `restart_pending` marker before invoking `herdr worktree remove --workspace ID --force`, captures the response, and verifies both Herdr and Git inventories plus path absence. Only then does it mark the old handoff restarted and use the ordinary launch path with a new unique name and the live Bead. The old Git branch is retained. An uncertain removal is never retried automatically. A failed relaunch records an attention state; any partial new launch has its own normal launch journal for recovery. Normal cleanup still requires verified integration and never uses force.

Pressing `h` records the selected Bead and queues discovery once. Repeated `h` for that pending Bead is ignored; another selected Bead can launch independently. Pending entries are keyed by launch sequence and carry their captured Bead. Discovery requires the selected harness when present; legacy Beads use Codex or the first available agent. Missing selection, discovery errors, and empty agent lists stop before launch. Sequence/identity checks reject stale or duplicate responses. The newest handoff owns the toast; older completions do not overwrite it. Worktrees refresh after success or uncertain failure, with a trailing refresh if a scan is already in flight. A board move does not cancel a pending/accepted launch or erase its recovery evidence. The authoritative registry, not board state or the pending map, prevents duplicate handoffs across processes/restarts.

LaunchJournal retains repository/entry identity, captured Bead context, parent target/base, and immutable reserved-target snapshots rather than a locked Registry. Each mutation reloads the latest registry, verifies its own intent/ownership, updates only that entry, and atomically saves under an exclusive lock. Git/Herdr verification runs outside launch write transactions. Returned creation handles are saved before checkout verification; start and prompt possibly-sent fences precede dispatch; raw start/readiness evidence precedes prompting; `active` follows a validated prompt response. A reserved-parent resume atomically claims the still-reserved entry at the start fence; concurrent stale handles cannot start twice or overwrite the winner's evidence. Journal transactions retry lock contention for about two seconds on background threads, then fail closed. Managed merge/reconciliation still retains its existing longer lock and can exhaust this window. Post-create/start/prompt journal/readiness failures are typed uncertain; failure-recording errors are appended without erasing the original recovery error or declaring the attempt retry-safe.

Discovery is read-only. Supported kinds are parsed from the installed CLI's `herdr agent start --help` list and intersected with **text** `herdr integration status` entries in the observed `current (vN) (path)` format. Availability is not hardcoded: any supported current integration, including Hermes, can be selected when Codex is unavailable. Outdated/not-installed entries and unsupported kinds are excluded; malformed/duplicate current entries and incompatible help/status output fail closed. No integration is installed or repaired automatically. A current integration establishes integration configuration, not a guarantee its agent executable can start.

Workspace selection uses `herdr worktree list --cwd CURRENT_DIRECTORY` and its JSON envelope's `result.source.source_workspace_id`, never global focus, labels, a live agent list, or inherited workspace variables. An absent/null/unsafe workspace ID fails with guidance to open the checkout in Herdr. **Launch discovery from linked checkouts is unsupported:** observed Herdr 0.9.1 responses report the main `source_checkout_path` even when `--cwd` names a linked checkout, so that field does not independently identify the linked workspace. Launch from the main checkout instead. The absolute, accessible, canonical source path must contain the current directory and have a normal `.git` directory. Any intervening `.git` boundary (including a nested linked checkout/submodule) fails before creation, without guessing another workspace. Ordinary main-checkout subdirectories remain supported. Read-only output for the main checkout returned `source_workspace_id=wY` and `source_checkout_path=/home/thomasr/Documents/dev/OnTrack`.

Before any mutation, launch retrieves `beads::show_bead(id)` and composes a prompt with the complete captured JSON. It re-runs discovery and verifies the selected workspace ID and agent against that fresh result. Generated agent names are at most 32 ASCII characters: `ot-`, up to eight trailing alphanumeric bead-ID characters, a separator, and a base32 nonce retaining the 32-bit PID plus a monotonic 64-bit time/counter. Names must start with a lowercase letter and contain only lowercase letters, digits, hyphens or underscores; overlong/unsafe names are rejected before creation. Branches already listed for worktrees are rejected rather than silently reused. It invokes these commands as argument vectors using `std::process::Command`, never shell strings:

```sh
herdr worktree list --cwd CURRENT_DIRECTORY
herdr agent start --help
herdr integration status
herdr worktree create --workspace SOURCE_ID --branch ontrack/UNIQUE_NAME --label UNIQUE_NAME --no-focus
herdr agent start UNIQUE_NAME --kind codex --pane NEW_ROOT_PANE_ID --timeout 30000 -- -c 'projects={"VERIFIED_MAIN_ROOT"={trust_level="trusted"}}' -c check_for_update_on_startup=false --dangerously-bypass-hook-trust
herdr agent prompt NEW_ROOT_PANE_ID PROMPT
```

Herdr 0.9.1 has no `worktree create --json` flag and treats `--workspace` and `--cwd` as mutually exclusive selectors; creation uses only the explicitly validated workspace. The example starts Codex; other discovered kinds use the same start contract without Codex-specific arguments. Prompt submission targets the newly returned pane ID, not an agent name or focused pane. The prompt carries the full Bead JSON, including comments, relationships, acceptance criteria, unknown fields, and the selected ID/title, as one argument. It asks the agent to read project instructions, inspect authoritative details, implement the task, and report checks/blockers. Issue text never becomes shell syntax or additional command options. Launch never supplies `--trust-repository`, changes focus, closes existing panes, or deletes worktrees. Git safe-directory trust remains an explicit user decision, separate from the authorized invocation-only Codex project trust below.

The initial handoff prompt requires checks and a commit before signalling Ready To Merge with status `in_progress` and label `ontrack:ready-to-merge`. It must explicitly instruct the agent **never to close before merge**. Unfinished/blocked work reports its blocker without claiming readiness. Bead closure belongs to verified-integration reconciliation, not to the implementation agent's completion report.

For title-only or incomplete live Beads, the prompt instructs the agent to infer an actionable description, scoped requirements and testable acceptance criteria from the live title and project context, explicitly record assumptions, and persist missing fields with `bd update` successfully **before implementation**. Existing nonempty requirements must be preserved and unrelated scope must not be invented. Missing description/criteria alone is not grounds for refusal; clarification is reserved for consequential ambiguity that context cannot reasonably resolve, after saving a useful inferred description, assumptions and open questions to the Bead. IDs and field values must remain safely separated arguments, not shell interpolation. This is agent prompt behavior, not app-side field generation or a new create-form requirement. Regression tests cover title-only, null/blank fields and detailed payload preservation; they verify the submitted instructions, not external agent compliance.

Only Codex receives the two `-c` overrides and `--dangerously-bypass-hook-trust` after Herdr's `--`: invocation-only project trust for the verified canonical **main repository root**, plus `check_for_update_on_startup=false` and invocation-only trust of enabled hooks. The shell quotes in the example merely illustrate argument boundaries; implementation uses separate argv strings, with the path encoded as a TOML quoted key (quotes, backslashes, Unicode and control characters are handled). Herdr 0.9.1 forwards these arguments; the verified Codex 0.162.1 loader applies CLI overrides before project trust is evaluated. OpenCode and other discovered integrations receive no additional arguments or Git trust lookups. No positional prompt is added to startup: the full captured prompt is still separately submitted once to the verified created pane.

Codex trust resolution first checks the source checkout, then checks the **actual created worktree path** before agent start. Read-only `git -C PATH rev-parse --path-format=absolute --git-common-dir` must identify the same canonical common directory for both. Only the normal non-bare `MAIN_ROOT/.git` layout is accepted. The common directory must be a real directory named `.git`; the main root must independently report its own exact top-level, matching common directory and matching absolute Git directory. Checkout top-level and non-bare checks also apply. Thus linked-worktree `--show-toplevel` alone cannot become the trust key, and arbitrary/nonstandard metadata parents are not trusted. Missing, redirected, mismatched or inaccessible evidence fails safely; source failures precede creation, while created-worktree failures retain recovery details and never start an agent or send a prompt. The resolver follows the [Codex main-repository trust model](https://github.com/openai/codex/blob/main/codex-rs/git-utils/src/trust.rs), but intentionally rejects nonstandard metadata layouts rather than guessing.

Repository trust enables project configuration; the separate `--dangerously-bypass-hook-trust` flag allows enabled user, project, session, and plugin hooks to run without persisted hash approvals for that invocation. The caller is assumed to trust these hook sources when launching through OnTrack. This follows the [Codex hook trust contract](https://learn.chatgpt.com/docs/hooks#review-and-trust-hooks), verified against installed Codex 0.163.0 help; older versions without this flag fail startup and retain the existing recovery path. This applies to managed worktree handoff. It does not enable disabled hooks or override managed hook restrictions. It writes no persistent/global configuration, grants no wildcard trust, does not pass Herdr `--trust-repository`, and does not relax sandbox or approval policy. Authentication, administrator-enforced policy and unknown startup dialogs can still require human intervention; no screen scraping, key injection or auth bypass is attempted. Git trust and metadata are not protected by a cross-process transaction, so external filesystem changes remain a race limitation.

The installed `herdr api schema --json` (protocol 22) specifies a `result.type=worktree_created` response with required `workspace`, `tab`, `root_pane`, and `worktree`. Exact IDs are `workspace.workspace_id`, `tab.tab_id`, and `root_pane.pane_id`. The adapter requires matching nested workspace/tab identities, a workspace not already listed as open for this repository, an absolute different worktree path, and the requested branch. It never substitutes the current/focused pane when a response is missing or incompatible. Herdr checks that the created pane is at an interactive shell prompt before starting an agent. Start/prompt responses must have `agent_started`/`agent_prompted` types and identify that same workspace/pane. Future incompatible schema changes are errors, not guessed aliases.

Creation/start/prompt are sequential and stop on failure. Errors identify the failed stage; after creation is attempted they retain the worktree and report source/new workspace, root pane when known, branch, agent name, and recovery guidance. A failed creation command can have partial side effects; inspect `herdr worktree list` and `git worktree list` before retrying. A startup error can leave an agent running. A prompt failure or unparseable response does **not** prove non-delivery: inspect the agent/pane before resubmitting. Nothing is automatically deleted.

Agent start waits up to 30 seconds for readiness. Prompt submission deliberately omits `--wait`: a successful `LaunchOutcome` means submission was accepted, not that the task completed. All subprocess output is captured rather than leaking into the terminal. CLI readiness timeouts are not an operating-system watchdog for an unresponsive CLI/socket. Discovery and creation are not atomic across external workspace changes; explicit IDs and response validation reduce but do not eliminate cross-process races.

The main issue list adds `· sent` to Open or Ready Beads whose OnTrack journal has the persisted `active` phase. `ManagedWorktree.handoff_accepted` is derived from that phase even if later Git or Herdr inspection reports a worktree needing attention. Earlier phases, including a prompt that may have been sent but was not confirmed, do not assert a successful handoff. The app reloads tracked worktrees after a successful launch, including when a worktree snapshot was already in flight; startup discovery and subsequent polling preserve the marker across refresh and restart. The marker is presentation only and never rewrites the Beads status.

Focused tests (`cargo test herdr::tests`) inject command execution, Bead retrieval, Git identity lookup and launch journaling. Each test thread initializes a disposable main Git checkout with `.git`, `src`, and `target` directories, so tests work when Cargo runs from either a main checkout or a linked worktree, including with `CARGO_TARGET_DIR` set elsewhere. They use deterministic fake text/JSON responses and captured argv, with no process-wide environment mutation, real Git worktree creation, or live Herdr creation/start/prompt. The tracked-launch regression persists a fake journal and checks that each phase is recorded before the next action; real Git journal behavior is tested separately in `src/worktrees.rs`. Coverage includes dynamic supported kinds/current integrations, malformed output, unavailable CLI, missing workspace without focus fallback, rejection of outside/nested linked launch directories using the observed main-source response and filesystem-only `.git` markers, full Bead capture before mutation, arbitrary single-argument prompting, ordering, no prompt wait, selection/collision checks, 32-character agent-name bounds and pre-creation overlong-name rejection, new target validation, stop-on-failure, and partial-success recovery. Codex regressions cover both overrides and the hook-trust flag after `--`, separate pane-targeted prompting, unchanged OpenCode startup, TOML key escaping, normal/linked **created-worktree metadata** main-root resolution (not linked-source discovery support), nonstandard metadata rejection, and source/new identity failure without starting an agent. Installed read-only help/worktree-list/schema established the 0.9.1 contracts; no live Herdr creation or agent lifecycle is exercised. Fixtures cover the required response subset, not every server version or external race.

Worktree registry tests create exclusive temporary fixture directories and keep each registry lock inside its fixture. These test paths do not change the production rule that Herdr launch discovery starts from the main checkout.

Merge requests use the repository-wide registry lock and persist `merge_queue` order and provenance on each handoff. Manual requests and automatic reconciliation both enqueue; reconciliation dispatches at most one prompt when no unresolved `merge_request` remains. Before checking that active-request fence, dispatch clears queued entries whose source-tip ancestry already proves integration (or whose workspace/checkout is already closed/removed); this prevents external Git integration or cleanup from leaving stale queue slots without weakening active prompt settlement. Dispatch revalidates live Git/agent state and, for automatic requests, the Ready To Merge status/label signal rather than pre-merge closure. Failed pre-prompt dispatch requires explicit retry; possibly sent prompts keep their durable fence and block later prompts. Verified integration and a settled prompted agent release the slot. External clients do not share this queue, so Git checks and ff-only integration remain required. A Beads-close failure after integration must retry only closure, never requeue or replay the merge.

## Background work and cached state

`mxo update` calls `update::check` in the foreground, prints the running
version and release status, and requires an explicit `y` or `yes` before calling
`update::install`. The update module calls `curl` with fixed HTTPS endpoints and
separate arguments, parses semantic versions, accepts
`mxo-vVERSION-TARGET.tar.gz` with a matching `.sha256` sidecar, and extracts
only the `mxo` entry from the archive. The selected target is one of glibc
Linux or macOS on x86_64 or aarch64. Release URL and asset URL checks restrict
downloads to the official tagged repository. Downloads and candidate
executables are staged with unique names in the executable directory, and
temporary files are removed on errors. SHA-256 is verified before extraction or
replacement, including GitHub's API digest when present; the final rename is on
the same filesystem. Failure before that rename leaves the original path
untouched. A running browser process continues using its old image; restart it
after success. `curl` and `tar` are runtime prerequisites for this command,
while normal browsing does not need them.

`App` uses standard-library channels with separate serial browser-Beads, Ready-mutation, and managed-work queues. Herdr discovery, harness discovery, manual/Ready launches, workspace focus, screenshot reads, and display-only agent observations are independent detached jobs. The UI loop checks available responses and polls input rather than waiting for subprocess completion. While any handoff is pending, worktree polling chooses the read-only list path instead of reconciliation. List and reconciliation display passes copy registry entries and release its lock before Git, Beads, or Herdr display checks; this lets another launch record its intent during a slow status lookup. Reconciliation still holds its lock while applying managed mutations, so launch journaling retains its bounded lock retry and recovery error.

Rendering is dirty/deadline-driven: draw initially, after terminal input or worker responses, after visible poll-induced loading/errors, and when `ui::next_visual_update` is due. Recompute the visual deadline after each draw, including the final splash/drawer frame and toast expiry; settled or expired visual state must return no deadline rather than spin. `event::wait_timeout` caps response checks at 100 ms and shortens that wait for visual deadlines; Crossterm wakes immediately on keyboard, mouse, or resize input. Cheap refresh eligibility checks retain completion-based, non-overlapping board/planning/worktree/agent polling. The Ready scheduler scans only after input, responses, or its own dispatch/notice progress, stopping once eligible work is drained instead of rescanning on idle timeouts. Terminal cleanup and detached worker behavior are unchanged.

A settled unchanged view does not continuously render. This removes idle rendering/scheduler work, not the cost of retained background `bd`, Git, or Herdr subprocesses. Measure TUI process CPU separately from child/backend CPU with the same PTY dimensions and fixture; backend size and command latency still affect overall resource use. Deterministic inline event/app tests use supplied `Instant`s to check idle suppression, response redraws, visual wakeups/expiry, poll loading/errors, and scheduler progress without sleeping.

For `ontrack-hwe`, a Linux 120×40 PTY with 100 synthetic issues and no managed
worktrees measured application CPU for 12 seconds after a six-second warmup.
Release-build CPU fell from 2.33% to 0.42% of one core (82% less); debug-build
CPU fell from 14.83% to 1.42% (90% less). These are single samples with 10 ms
CPU accounting and stubbed CLI backends, not measurements of live backend cost.
Both versions passed navigation, drawer, resize, quit, and terminal-restoration
smoke checks; a headless PTY does not establish visual appearance.

The separate agent-observation response may relabel only an `InProgress` managed row as `Blocked` when its recorded pane, workspace, agent kind, native session, and terminal match and Herdr reports `blocked` or `error`. Other or failed observations restore `InProgress`; a worktree refresh carries a verified blocked observation forward until the next poll. Git-based statuses retain precedence. The observation remains presentation data and does not change Beads or the mutation gates.

Failed Ready handoffs share `App::fail_handoff` across discovery, harness selection, launch, and queue errors. `ResetFailedHandoff` uses the Ready worker lane and the existing per-ID label removal, without changing Beads status or cascading to descendants. Its separate response preserves the original error and does not release an unrelated mutation lock. Only confirmed label removal updates cached cards; success and failure both refresh authoritative browser state. Queue failure reports that Ready could not be removed. Scheduler attempt fencing remains in place, successful/reserved handoffs are unchanged, and partial-launch journal/worktree evidence remains available for recovery.

Ready mutations (`SetReady` and `SetTreeReady`) use their own serial worker lane so bulk label updates never hold up list/detail requests. `mutation_busy` still excludes conflicting actions and pauses scheduling; `ready_pending` permits browser navigation, help, worktree browsing, and quit through that exclusion. Pending feedback is a header notice. Completion clears both flags, applies only confirmed IDs, and invalidates list/detail snapshots on success or error. Existing refresh sequencing schedules a trailing read when an older snapshot is already in flight. Queue failure clears both flags immediately. Workers remain detached on exit; quitting does not cancel a Beads update already running.

Automatic refresh uses the same public read-only list/show commands above. Schedule the next poll roughly one second after the previous refresh completes, including reloading the selected detail; do not accumulate timer-driven requests while refresh work is in flight. This is polling, not an event stream: slow CLI calls extend refresh latency. No watcher, extra dependency, configuration, or optional Beads events setup is required. Successful create/edit/move/close/delete operations request an immediate refresh. In the worktree drawer, `r` separately refreshes tracked worktrees.

Each serial queue retains request order without dropping mutations. Launches and managed reconciliation also perform authoritative read-only `bd show` calls outside the browser queue; not all `bd` calls are serialized. Responses for a show ID other than the current selection are ignored. Independent jobs may complete out of order and are correlated by their sequence/Bead identity.

The durable `w` drawer appends `Agent observation (display only): ...` to existing details. Observations snapshot the registry, release its lock, and query each exact recorded pane; workspace/kind, native session, and terminal must still match. Missing, malformed, mismatched, or failed observations report unknown and retain recovery identity. Polls do not overlap and recur about two seconds after completion. This presentation evidence never authorizes retry, readiness, merging, closure, or cleanup; readiness labels, original-session/lifecycle checks, and fresh Git integration proof remain separate gates.

Detail drawers use cached list JSON immediately while requesting full details. Refresh keeps prior cards/details while loading and preserves selection by ID, active column, drawer context, bounded per-column/detail scroll positions, and form input. Failed reads retain the last snapshot with visible errors and retry automatically. Narratives render with `tui-markdown`; details retain the selected ID, raw Beads status, and priority even when display-column mapping differs. `detail::render` omits Other Details metadata, and `detail::relationship_chart` omits Relationship Details extras and unsupported records without mutating JSON. Rendered text drives wrapping and scroll bounds.

Keep CLI calls off the input/render loop and preserve stale-response filtering when changing the worker. The worker is not joined on quit, so terminal cleanup does not wait for a slow CLI call; in-flight commands are not cancelled on quit.

## Release check

`src/update.rs` queries the official `guibleme/macross-x-ontrack` GitHub Releases
[`latest` endpoint](https://docs.github.com/en/rest/releases/releases#get-the-latest-release)
with `curl`. It validates an official stable semantic-version tag and a
compatible asset before offering installation. HTTP 404 yields a no-release
result; unavailable assets and malformed metadata return errors. The running
version comes from `CARGO_PKG_VERSION`; `semver` compares it with the release.
`src/update_cli.rs` keeps the prompt and output separate from the browser;
injected checker/installer tests cover current, no-release, error, decline, and
confirmed-install paths without network access or executable replacement.

## Terminal cleanup invariants

`main` attempts restoration after `app::run` returns, before returning its error. Terminal initialization attempts cleanup if entering the alternate screen, enabling mouse capture, or constructing the terminal fails. The installed panic hook attempts cleanup before invoking the previous hook. Restoration attempts cursor display, disables raw mode and mouse capture, and leaves the alternate screen; preserve these paths when changing startup or exit handling.

Run interactive checks in a real terminal or a PTY, not an ordinary non-interactive command runner. Exercise quit during startup, navigation, refresh/loading, errors, and tiny terminal sizes. Normal/error/panic cleanup does not imply cleanup after an uncatchable process kill.

## Tests and required checks

Tests are inline in the source modules. They cover CLI arguments and parsing, form input and mutation state, navigation and focus, mouse selection, cached/stale detail behavior, quit-key recognition, and rendering using ratatui's `TestBackend`.

For deterministic board scrolling regressions, use fixed card fixtures and a fixed-size `TestBackend`, render, send keyboard or wheel input, then render again. Assert both the selected card and per-column viewport offsets at the upper and lower visible edges: in-view moves leave offsets unchanged, edge crossings scroll only enough to reveal selection, first/last cards do not wrap, and switching columns preserves each column's offset. Check the rendered selected card remains visible; no live Beads workspace or timing delays are needed.

Escape is navigation, not a quit key. An open choice list consumes Escape before its dialog; closing a drawer restores the board selection. Escape on the board does not leave the app. Preserve explicit quit keys (`q` and `Ctrl+C`) and existing mutation locks, including navigation allowed during Ready updates.

For automatic refresh, check external list/detail changes without manual input, completion-based polling without a backlog, cache/error recovery, and preservation of selection, column/drawer context, per-column scrolling, and form input. Keep immediate refresh after mutations and stale-detail filtering covered. Board coverage should include six ordered empty counts, deferred/custom fallback with raw status preserved, empty-column navigation, responsive grid reflow and vertical scrolling, adjacent/chooser moves without child cascades, sent-marker backward moves, drawer restoration, and unchanged form/help/worktree recovery actions. Lifecycle coverage should prove Ready To Merge signalling, refusal to close without fresh tracked integration, external/manual integration with all preferences off, child-versus-top-level targets, close-only retry after a Beads failure, and no premature cleanup or destructive authorization from board moves.

Run all required gates from the repository root:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build
```

GitHub Actions runs these gates on pull requests and pushes to `main`. See the [CI guide](ci.md) for workflow details, logs, and branch protection setup.

The same commands can run from a linked Git worktree. Keep the default test concurrency for `cargo test` to exercise fixture and registry lock isolation.

For focused iterations:

```sh
cargo test beads::tests
cargo test herdr::tests
cargo test app::tests
cargo test event::tests
cargo test ui::tests
```

Unit tests do not replace an interactive terminal cleanup check. When changing functionality, update the relevant Markdown guides under `docs/` and keep the README concise, preserving its controls and links to detailed documentation.

## Mutations

Create, edit, delete, and per-Bead moves are asynchronous CLI operations. Forms retain Beads types and P0–P4 priorities. Deletion and the explicit `c` close shortcut require affirmative confirmation and never cascade. Board moves use `beads::move_bead(id, target)`; closure uses `worktrees::close_integrated_bead(id)` with fresh tracked proof before the low-level Beads command. Reconciliation also closes integrated Beads without a separate cleanup preference. Requests retain queue order and prevent repeat submissions while pending; success refreshes board/details, and failures remain visible for recovery.

## Board layout and details

Fixed display-column order replaces raw-status alphabetic groups. Closed uses parsed RFC 3339 `closed_at` descending, falling back to `updated_at` and then `created_at`; undated cards follow dated ones, with ID breaking ties. There are no collapsed headers. Shared geometry must support responsive board grids and independent column viewports without changing Beads data or scheduler ordering. Keep the active column visible, retain empty-column focus without a stale card identity, and align selected IDs with drawer and mutation targets after refresh or movement.

`detail::relationship_chart` renders direct `dependencies`, `dependents`, `blocks`, and `blocked_by` fields as compact solid Unicode branches. Blocking groups come first: “Blocked by ← prerequisites” and “Blocks → dependents” are relative to the selected issue, matching [Beads dependency direction](https://beads.gascity.com/core-concepts/issues). Explicit `blocks`/`blocked_by` fields supply that type when omitted; other untyped records stay unspecified. `dependency_type` takes precedence over `type`. Parent-child links group under Parents/Children; other types retain their source field. Node headers and branches use semantic indexed colors mapped by `theme::markdown_foreground` into the active theme's priority palette; explicit numeric priority labels preserve meaning without color. Valid priority values appear inline; malformed priorities, unused fields, and unsupported relationship records are omitted. Unsupported records alone produce no dependency section, while explicitly empty arrays retain the no-links message. IDs may be strings or objects with `id`. Labels stay literal rather than being parsed as Markdown and wrap by display cells before `Paragraph` calculates scroll bounds. Panes under 12 columns omit indentation. Compactness tests require eight representative children to occupy 19 chart rows at 100 columns; no relationships or titles are dropped to fit a viewport.

Renderer choice for OnTrack-338: retain `tui-markdown` for narrative text and draw structured relationships directly with Ratatui. [ratatui-markdown](https://github.com/celestia-island/ratatui-markdown) supports Mermaid and its current manifest targets Ratatui 0.30, so it is technically compatible. Its Mermaid feature adds `pest`/`pest_derive` and a Markdown-to-diagram parsing path. Our input is already structured JSON, and this feature needs only a selected-node diagram of direct links. Direct rendering avoids generating/escaping Mermaid identifiers, changing the established narrative styles, and adding parser dependencies; no dependency changes are needed. This is a scoped renderer decision, not general Mermaid support for descriptions. Inline tests cover arrow direction, structural/unknown types, hidden fallback data, Unicode and narrow widths, priority styles and compact grouped rows, plus a rendered UI buffer and scrolling after resize.

Cards derive issue type from retained `Bead.data` (`issue_type`, with `type` as fallback), not by replacing the internal ID. Preserve semantic type/priority/status cues and readable title text independently of selection emphasis; missing/custom types remain safe to display. Column mapping never rewrites raw status or unknown JSON fields.
## Tracked worktree lifecycle

`a` in the worktree drawer requests one continuation prompt for the same
recorded agent and checkout. `continue_task_with` verifies canonical Git and
Herdr ownership, the live open Bead, the original native session, an idle/done
agent, and its current checkout paths. Dirty task edits are allowed and kept.
The journal stores `continue_prompt_pending` before dispatch. A transport or
response error leaves that fence and a Needs Attention row after app restart,
so no prompt is resent blindly. A matching acknowledgement clears the fence,
but the pre-prompt lifecycle sequence prevents another prompt until the agent
has reported later activity.
The existing confirmed `Shift+R` path remains the fallback when the original
agent cannot be verified; it refuses an uncertain continuation fence.

`worktrees_inflight` bounds pending scans independently of `worktrees_loading`,
which hides empty-state guidance only during initial discovery without a prior
scan error. Background refreshes and retries retain the settled empty view.
The refresh regression renders terminal buffers at normal and compact sizes
across initial discovery, repeated polls, manual refresh, failure/retry, and
the arrival of a new handoff to catch text disappearing between frames.

OnTrack records only worktrees created by its own explicit Herdr actions; it
does not adopt pre-existing Git worktrees. The all-handoffs recovery drawer is separate
from Beads: a closed or missing Bead does not remove the managed-worktree record.
The app filters verified `CheckoutRemoved` rows from the visible worktree list
on refresh and immediately after a successful removal action. The registry still
retains those records, and `NeedsAttention` rows remain visible when inspection
finds a problem after removal. Selection clamps to a remaining row or the empty
state. Accepted handoff IDs are retained separately from visible rows so an Open
Bead's `· sent` marker still reflects its confirmed handoff.
The existing periodic reconciliation checks the recorded path of each active
handoff before considering automatic actions. A confirmed missing path sets
`checkout_removed` and `checkout_removed_externally` in the registry even when
cleanup preferences are off; it does not call Herdr, remove a Git branch, or
claim that branch was integrated. An invalid path or filesystem inspection
error is retained as `NeedsAttention`. A pending restart remains visible for
recovery; a pending cleanup operation still uses its existing read-only
confirmation before this check. External-removal records
remain removed across restarts while the path is absent; if it reappears, they
surface as `NeedsAttention` for identity review.
The launch journal stores the authoritative Bead title and description before
Herdr creates a worktree. The worktree drawer prefers current text from the already
refreshed Beads list and falls back to this registry snapshot after Bead removal
or a list failure; it makes no extra `bd` request. Registry entries from older
versions may omit these optional fields; the view then shows a fallback. The
drawer wraps multiline descriptions and keeps its own bounded scroll position,
reset when the selected worktree changes.

For a parent with open or unintegrated children, `h` creates and verifies a managed parent checkout, then persists phase `reserved` without starting an agent. A child launch requires that checkout, records its parent Bead ID and target branch/path, and passes `--base PARENT_BRANCH` to Herdr creation. The child signals Ready To Merge, integrates into its parent branch, then closes. Later parent `h` reuses the reserved workspace/pane rather than creating another checkout. The registry rejects duplicate active handoffs regardless of manual board moves. Background reconciliation must continue with all automatic preferences off to discover tracked integration and retry Bead closure; it cannot be gated only on the visible drawer or cleanup settings. Reconciliation selects `auto_merge_children` for managed children (default true) and `auto_merge` for top-level handoffs (default false). Both consume the new readiness signal through durable `queue_merge` and guarded dispatch. Source/target identity, cleanliness, branch, and tips are verified before prompts or target fast-forward. Parent requests/finalization recheck open descendants and pending child integration. Child entries remain below parents for recovery; selection uses worktree ID. Parent cleanup waits for child checkout removal. Historical inspection of removed children may use retained child/parent branch ancestry when the parent checkout is missing; a changed existing path fails identity checks, and live mutations never use that historical fallback.
The verified repository target branch is shown to the user. A merge request is
pending work, not a completed merge; Git ancestry/cleanliness checks decide
integration. Conflicts and uncertain outcomes remain inspectable for manual
recovery. Cleanup is permitted only after verified integration. Checkout removal
is one confirmed operation that removes the checkout and closes its Herdr
workspace; when both cleanup switches are enabled, removal runs instead of a
separate close. It never force-removes a checkout or deletes a branch. Ready To
Merge is an eligibility signal, not proof that an agent is idle or its changes
are integrated. Automatic merge may defer; confirmed manual merge remains
available under the existing Git and agent checks.

`worktrees::close_integrated_bead(id)` closes only after fresh tracked integration
verification into the recorded target: the parent branch for a child, the
recorded top-level target otherwise. Reconciliation detects external and confirmed
manual integration even with automatic merge/cleanup disabled. Persist integration
before attempting Beads closure; if closure fails, retain visible recovery state
and retry that close only, never replay the merge or clean up prematurely. Closed
board moves and shortcut `c` use this same boundary. Untracked work, a cached
Integrated display, a readiness label, and advisory agent activity are not proof.
Legacy already-closed pre-merge handoffs are not bulk reopened; keep their work
recoverable conservatively without treating closure as integration evidence.
No manual board-state change authorizes destructive Git operations.

`inspect` in `src/worktrees.rs` remains the Git-based safety classification.
Read-only inspection checks checkout identity, branch, Git tips, and unfinished
operations separately from ordinary tracked or untracked changes. A dirty source
or target yields `DirtyCheckout` (`UNCOMMITTED CHANGES`) rather than
`NeedsAttention`; it cannot be displayed as ready or integrated, so it cannot
enable cleanup or automatic merge. A pending merge request keeps
`MergeRequested` and includes a dirty-checkout detail. Unresolved source merge
conflicts still take `NeedsResolution`, and identity or history failures still
take `NeedsAttention`. Mutating paths use strict `verify`, which requires both
checkouts to be clean before any merge, close, or removal operation.
For an active branch with task commits outside its target, `inspect_for_display`
shows `InProgress` until the live Bead signals Ready To Merge (`in_progress`
plus `ontrack:ready-to-merge`) and the recorded agent settles after prompt activity.
A launch with a
native session uses the strict settled-session check. For older launch responses
without a session, the display checks the captured pre-prompt `agent_info`,
later lifecycle sequence, pane/workspace/kind, checkout paths, and terminal ID
when present. Failed or unavailable completion observations remain `InProgress`;
an untouched launch-base branch is also `InProgress`. The internal `Unmerged`
status renders as `READY TO MERGE` in the worktree drawer. New handoff prompts
require checks and commits before that signal and explicitly forbid pre-merge
closure; unfinished work keeps its Bead open or in progress without readiness.
The display check makes read-only Beads and Herdr calls during worktree refresh;
it does not change merge, finalization, or cleanup gates. A legacy launch without
a recorded session can show `READY TO MERGE` from weaker display evidence.
Automatic reconciliation separately requires a later native session on the same
recorded terminal and persists it before acting. Merge completion then requires a
settled matching agent, a confirmed prompt, and Git ancestry proving that the
source contains both commits recorded for the request. That ancestry is sufficient
even when Herdr does not report a newer lifecycle sequence after the merge prompt;
without the Git proof or settled agent, automatic merge remains deferred. Explicit
manual merge confirmation remains available for InProgress rows and is still
subject to the existing Git and agent checks.
The launch journal records `state_change_seq` and `terminal_id` from the
pre-prompt `agent get` response even when `agent_session` has not appeared yet.
Reconciliation can recover those observations from the retained `ready`
response in older entries. A later native session is bound and persisted only
after the same pane, workspace, agent kind, terminal, checkout, and post-prompt
activity pass the settled-agent check. Subsequent merge and cleanup passes
require that exact session. Missing or changed evidence leaves the worktree
visible; no automatic action is dispatched.
`inspect_for_display` only changes a `NeedsResolution` merge-request row to
`MergeRequested` when a read-only Herdr `agent get` response reports `working`
for the recorded pane, workspace, and agent kind, with the recorded session
matching when available. The original Git concern stays in the detail text.
Idle, done, blocked, mismatched, malformed, and unavailable observations retain
`NeedsResolution`. Both listing and reconciliation use this display mapping;
finalization, action gates, and cleanup continue to use the Git classification.

Manual merge provenance is persisted separately from automatic requests: optional
agent-session metadata is not required for an explicitly confirmed manual merge,
but current Git identity, cleanliness and ancestry checks still apply. Native
idle/sequence evidence is not proof of a particular prompt turn completing.
Cleanup revalidates the current Herdr workspace/path/branch mapping before any
mutation. Untouched launch-base branches remain unmerged. If a workspace closed
before checkout removal, OnTrack can remove the checkout through Git after it
verifies the recorded repository, branch, clean source and target, integrated
ancestry, absence of an open Herdr workspace for that path, and absence of
tracked child checkouts. It uses non-force `git worktree remove`, preserves the
branch, and confirms both path removal and Git registration before clearing the
recovery state. An uncertain result is inspected on refresh and is never retried
automatically.
## Screenshot descriptions

`src/screenshots.rs` reads PNG and JPEG clipboard data using `wl-paste` or `xclip`, limits images to 10 MiB, and stores them in the shared workspace reported by `bd where --json`. The create/edit form inserts an absolute Markdown image path only after the bytes are saved. The Herdr launch prompt extracts these references from the live description and lists paths for the agent to inspect. Image storage is local and separate from Beads records and Git worktrees; do not assume remote sync or embed binary data in Herdr text prompts.
