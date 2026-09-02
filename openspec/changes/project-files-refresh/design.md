## Context

See proposal.md. Projects persist in Engine `ProjectsStore` (`engine/projects.rs`, `~/.config/noviewlog/projects.yaml`). `LaunchConfig.log_file` already exists on `ProgramConfig`; snapshot currently skips `is_file_session()`. File load/reload lives in `engine/file_session.rs`. Slint FILES rows are `TerminalRow` in `ui/app.slint` with `show-run-stop: false`. ANSI coloring (`terminal.rs` vs `ansi.rs`) is not in scope.

## Goals / Non-Goals

**Goals:**

- Persist FILES as Programs with `log_file` while a Project is active.
- Replace FILES on Project open; lazy-load on select if not yet loaded.
- Explicit reload command + FILES Refresh / File → Reload log.

**Non-Goals:**

- Auto-tail / Follow for file sessions.
- Watcher that reloads without a click.
- Separate `files:` array in YAML (reuse Programs).
- Changing CLI `log_file` skip of Project restore.

## Decisions

1. **Same `programs` list, split in UI.** File Programs use `launch.log_file`; live Programs use `command`. Stats already split TERMINALS / FILES via `is_file_session()`. Snapshot writes live Programs first, then file Programs.

    Alternative considered: `ProjectConfig.files: Vec<…>` — extra schema for no UI gain.

2. **`project_open` drops leftover FILES.** Previous spec left FILES unchanged so switching Projects mixed logs. Replace to match “this Project’s files.”

    Alternative considered: merge leftover FILES into the opened Project — rejected; silent mix.

3. **Lazy load on `terminal_switch`.** `advance_file_load` only ticks the active session. Restored FILES start with `log_file` set and no `file_backed`; first select calls `start_log_file_load`. Open via dialog still loads immediately then `sync_active_project_from_terminals`.

    Alternative considered: load every file during `project_open` — N large logs on the active tick path.

4. **`Command::ReloadFile { terminal_id: Option<String> }`.** Switches to that file session and `start_log_file_load`. Not `Command::Start` (process launch stays view-only for files). Missing path: status error, session kept.

    Alternative considered: reuse `load_file` from the UI — would create a new session if the path string drifted; reload is bound to the row’s saved path.

5. **Refresh icon, not Play.** Play is Run for live Terminals. New `TerminalRowIcon` kind `refresh`, `Theme.text-muted` stroke, no accent border.

## Risks / Trade-offs

- [Large file on Refresh] → Same incremental `FileLoadState` tick path as first open.
- [Rotated log (new inode)] → Reload opens the path again; inode change is a new `FileLoadState`.
- [No Project active] → FILES stay session-only (same as extra Terminals not saved).

## Migration Plan

No YAML version bump: existing Projects have no file Programs. After this change, opening those Projects clears leftover FILES (intended). Rollback is revert; extra `log_file` Programs are ignored by old snapshot code that skipped files.
