## Why

Terminals are ephemeral: launch commands, cwd, and filter tabs are lost when the app restarts. Users need named Projects that group Programs (saved launch + tabs) and restore them on open, with explicit Run/Stop instead of auto-start.

## What Changes

- Persist Projects / Programs in `~/.config/noviewlog/projects.yaml` (existing serde shapes).
- Open a Project replaces TERMINALS with one stopped Terminal per Program (launch + filter tabs); FILES unchanged.
- Run starts saved command (or interactive shell); Stop kills that session’s PTY.
- Programs with a saved `command` stay stopped after Stop or process exit (no auto shell respawn).
- PROJECTS sidebar section: list / open / create / rename / delete.
- Terminal rows gain Run/Stop; minimal Edit Launch (command, args, cwd).
- WSL UI / fixes deferred to Phase 2 (types and resolve path remain).

## Capabilities

### New Capabilities

- `terminals/projects`: Project open/create/save, Program ↔ Terminal restore, Run/Stop lifecycle rules
- `ui/projects-sidebar`: PROJECTS section chrome, Run/Stop on terminal rows, Edit Launch

### Modified Capabilities

- (none — file-sessions close rules unchanged; last live terminal still protected)

## Impact

- `noviewlog-core`: Engine holds `ProjectsStore`; new Commands; terminal lifecycle; stats; tests; `docs/terminals.md`
- `noviewlog-slint`: PROJECTS sidebar, Run/Stop, Edit Launch, bridge
- Verify: `cargo test -p noviewlog-core --lib`, `cargo build --release -p noviewlog-slint`
