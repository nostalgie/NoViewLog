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
| `launch` | Optional saved command / log file for ▶ Start / reload |
| `running` | Whether this session’s PTY child is alive (always false for files) |

Multiple sessions can run **at the same time**. Switching the active session only changes the viewport — it does **not** stop other PTYs.

### Live terminals

- Ring-buffer scrollback (`max_scrollback_lines`, default 10k, max 30k)
- Follow mode, keyboard → stdin, interactive shell / launched process
- Primary tab display name: **Terminal**

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

There is **no** `projects.yaml` session store. Filter presets, scrollback, viewport font, and sidebar section expand state live in `~/.config/noviewlog/config.yaml`.

## UI chrome

Slint sidebar: collapsible **TERMINALS** and **FILES** (each with `+`). TERMINALS supports DnD reorder. Follow is hidden for file sessions. See `docs/architecture.md`.
