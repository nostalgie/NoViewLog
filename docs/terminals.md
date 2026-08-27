# Terminals model

## Overview

NoViewLog manages **terminals** — independent sessions:

```
Terminal (shell / process / log file + cwd or file label)
 └── Tab (Terminal tab + filter tabs)
```

Each terminal:

| Field | Notes |
|-------|--------|
| `id` | Stable session id (`terminal-…`) |
| `cwd` | Live working directory (spawn cwd, updated via OSC 7 when the shell reports it); for file sessions, the opened file’s parent directory |
| `label` | Sidebar label: optional **custom title** (via `terminal_rename`), else last path segment of `cwd` (`~` for home), or the log **file basename** for file sessions |
| `launch` | Optional saved command / log file for ▶ Start / reload |
| `running` | Whether this terminal’s PTY child is alive |

Multiple terminals can run **at the same time**. Switching the active terminal only changes the viewport — it does **not** stop other PTYs.

### File terminals

`load_file` opens (or switches to) a **dedicated** terminal for that path:

- Does **not** replace an interactive shell’s buffer
- No PTY, no keyboard → stdin, no process Start
- Filter tabs, search, wrap, follow, and scroll work as usual
- Re-opening the same path switches to the existing file terminal and reloads
- Closing a file terminal (sidebar) is the same as closing any non-first terminal

CLI launch with a log file still configures the initial terminal as a file session (no separate empty shell left behind).

## Commands

| Command | Behavior |
|---------|----------|
| `terminal_add` | Create a terminal and start an interactive shell; switch to it |
| `terminal_switch` | Change `active_terminal` + dirty viewport (no PTY stop) |
| `terminal_close` | Stop that id’s PTY and remove it; **refuses index 0** (first terminal) |
| `terminal_move` | Reorder sidebar without stopping PTYs (Slint DnD) |
| `terminal_rename` | Set a **custom** sidebar title for a terminal id (trim; empty/unknown = no-op; does not clear an existing custom title) |
| `terminal_start` | Start saved launch / log file / interactive shell on a terminal |
| `load_file` | Open path in a dedicated file terminal (or switch + reload) |
| `tab_move` | Reorder filter tabs on the active terminal; **the Terminal tab stays at index 0** |

## Engine shape

- `terminals: Vec<TerminalState>`
- `active_terminal: usize`
- `ptys: HashMap<String, PtyManager>` — one manager per running terminal id
- Shared `pty_tx` / `pty_rx`; `PtyEvent::{Bytes,Exit}` carry `id` so output is routed to the matching terminal
- File sessions: `file_load` / `file_backed` / `launch.log_file` → `is_file_session()`

Geometry (`resize`) updates **all** PTYs in the map. OSC 7 cwd updates apply via `TerminalIngest::take_cwd_update()`.

## Stats (UI)

Consumed by the Slint UI:

```json
{
  "terminals": [{ "id", "label", "running", "cwd", "index" }],
  "active_terminal": 0,
  "terminal_id": "…",
  "terminal_label": "…",
  "has_active_terminal": true
}
```

## Persistence

There is **no** `projects.yaml` session store for terminals. Filter presets and user config remain in `~/.config/noviewlog/config.yaml`. Legacy `ProjectConfig` / `ProgramConfig` types may still exist for YAML round-trips but are unused by the engine.

## UI chrome

Slint sidebar **TERMINALS** list supports **drag-and-drop reorder** (engine
`terminal_move`; PTYs keep running) and **double-click inline rename** (engine
`terminal_rename`; custom title overrides the auto cwd/file label for the
session). The tab strip supports DnD among **filter tabs**; the Terminal tab
stays pinned at index 0 and cannot be dragged or dropped onto. File → **Open log
file**. See `docs/architecture.md`.
