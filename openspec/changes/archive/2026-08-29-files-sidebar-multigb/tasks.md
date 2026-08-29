## 1. Engine stats and file session UX

- [x] 1.1 Split stats into live `terminals` and `files` lists (unified `TerminalState` vec; filter by `is_file_session`)
- [x] 1.2 Ensure `load_file` / open always creates or switches a file session that never appears under TERMINALS
- [x] 1.3 Primary tab (index 0) for file sessions uses file basename instead of `Terminal`
- [x] 1.4 Force `auto_follow = false` for file sessions; ignore Follow commands when active session is a file
- [x] 1.5 Close rules: any file closable; refuse closing the last live terminal; FILES may be empty
- [x] 1.6 Core tests for stats split, tab name, follow, close rules

## 2. Slint FILES sidebar

- [x] 2.1 Add collapsible TERMINALS and FILES section headers (persist expand state in AppConfig if practical)
- [x] 2.2 Bind `files-model`; FILES `+` opens log file dialog / LoadFile
- [x] 2.3 Terminal and file row actions (switch, close, rename where applicable) use the correct list
- [x] 2.4 Hide Follow chrome when active session is a file; show filename on tab 0
- [x] 2.5 Update `docs/terminals.md` for dual sidebar model

## 3. Sparse / on-disk line index

- [x] 3.1 Replace dense full-file `LineIndex` with sparse checkpoints and/or on-disk index
- [x] 3.2 Adapt file window seek/read and prefetch to the sparse index
- [x] 3.3 Core tests for large-file open/scroll without O(lines) dense RAM vector

## 4. Match-index filters and search

- [x] 4.1 Background whole-file scan → `match_offsets` per file filter tab (include/exclude/severity)
- [x] 4.2 Filtered viewport scrolls by match ordinal; seek/read from original file
- [x] 4.3 Cancel/rescan on rule change; empty match set UX
- [ ] 4.4 Whole-file search via match-index path
- [x] 4.5 Core tests for sparse include across windows and search outside window

## 5. Verify

- [x] 5.1 `cargo test -p noviewlog-core --lib`
- [x] 5.2 `cargo build --release -p noviewlog-slint` or `bash scripts/run-slint.sh`
