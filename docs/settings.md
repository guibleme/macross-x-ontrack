# Settings

OnTrack stores theme, default-agent, agent-limit, animation, and cleanup preferences
globally for your Linux user, not in the workspace or Beads database. OnTrack
Dark is the default theme. Forty built-in themes are available:

| Stable ID | Display name | Style |
| --- | --- | --- |
| `dark` | MXO Dark | Dark |
| `light` | MXO Light | Light |
| `tokyo-night` | Tokyo Night | Dark |
| `nord` | Nord | Dark |
| `dracula` | Dracula | Dark |
| `gruvbox-dark` | Gruvbox Dark | Dark |
| `gruvbox-light` | Gruvbox Light | Light |
| `catppuccin-mocha` | Catppuccin Mocha | Dark |
| `catppuccin-latte` | Catppuccin Latte | Light |
| `solarized-dark` | Solarized Dark | Dark |
| `solarized-light` | Solarized Light | Light |
| `rose-pine` | Rosé Pine | Dark |
| `field-journal` | Field Journal | Dark |
| `opencode` | OpenCode | Dark |
| `aura` | Aura | Dark |
| `ayu` | Ayu | Dark |
| `carbonfox` | Carbonfox | Dark |
| `catppuccin-frappe` | Catppuccin Frappé | Dark |
| `catppuccin-macchiato` | Catppuccin Macchiato | Dark |
| `cobalt2` | Cobalt2 | Dark |
| `cursor` | Cursor | Dark |
| `everforest` | Everforest | Dark |
| `flexoki` | Flexoki | Dark |
| `github` | GitHub | Dark |
| `kanagawa` | Kanagawa | Dark |
| `lucent-orng` | Lucent Orng | Terminal |
| `material` | Material | Dark |
| `matrix` | Matrix | Dark |
| `mercury` | Mercury | Dark |
| `monokai` | Monokai | Dark |
| `night-owl` | Night Owl | Dark |
| `one-dark` | One Dark | Dark |
| `orng` | Orng | Dark |
| `osaka-jade` | Osaka Jade | Dark |
| `palenight` | Palenight | Dark |
| `synthwave-84` | Synthwave ’84 | Dark |
| `vercel` | Vercel | Dark |
| `vesper` | Vesper | Dark |
| `zenburn` | Zenburn | Dark |
| `system` | System | Terminal |

Lucent Orng retains the terminal background with fixed orange accents. System
uses terminal foreground/background defaults and ANSI accents. Their appearance
and contrast depend on your terminal palette; use a fixed theme if these choices
are hard to read. System keeps OnTrack's blue Ready/feature color.

## Palette references and attribution

Theme names identify visual inspirations; OnTrack assigns semantic roles,
contrast adjustments, and interface colors for this terminal UI. OnTrack is not
affiliated with these projects. See their upstream references and licenses:

