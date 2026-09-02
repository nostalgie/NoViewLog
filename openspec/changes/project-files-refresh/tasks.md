## 1. Engine persist and restore

- [x] 1.1 Snapshot FILES as Programs with `launch.log_file` in `sync_active_project_from_terminals` (live Programs first)
- [x] 1.2 `project_open` restores file Programs as FILES and replaces leftover FILES; keep at least one live Terminal
- [x] 1.3 Lazy-load on `terminal_switch` when a file session has `log_file` but no `file_backed` / `file_load`
- [x] 1.4 Sync after open/close/rename of file sessions while a Project is active

## 2. Engine reload

- [x] 2.1 Add `Command::ReloadFile { terminal_id: Option<String> }` and reload from the session path
- [x] 2.2 Tests: Project save/restore FILES; open Project replaces leftover files; reload picks up appended lines; missing path keeps session

## 3. Slint Refresh UI

- [x] 3.1 `TerminalRowIcon` kind `refresh` (muted stroke, no Theme.accent) and `show-refresh` on FILES rows
- [x] 3.2 File → Reload log when `is-file-session`; wire `reload_file` in `main.rs`

## 4. Docs and verify

- [x] 4.1 Update `docs/terminals.md` Persistence (FILES belong to the active Project; Refresh)
- [x] 4.2 `cargo test -p noviewlog-core --lib`
- [x] 4.3 `bash scripts/run-slint.sh` or `cargo build --profile release-dev -p noviewlog-slint`
