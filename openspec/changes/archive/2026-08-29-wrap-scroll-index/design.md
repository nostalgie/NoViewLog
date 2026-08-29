## Context

See proposal.md. Mid-scroll Wrap ON currently walks from flat index 0 in [`collect_visible_visual_lines_with_total`](crates/noviewlog-core/src/viewport_layout.rs). Near-bottom Follow already uses an end-walk. Scroll commands do not rebuild flat lines — the cost is layout mapping + full paint.

## Goals / Non-Goals

**Goals:**
- O(1) total visual row count from a maintained index.
- O(log n) find flat line for `first_row`; then O(viewport) materialize visible wraps.
- Index updates only when flat lines / wrap / cell geometry change.
- Same path for Terminal scrollback and FILES windows (opening `big.log` must not jitter on mid-scroll).

**Non-Goals:**
- VT cell-grid damage compositor.
- Changing default Wrap OFF.
- PTY ingest/paint throttle changes.

## Decisions

1. **`VisualRowIndex`** — `prefix: Vec<u32>` where `prefix[i]` = cumulative visual rows after flat line `i` (exclusive end: `prefix[i]` = rows of lines `0..=i`). Empty lines contribute 1 row. Keyed by `(viewport_width, cell_width, wrap)`.
2. **Storage** — Own the index on `LogView` (alongside existing `visual_rows_cache`); rebuild lazily when key mismatches or flat content changes.
3. **Collect path** — Binary search `prefix` for `first_row`, start enumeration at that flat index; stop after `max_rows` visual lines. Drop the “walk from 0” mid-buffer path.
4. **Incremental** — On append: push new prefix entries. On prefix drop of `n` flats: subtract `prefix[n-1]` from remaining and drain. On volatile tail replace: truncate prefix to stable and re-append. Full rebuild if geometry/wrap changes.
5. **Wrap OFF** — Index optional / identity (`total = len`); keep slice collect.

## Risks / Trade-offs

- [Risk] Index desync with flat_lines → Mitigation: debug asserts in tests; rebuild on dirty flat.
- [Risk] Full rebuild O(n) on every PTY patch if incremental wrong → Mitigation: patch paths must extend/truncate prefix; measure under flood.

## Migration Plan

No user config migration. Addresses DIAGNOSIS R3 from pty-flood-resilience.