- Theme-catalog inspiration: [OpenCode v2 themes](https://opencode.ai/v2/docs/themes).
- [Tokyo Night](https://github.com/folke/tokyonight.nvim) — Apache-2.0
  ([license](https://github.com/folke/tokyonight.nvim/blob/main/LICENSE)).
- [Gruvbox](https://github.com/morhetz/gruvbox) — MIT
  ([license](https://github.com/morhetz/gruvbox/blob/master/LICENSE)).
- [Nord](https://nordtheme.com/docs/colors-and-palettes).
- [Dracula](https://draculatheme.com/spec).
- [Catppuccin](https://catppuccin.com/palette) — MIT
  ([license](https://github.com/catppuccin/catppuccin/blob/main/LICENSE)).
- [Solarized](https://ethanschoonover.com/solarized/).
- [Rosé Pine](https://github.com/rose-pine/rose-pine).

These references are attribution, not an affiliation claim. Any exact upstream
palette data reused or adapted must keep its required copyright and license
notices with the redistributed data; consult the current upstream license.
The 28 additional choices are adapted from MMOGame-TUI's local theme catalog.
Its OpenCode palettes trace to [commit
171dbd6abe416cc1d9adaab8752bc3d60b133725](https://github.com/anomalyco/opencode/tree/171dbd6abe416cc1d9adaab8752bc3d60b133725/packages/tui/src/theme).
The required [OpenCode MIT notice](../assets/themes/OPENCODE-LICENSE.txt) ships
with the palettes. Field Journal comes from MMOGame-TUI's original palette.

The OnTrack-specific role mapping and contrast choices are maintained in
`src/theme.rs`. Imported backgrounds, panel colors, borders, and primary accents
retain their source identity. Text and semantic colors are brightened where
needed for contrast. Primary accents also color selections; the base background
becomes selection text. Warnings use OnTrack's gold and Ready To Merge uses
purple so lifecycle states remain distinct even in monochromatic source themes.
Existing equivalent themes keep their original OnTrack palettes and IDs.

## Location and format

- If `XDG_CONFIG_HOME` is nonempty and absolute:
  `$XDG_CONFIG_HOME/ontrack/settings.json`.
- Otherwise: `$HOME/.config/ontrack/settings.json` when `HOME` is absolute.
  Empty or relative `XDG_CONFIG_HOME` values are ignored.
- If neither a valid `XDG_CONFIG_HOME` nor an absolute `HOME` is available,
  resolving the settings path returns an error.

The file is a JSON object:

```json
{"theme":"dark","default_agent":"automatic","agent_limit":5,"auto_merge":false,"auto_merge_children":true,"auto_close_workspace":false,"auto_remove_checkout":false,"animate_drawers":true}
```

`theme` accepts one of the stable IDs above; existing `"dark"` and `"light"`
files remain valid. Missing `auto_merge_children` defaults to true, including in
older theme-only files. Other missing worktree preferences default to false.
A missing `agent_limit` defaults to 5.
`animate_drawers` accepts a boolean and defaults to true when omitted.
It accepts a positive integer or `"unlimited"`; zero, negative, fractional, and
other values are invalid. Child merge requests are on by default; top-level
merge requests, workspace closing, and checkout removal are off by default.
Background reconciliation remains active to detect integration and finish Bead
closure even with all automatic merge and cleanup preferences off. Epic subtasks merge
into their recorded parent checkout. Workspace closing and checkout removal
preferences also apply to integrated child worktrees, with the same safety
checks as top-level tasks; parent cleanup waits for child checkouts to be removed.
Unknown additional keys
are accepted for future expansion. Invalid JSON,
non-object JSON, unknown themes, and non-string theme values are errors.

`default_agent` accepts `automatic`, `unassigned`, or a supported agent kind such
as `codex` or `hermes`. Existing files without this field use `automatic`.
Settings lists the supported kinds; the new-issue form checks current Herdr
integrations when it opens. `automatic` selects Codex when available, otherwise
the first available integration. A saved kind that is not currently available
uses that same fallback in the form while retaining the saved preference.
`unassigned` starts new issues without a selected agent. New child Tasks use the
same preference. The form choice can always be changed; editing an existing
Bead keeps its stored agent choice.

## Changing settings

Press `s` to open Settings. `Up` / `Down` moves among Theme, Default agent for
new issues, Agent limit, top-level merge, child merge, workspace close, checkout
removal, drawer animation, and Close.
The dialog places setting titles in a wider, left-aligned column and centers
theme, agent, limit, and cleanup controls in the right column. The choice lists open beneath
its value; on narrow supported terminals, long titles and values may be
shortened with an ellipsis.

`Left` / `Right` cycles theme or default-agent choices with wraparound and
immediately saves the new choice, whether the list is open
or closed. The bounded lists scroll to keep the current choice visible. `Enter`
or `Space` opens the focused list; while it is open, either key saves the selected
choice again and closes the list, which also lets you retry a failed save. `Esc` closes
an open list without reverting a successful change; `Esc` again closes Settings.
`Enter` / `Space` toggles a preference. Drawer animation is on by default;
turn it off to open and close both right-side drawers immediately. Changes take effect at
once and save with the rest of Settings. Every change saves all settings
together. The removal preference is labeled “Remove checkout + close Herdr workspace”;
removing the checkout also closes its Herdr workspace. Cleanup remains
subject to backend integration checks and cannot force-remove a checkout.
Use `Tab` / `Shift+Tab` to move focus; Close exits the dialog.

“Auto-merge child tasks into parent” controls managed child handoffs, including
Epic subtasks and nested child tasks. It is enabled by default. Once a child
Bead signals Ready To Merge (`in_progress` plus `ontrack:ready-to-merge`) and its
agent has settled, OnTrack requests integration into the recorded parent
worktree without a confirmation dialog. The existing Git,
agent, and Beads checks still apply; conflicts and uncertain outcomes stay
visible for manual recovery. Turning this off leaves children available for
the confirmed manual `m` action. “Auto-merge top-level tasks” controls
handoffs without a managed parent independently and remains off by default.
Neither merge setting closes workspaces or removes checkouts by itself.

Bead closure is not a cleanup preference: after freshly verifying tracked Git
integration, OnTrack closes a child integrated into its parent branch or a
top-level Bead integrated into its recorded target. This also applies to detected
external or confirmed manual merges when automatic merging and cleanup are off.
A failed Beads close is retried without replaying the Git merge; cleanup must
wait rather than conceal the incomplete lifecycle. A board move alone never
authorizes a merge, workspace closure, checkout removal, or restart.

Agent limit starts at 5. `Left` / `Right` moves through Unlimited, 1, 2, 3,
and increasing positive counts, saving each change immediately. `Enter` or
`Space` opens the list and closes it with a save, including a retry after a
failed save. The list scrolls around the selected count. The limit applies to
automatic Ready-task scheduling. Active OnTrack-managed handoffs started with
`h` occupy slots, but `h` itself is not blocked by the limit. Reserved parent
worktrees and handoffs ready for merge or cleanup do not occupy slots. Ready
tasks remain queued when full and resume after a managed-worktree refresh
finds an available slot. Unlimited disables this capacity check.

When both cleanup switches are enabled, removal runs first and also closes the
workspace. Closing first and enabling removal later is unsupported by the
verified Herdr interface; OnTrack reports manual recovery guidance instead.
Automatic operations defer when native agent evidence is missing, and untouched
branches without task commits are never automatically cleaned up.
Herdr may attach an agent's native session after OnTrack sends the first prompt.
OnTrack records the initial lifecycle sequence and terminal at launch, then
binds the later session during reconciliation when the terminal and checkout
still match and the agent has finished. This also works for earlier handoffs
whose launch response retained those fields. Missing or changed evidence keeps
the worktree visible with a recovery reason instead of triggering cleanup.

Each setting change is saved immediately. On success, the active appearance and
the saved settings update together, and any startup or save error clears. On
failure, the active theme and saved file stay at their last successful values;
the dialog keeps the attempted choice and shows an error so you can retry it by
opening the list and pressing `Enter` / `Space`, or choose another theme.

## Recovery and failure policy

A missing settings file uses defaults without creating files or directories.
On startup, a load or path-resolution error uses defaults with a visible notice;
the existing file is not silently repaired or overwritten. To recover, fix the
JSON or move/remove the file, then restart. Check directory permissions and
available disk space if saving fails.

An explicit save creates missing parent directories. It writes a uniquely named
sibling temporary file, synchronizes and closes it, then atomically renames it
over the destination. Thus readers see the previous complete file or the new
complete file, not a partial write. Concurrent successful saves are last-writer
wins. Temporary files are created with owner-only permissions on Unix. On write
or rename failure, cleanup is attempted without masking the original error;
existing settings remain untouched. Directory creation may remain after failure,
and crashes or cleanup failures can leave a sibling `.settings-<pid>-<id>.tmp` file.
Such leftovers are not loaded and can be removed while OnTrack is closed.
Atomic replacement does not guarantee survival of a sudden power loss because
the containing directory is not explicitly synchronized.

## Developer interface

`src/settings.rs` exposes `Theme`, `DefaultAgent`, `AgentLimit`, `Settings`
(theme, default agent, five-agent default limit, a true-by-default child merge
preference, and three false-by-default worktree preferences), `config_path()`,
`load_from(path)`,
and `save_to(path, settings)`. The filesystem functions return `io::Result`;
invalid settings contents use `io::ErrorKind::InvalidData` with the file path and
reason. Loading never writes. Saving serializes the current supported fields
only, so unknown fields are accepted when reading but are not preserved on save.

The caller owns startup notices and popup state. Keep the selected candidate
separate from committed settings so a failed write cannot change the active or
saved theme. Update both only after `save_to` succeeds; closing Settings does
not roll back prior successful saves.
To add a theme, add a stable variant to `settings::Theme`, then add exactly one
`ThemeDefinition` to `theme::CATALOG` with a unique kebab-case ID, display name,
light/dark classification, and complete palette. Keep `Theme::ALL` in catalog
order, retain the existing `dark` and `light` IDs, and extend catalog,
round-trip, contrast, and renderer coverage. Use palette roles in `ui.rs` instead
of adding theme-specific conditions there.
Path-selection tests call a private pure helper rather than mutating process
environment variables. Persistence tests use isolated temporary directories:

```sh
cargo test settings::tests
```
