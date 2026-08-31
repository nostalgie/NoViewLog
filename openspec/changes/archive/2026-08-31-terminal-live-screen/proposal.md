## Why

Paint cadence under flood stopped the UI freeze, but `cat` of a large file still hitchs versus a native terminal: every budgeted ingest rebuilds the live VT screen as `LogRecord`s (`detect_level`, `Utc::now`, ANSI serialize then reparse into `FlatLine`). Native terminals keep the live screen in a cell grid and only commit scrolled-off rows. Phase A removes that ingest mismatch; strip-damage paint (B) and a cell-grid compositor (C) stay residuals.

## What Changes

- Live VT screen stays in the emulator grid. `RecordBuffer` receives only scrolled-off (committed) rows — not a volatile tail of overwrite records rebuilt on every chunk.
- Terminal tab visible lines = committed scrollback prefix + a live overlay built from VTE cells (no `LogRecord` for the overlay). Overlay replaces in place (echo, prompt, spinners) without a full scrollback rebuild.
- Filter tabs see committed records only. In-place spinner frames stay on the Terminal tab until they scroll off.
- Cheap committed ingest: one timestamp per ingest chunk; skip `detect_level` on the firehose (severity on visible paint or filter-tab rebuild); build Terminal tab `FlatLine` from the committed ANSI string once (no strip+parse again).
- Live overlay: cells/pens → `TextSegment`s directly (no whole-screen ANSI round-trip).
- Residual (not this change): Follow strip-damage blit (B). Follow Terminal tab now paints the VTE cell grid instead of stuffing overlay `FlatLine`s into LogView (that path caused Follow jumps and ~1 core of CPU on `cat`).
- No scrollback cap raise; no FILES changes; paint cadence from `terminal-flood-paint` stays.

## Capabilities

### New Capabilities

- (none)

### Modified Capabilities

- `engine/pty-flood`: live screen is not stored as Records; committed-only buffer; cheap firehose ingest; Terminal tab overlay; filter tabs committed-only.
- `engine/console-latency`: incremental Terminal tab update is an overlay replace (not a volatile Record tail in the buffer); still no full-scrollback rebuild on echo.

## Impact

- `crates/noviewlog-core` — `TerminalIngest`, `RecordParser`, `LogView` patch, `poll_pty` / caret mapping; optional `RecordBuffer` `raw_lines` clone drop
- Tests in `volatile_patch.rs`, `pty_flood.rs`, ingest tests in `terminal.rs`
- Verify: `cargo test -p noviewlog-core --lib` and `cargo build --release -p noviewlog-slint`
- Docs: `docs/architecture.md` Dual ANSI paths / Terminal ingest if wording still implies volatile Records in the buffer
