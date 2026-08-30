## Context

See proposal.md for motivation. Dual ANSI paths: live VT coloring is owned by `crates/noviewlog-core/src/core/terminal.rs` (vte grid); stored-line SGR is `core/ansi.rs`. This change stops re-serializing the live grid into Records so `ansi.rs` can parse it again. Slint host (`noviewlog-slint`) stays on the existing paint-cadence / ingest-only HOST_TICK path; no new host API is required if overlay lands in Terminal tab `flat_lines` before `render`.

Today `TerminalIngest::feed` strips a volatile Record tail, advances VTE, commits scrolled-off rows through `RecordParser`, then restores the whole screen as overwrite `LogRecord`s (`detect_level`, `Utc::now` per line). `LogView::try_patch_volatile_tail` rebuilds that tail via `rebuild_flat_lines_for_records` (strip + parse ANSI again). Almost every line of a large `cat` is committed, so the firehose cost is parser flush + double ANSI + `raw_lines` clone, not only the overlay.

## Goals / Non-Goals

**Goals:**
- Live screen never enters `RecordBuffer`.
- Terminal tab = incremental committed prefix + VTE overlay (cell → segments).
- Echo still O(overlay), not O(scrollback).
- Committed ingest cheap enough that `cat` is dominated by VTE + one serialize per scrolled-off row, not per-line clocks and reparse.

**Non-Goals:**
- Follow strip-damage / framebuffer blit (phase B).
- Raising scrollback cap; FILES; host paint-cadence changes.

## Decisions

1. **Follow paints the VTE grid, not LogView overlay** — Overlay-in-`flat_lines` on every ingest caused Follow jumps (max_scroll oscillating) and pinned a CPU core (`cat` vs native). While Follow is on, ingest only advances VTE + committed Records; paint is the physical cell grid. Scroll-up / search / exit materializes committed prefix + overlay into the Terminal tab. Alternative: keep patching overlay into LogView — rejected after the jump/CPU regression.

2. **Filter tabs stay Record-only** — They rebuild from `RecordBuffer` (committed). Spinners do not animate there until rows commit. Alternative: still inject volatile Records for filter tabs — kills the ingest win and reintroduces strip/restore.

3. **Live overlay from cells/pens** — `Row` already has `Cell { ch, pen }`. Build `TextSegment`s (and plain `raw`) from the grid for the overlay. `Row::serialize` remains for **committed** rows (one ANSI string into the parser/buffer). Alternative: `screen_lines()` + `parse_ansi_line` for overlay — simpler, keeps the round-trip we are removing.

4. **Cheap `RecordParser::flush`** — Stamp `received_at` from a chunk-level time passed into `feed` (or set on the parser at the start of `poll_pty`), not `Utc::now()` per record. Leave `level: None` on ingest. Terminal tab severity gutter: `detect_level` on visible rows at paint (O(viewport)). Filter/severity views: `detect_level` when they rebuild. Alternative: keep ingest-time `detect_level` — still O(lines) on `cat`.

5. **Terminal tab committed append** — When rows commit, parse ANSI once into `FlatLine` and extend the committed prefix; then replace overlay. Do not call `rebuild_flat_lines_for_records` on the new tail. `flat_lines_record_cursor` tracks committed `records_len()` only (overlay is not in the cursor).

6. **`raw_lines` clone** — Try to drop `record.lines.iter().cloned()` in `RecordBuffer::add` by walking `record.lines` in `set_format` reparse. If tests or file-window code still need a parallel ring, keep `raw_lines` and skip this sub-item rather than blocking A.

7. **Caret** — `terminal_caret_rect` base index = committed flat length (not `len - volatile_count` from buffer). Overlay line `i` corresponds to live screen line `i`.

8. **`ensure_live_screen` / empty prompt** — Overlay can be empty-screen rows from the emulator without inserting Records. `finish()` still `flush_all` into the buffer at process exit.

## Risks / Trade-offs

- [Risk] Filter tabs miss in-place UI (ora/listr) until scroll-off → Mitigation: documented; Terminal tab remains the interactive surface.
- [Risk] Severity gutter lags or misses on Terminal tab if paint-time detect is skipped → Mitigation: detect on visible overlay + visible committed rows in `draw_visual_line` / render prep.
- [Risk] Patch cursor vs overlay length bugs (caret, trim, wrap index) → Mitigation: tests in `volatile_patch.rs` / ingest: overlay not in `records_len`; echo patch; ring trim + Follow.
- [Risk] Dropping `raw_lines` breaks format reparse or FILES helpers → Mitigation: change `set_format` first; revert clone-drop if anything still needs the parallel ring.

## Residual (not this change)

| ID | Issue | Next |
|----|-------|------|
| B / R7 | Full-frame fontdue of the live grid vs native strip scroll | Follow blit + bottom-row paint |

## Migration Plan

No user-config migration. Behavior change: filter tabs no longer show uncommitted live frames. Rollback is reverting the change.
