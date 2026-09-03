# Terminals and files model

## Overview

NoViewLog manages **sessions** in two sidebar sections:

```
TERMINALS (collapsible)     live PTY shell / process
FILES (collapsible)         read-only log file
 └── Tab (primary + filter tabs)
```

| Field | Notes |
|-------|--------|
| `id` | Stable session id (`terminal-…`) |
| `cwd` | Live working directory (spawn cwd, OSC 7); for files, the opened file’s parent directory |
| `label` | Sidebar label: optional **custom title**, else cwd segment / **file basename** |
| `launch` | Optional saved command / log file for Start / reload |
| `running` | Whether this session’s PTY child is alive (always false for files) |

Multiple sessions can run **at the same time**. Switching the active session only changes the viewport — it does **not** stop other PTYs.

### Live terminals

- Ring-buffer scrollback (`max_scrollback_lines`, default 10k, max 30k)
- Follow mode, keyboard → stdin, interactive shell / launched process
- While Follow is on, the Terminal tab Viewport paints the live VT cell grid
  (not a LogView overlay). Scrollback is composed into the Terminal tab only
  when Follow is off (scroll up, search, process exit). Filter tabs see
  committed Records only.
- Primary tab display name: **Terminal**
- Live PTY output uses a **bounded queue** and per-tick ingest budget so floods
  (`cat` of tens of MB, chatty builds) stall the writer instead of freezing the UI.
  Scrollback still caps at 10–30k — open large logs via **FILES** / `load_file`
  to browse the whole file.
- When the ring drops oldest lines and Follow is off, scroll position is **anchored**
  (offset shrinks with the dropped height) so scrolled-up content does not slide.
  Diagnosis / residual risks for remaining jank:
  [`openspec/changes/archive/2026-08-29-pty-flood-resilience/DIAGNOSIS.md`](../openspec/changes/archive/2026-08-29-pty-flood-resilience/DIAGNOSIS.md).

### File sessions

`load_file` / FILES `+` opens (or switches to) a **dedicated** file session:

- Appears under **FILES**, never under TERMINALS
- No PTY, no keyboard → stdin, no Follow
- Primary tab display name: **file basename** (not `Terminal`)
- Filter tabs and search apply over the file (match-index path for whole-file filters)
- Re-opening the same path switches to the existing file session and reloads
- Closing a file is always allowed; the last **live** terminal cannot be closed; FILES may be empty

CLI launch with a log file still configures the initial session as a file session.

## Commands

| Command | Behavior |
|---------|----------|
| `terminal_add` | Create a live terminal and start an interactive shell; switch to it |
| `terminal_switch` | Change active session + dirty viewport (no PTY stop) |
| `terminal_close` | Stop that id’s PTY if any and remove it; refuses closing the last live terminal |
| `terminal_move` | Reorder sessions (Slint DnD within TERMINALS) |
| `terminal_rename` | Set a **custom** sidebar title for a session id |
| `terminal_start` | Start saved launch / log file / interactive shell on a session |
| `load_file` | Open path in a dedicated file session (or switch + reload) |
| `reload_file` | Re-read the file session’s saved path from disk (optional `terminal_id`) |
| `set_sidebar_expanded` | Persist TERMINALS/FILES section expand state |
| `tab_move` | Reorder filter tabs on the active session; primary tab stays at index 0 |

## Engine shape

- `terminals: Vec<TerminalState>` (unified; kind via `is_file_session()`)
- `active_terminal: usize`
- `ptys: HashMap<String, PtyManager>` — one manager per running live session id
- Stats expose separate `terminals` and `files` lists for the sidebar
- File sessions: `file_load` / `file_backed` / `launch.log_file` → `is_file_session()`

## Stats (UI)

```json
{
  "terminals": [{ "id", "label", "running", "cwd", "index" }],
  "files": [{ "id", "label", "running", "cwd", "index" }],
  "active_terminal": 0,
  "is_file_session": false,
  "terminals_section_expanded": true,
  "files_section_expanded": true,
  "terminal_id": "…",
  "terminal_label": "…",
  "has_active_terminal": true
}
```

## Persistence

Filter presets, scrollback, viewport font, and sidebar section expand state live in
`~/.config/noviewlog/config.yaml`.

**Projects / Programs** are stored in `~/.config/noviewlog/projects.yaml`:

- A **Project** groups one or more **Programs**
- Each Program saves launch (`command` / `args` / `cwd` **or** `log_file`) and filter tabs
- Manage Projects via **File → Projects…** (open, rename, delete, or create)
- **New** creates an empty Project (one live Terminal that auto-starts an interactive
  shell; no copied launch or filter tabs from the previous Project)
- Opening a Project (File → Projects, or last Project on app startup) **replaces**
  TERMINALS and FILES with that Project’s Programs
- CLI launch (`command` or log file) takes priority over project restore

### Project open / cold start (mandatory UX)

Same on Linux and Windows:

1. **Tab strip:** every restored **live** TERMINALS session opens on the primary
   **Terminal** tab (`active_view == 0`), even if `projects.yaml` saved a filter
   tab as `active_tab`. Filter tabs remain in the strip; the user can switch to
   them after output exists. FILES may keep a restored filter tab.
2. **Auto-start:** opening/restoring a Project **starts sessions without pressing
   Start**:
   - Live Program with a saved `command` → start that process
   - Live Program with no command → start an interactive shell
   - FILES Program → begin loading the log file
3. Empty viewport hints (stopped / empty buffer only) are ASCII and point at the
   TERMINALS row Start control if a filter tab is somehow still empty — they are
   not a substitute for auto-start on Project open.

### After Start / Stop

- Manual **Start** on a TERMINALS row still starts saved command / interactive shell
- **Stop** kills that PTY; Programs with a saved command stay stopped after Stop or
  process exit (no auto shell respawn)
- **Refresh** on a FILES row (or File → Reload log) re-reads that path from disk;
  it is not Follow/tail

While a Project is active, opening, renaming, or closing a file session updates that Project’s store. With no Project active, FILES stay session-only.

## UI chrome

Slint sidebar: collapsible **TERMINALS** and **FILES** (each with `+`).

- Section markers are colored discs (`SectionDot`): **accent** for TERMINALS,
  **include** green for FILES — never Unicode chevrons/dots (Windows tofu).
- Row status discs use the same colors (TERMINALS: accent when running, muted
  when stopped; FILES: include).
- Action icons are **Path** geometry (`TerminalRowIcon`: edit, play/stop,
  refresh, close). Dialogs use **`AppButton`**, not fluent `Button`.
- Chrome text uses bundled **Noto Sans**; the log viewport bitmap uses mono
  (bundled Noto Sans Mono / system Cascadia when available).
- Inline rename: TERMINALS rows and filter tabs only — **FILES rows cannot be
  renamed**. Click-away (including empty sidebar space) ends rename.
- When a Project is open, its name appears above TERMINALS. TERMINALS supports
  DnD reorder. Follow is hidden for file sessions. Projects are managed from
  **File → Projects…**, not the sidebar.

See [`docs/architecture.md`](architecture.md),
[`openspec/specs/terminals/projects/spec.md`](../openspec/specs/terminals/projects/spec.md),
and [`.cursor/rules/`](../.cursor/rules/).
