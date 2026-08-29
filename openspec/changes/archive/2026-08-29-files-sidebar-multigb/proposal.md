## Why

Live PTY sessions and multi-gigabyte log files share one sidebar list and one memory model (ring buffer / sliding window ≤30k lines). Filters and search only see that window, so opening large logs cannot be a first-class job. We need a clear FILES surface and a file path that scales without copying the whole log into RAM.

## What Changes

- Split the sidebar into collapsible **TERMINALS** and **FILES** sections; FILES `+` opens a log file.
- File sessions no longer appear under TERMINALS; tab 0 shows the file basename (not `"Terminal"`).
- Disable / hide Follow for file sessions.
- Live terminals keep the existing scrollback ring (default 10k, max 30k).
- Replace the full in-RAM per-line offset vector with a sparse / on-disk line index so multi-GB files can open.
- File filter tabs (and search) use a **match index**: whole-file scan → byte offsets of matches → viewport seeks/reads from the original file (no derived full-text copy in v1).
- Update `docs/terminals.md` for the dual sidebar model.

Affected crates: **noviewlog-core** (engine, file index, filters) and **noviewlog-slint** (sidebar chrome).

Verify: `cargo test -p noviewlog-core --lib`; `cargo build --release -p noviewlog-slint` or `bash scripts/run-slint.sh`.

## Capabilities

### New Capabilities

- `ui/files-sidebar`: Collapsible TERMINALS and FILES sections; FILES list and open affordance.
- `engine/file-match-index`: Whole-file match index for filter/search on file sessions; sparse/on-disk line index for multi-GB open.
- `terminals/file-sessions`: File session UX vs live terminals (tab naming, no Follow, close rules, stats split).

### Modified Capabilities

- (none — no existing REQUIREMENTS change; new capabilities cover the behavior)

## Impact

- Engine: `terminal_state`, `file_index`, `file_load`, `engine/file_session`, `engine/stats`, `log_view`, filter rebuild path for files.
- Slint: sidebar in `ui/app.slint`, models/callbacks in `main.rs`.
- Docs: `docs/terminals.md` (and architecture notes if needed).
- Config: optional persist of section collapse state in `AppConfig`.
