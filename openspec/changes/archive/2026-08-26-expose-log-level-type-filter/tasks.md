## 1. Engine — severity mode on LogView

- [x] 1.1 Add `SeverityFilter` enum (All, Error, Warn, Info, Debug, Unleveled) and store it on `LogView` (default All)
- [x] 1.2 Apply severity after include/exclude when rebuilding flat lines (`log_view` / `core/visible`)
- [x] 1.3 Carry optional `LogLevel` (or first-line cue flag) on `FlatLine` for viewport paint
- [x] 1.4 Add typed `Command` + JSON path to set severity on the active Tab/View; expose mode on stats
- [x] 1.5 Add `noviewlog-core` lib tests for Errors / Unleveled / pipeline order with include-exclude

## 2. Engine — Viewport severity cue

- [x] 2.1 Paint muted severity cue on first physical line of leveled Records in `viewport.rs` (no Theme.accent borders/strips)
- [x] 2.2 Ensure selection / copy text does not invent misleading characters if a glyph prefix is used (prefer tint or non-selectable gutter)

## 3. Slint — severity chrome

- [x] 3.1 Add severity selector control in Slint chrome (near Find/filter area)
- [x] 3.2 Wire control ↔ `engine_bridge` Command + stats sync for active Tab/View
- [x] 3.3 Confirm control uses Theme.border / soft tint only (no accent focus bar / accent borders)

## 4. Docs and verify

- [x] 4.1 Note severity filter order in `docs/architecture.md` (include/exclude then severity)
- [x] 4.2 Run `cargo test -p noviewlog-core --lib`
- [x] 4.3 Run `bash scripts/run-slint.sh` or `cargo build --release -p noviewlog-slint`
