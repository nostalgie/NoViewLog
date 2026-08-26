## 1. Engine — collapse state and flat lines

- [x] 1.1 Add per-`LogView` expand tracking (`expanded_record_ids`) and rebuild helpers
- [x] 1.2 Emit collapsed preview `FlatLine` (first line + hidden count + disclosure metadata) for multiline Records not in the expand set
- [x] 1.3 Add Commands: toggle record, expand-all (current filtered multiline ids), collapse-all; expose needed stats
- [x] 1.4 Auto-expand a Record when search-goto targets a match on a hidden line
- [x] 1.5 Lib tests: default collapsed, toggle, expand-all/collapse-all, exclude on full text, search-goto auto-expand

## 2. Engine — Viewport paint and hit-test

- [x] 2.1 Paint muted disclosure cue for collapsible Records (collapsed vs expanded distinguishable; no Theme.accent borders/strips)
- [x] 2.2 Map pointer hits on disclosure / collapsed preview to toggle Command
- [x] 2.3 Keep ANSI segment paint for preview first line via existing `ansi.rs` path

## 3. Slint — chrome actions

- [x] 3.1 Add Expand all / Collapse all actions in Slint chrome or View menu
- [x] 3.2 Wire actions and viewport clicks through `engine_bridge`
- [x] 3.3 Confirm no accent border / accent drop chrome on new controls

## 4. Docs and verify

- [x] 4.1 Document Record collapse behavior in `docs/architecture.md`
- [x] 4.2 Run `cargo test -p noviewlog-core --lib`
- [x] 4.3 Run `bash scripts/run-slint.sh` or `cargo build --release -p noviewlog-slint`
