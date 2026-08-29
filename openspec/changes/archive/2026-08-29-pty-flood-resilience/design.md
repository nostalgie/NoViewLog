## Context

See proposal.md — Why. Live PTY path: bounded `sync_channel` from [`pty.rs`](crates/noviewlog-core/src/pty.rs) → budgeted [`poll_pty`](crates/noviewlog-core/src/engine/terminal_lifecycle.rs) → VTE/`RecordBuffer`. FILES already budgets load and adjusts scroll on window swap; live Terminal must match that scroll-anchor behavior under ring eviction.

## Goals / Non-Goals

**Goals:**
- Writer backpressure through a bounded PTY queue.
- Bounded UI-thread ingest per tick with continued draining across ticks.
- O(dropped) ring trim and flat-line prefix drop under the existing scrollback cap.
- **Scroll stays anchored** when the ring drops a prefix (scrolled-up view must not slide).
- **No paint-rate throttle** that skips frames while content advances (causes Follow/scroll jumps).
- UI remains interactive during floods; Follow still converges smoothly.

**Non-Goals:**
- Raising live scrollback past 30k or retaining full `cat` output.
- Changing FILES sparse/match-index paths.
- Optimistic local echo.
- Full VT cell-damage / alternate grid compositor for Terminal tab (see Residual risks).

## Decisions

1. **Bounded sync channel** — `sync_channel` ~384 slots (~1.5 MB). Reader blocks on full → kernel PTY backpressure.

2. **Per-tick ingest budget** — Cap coalesce/`feed` at **256 KB**/tick (tuned down from 512 KB so each Follow paint is cheaper). Leave remainder in channel + `pty_hold`; set `pty_drain_pending` for the host. **Do not** call `pty_activity_wake` from `poll_pty` while draining — that made HOST_TICK busy-loop and froze the UI. Host: one ingest+paint pass, yield to Slint, then `invoke_from_event_loop` for the next chunk (Terminal stays interactive under `cat`).

3. **`VecDeque` RecordBuffer** — O(dropped) `pop_front` trim; `make_contiguous()` at slice accessors.

4. **Flat prefix drop + scroll anchor** — On `shifted_raw_lines > 0`, drop matching `flat_lines` prefix. **Before** the drop, measure visual height of that prefix (`count_visual_rows` × `row_stride`). If Follow is off, subtract that height from `scroll_offset_y` (clamp ≥ 0). Shift/clear selection line indices the same way. Mirrors FILES `scroll_adjust`.

5. **Always dirty on active ingest** — Do **not** throttle `mark_viewport_dirty` during flood. Skipping paints while ingest continues was observed as content/scroll stutter. Ingest budget alone limits hitch.

6. **Follow + wrap** — Wrap-off visual rows stay O(1); wrap-on Follow uses cached totals / bottom-up `collect_visible` when near the end.

## Residual risks (do not re-litigate blindly)

Documented so we do not loop the same fixes. If jitter remains after this change, check in order:

| # | Hypothesis | How to verify | Next lever (only if confirmed) |
|---|------------|---------------|--------------------------------|
| R1 | Scroll anchor wrong under Wrap ON (multi-row flat lines) | Reproduce with Wrap on, scroll up mid-`cat`, note line under top edge before/after trim | Compute dropped height from exact wrap of drained prefix (already attempted); add golden test with long lines |
| R2 | Slint scrollbar still fights engine (`set_scroll_y` vs user drag) | Log syncing_scroll + stats scroll while dragging mid-flood | Drive scroll only from engine while pointer down; or debounce stats scroll push |
| R3 | Wrap ON mid-scroll: O(scrollback) `first_row` walk each paint (feels like scroll is broken) | Scroll up mid-history with Wrap ON and large scrollback; CPU in layout walk before fontdue | **Fixed in** `wrap-scroll-index` (visual-row prefix index). Do not add throttle knobs. Residual full-frame fontdue under Follow floods is separate and only if index fix is still insufficient. |
| R4 | `rebuild_flat_lines` still firing every tick (patch failing) | Counter: patch success vs dirty rebuild under flood | Fix patch failure mode; never full-rebuild on trim-only |
| R5 | Product expectation is “see all of cat” | User wants full 80 MB in Terminal | Out of scope — FILES/`load_file`; do not raise 30k as a perf fix |

**Stop condition:** R3 (wrap mid-scroll O(scrollback)) is addressed by `wrap-scroll-index`. Do not stack more throttle/budget knobs. Only if that index fix is still insufficient under Follow floods, open a dedicated Terminal-tab damage/VT-grid change — not more flood knobs.

## Risks / Trade-offs

- [Risk] Bounded channel stalls `cat` → Mitigation: expected; matches native terminals; FILES for full-file browse.
- [Risk] 256 KB budget too small → choppy Follow → Mitigation: raise toward 512 KB only after measuring; do not reintroduce paint throttle.
- [Risk] Prefix flat drop desyncs multi-line records → Mitigation: fall back to dirty rebuild; Terminal tab first.
- [Risk] Always-paint under flood raises CPU → Mitigation: acceptable vs jumpiness; R3 is the escape hatch.

## Migration Plan

No user config migration. Deploy via normal binary update.
