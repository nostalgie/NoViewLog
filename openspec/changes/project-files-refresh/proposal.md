## Why

Opened log files are not saved with a Project, so nginx (and similar) logs have to be re-picked after every Project open. File content is a snapshot at open time; there is no control to reload the path from disk when the file grows or is rotated.

## What Changes

- While a Project is active, FILES sessions are persisted as Programs with `launch.log_file` (name, tabs, `program_id`) in `projects.yaml`.
- Opening a Project restores those FILES rows and **replaces** leftover FILES from the previous Project (**BREAKING** vs “FILES stay unchanged”).
- Selecting an unrestored file row loads it from disk; File / FILES `+` still loads immediately and then syncs.
- New `reload_file` command plus a FILES-row Refresh icon and File → Reload log (active file session). Manual reload, not tail/Follow.

## Capabilities

### New Capabilities

- (none)

### Modified Capabilities

- `terminals/projects`: Active Project snapshots and restores FILES as Programs with `launch.log_file`; open replaces FILES; still at least one live Terminal.
- `terminals/file-sessions`: `reload_file` re-reads the session path from disk; missing file reports status and keeps the session.
- `ui/files-sidebar`: FILES rows expose Refresh; File menu Reload log when a file session is active.

## Impact

- `noviewlog-core`: `engine/projects.rs` snapshot/restore, `engine/file_session.rs` reload, `Command::ReloadFile`, lazy load on `terminal_switch`.
- `noviewlog-slint`: FILES row refresh icon, File → Reload log. No `Theme.accent` chrome.
- Docs: `docs/terminals.md`.
- Verify: `cargo test -p noviewlog-core --lib`; `bash scripts/run-slint.sh` (or `cargo build --profile release-dev -p noviewlog-slint`).
