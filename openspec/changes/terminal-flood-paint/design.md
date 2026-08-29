## Context

Live Terminal under `cat` (~80 MB) is interactive after R6 (no HOST_TICK busy-loop) but still paints a full fontdue frame per 256 KB ingest via ASAP `schedule_host_tick`. See archived `pty-flood-resilience` DIAGNOSIS R6.

## Goals / Non-Goals

**Goals:**
- Ingest budgeted PTY bytes as fast as the UI thread can without allocating/painting every chunk.
- Paint Viewport at most ~display rate (~30–60 Hz) while flooding.
- Snap Follow `scroll_offset_y` on **every** ingest so deferred paints still show the correct tail.
- Reuse RGBA framebuffer across painted frames.

**Non-Goals:**
- Full VT cell-grid compositor.
- Raising scrollback beyond existing 10k/30k clamp.
- FILES changes.
- Phase 2 strip-damage (documented residual only).

## Decisions

1. **Engine paint cadence** — Track `last_viewport_paint_at`. On active PTY ingest: always `snap_follow_scroll_after_ingest`; call `mark_viewport_dirty` only if `Instant::now() - last_paint >= PAINT_MIN_INTERVAL` (33 ms) **or** no prior paint / force paths (resize, tab switch, selection, Follow off scroll). Expose `take_viewport_painted()` / host calls `note_viewport_painted()` after successful render so the cadence clock advances.

2. **Ingest-only host ticks** — When `pty_work_pending` and `!needs_render()`, HOST_TICK still runs `tick()`/`poll_pty`, then schedules ASAP retick **without** allocating `SharedPixelBuffer` or calling `render`. When dirty, paint once, then continue drain via ASAP if pending.

3. **RGBA reuse** — Keep `RefCell<Option<(u32,u32,SharedPixelBuffer)>>` (or raw bytes) in the Slint host; recreate only when width/height change.

4. **Anti-stutter** — Never skip Follow snap when skipping paint. Old flood paint throttle failed because content advanced under a stale scroll; snap every ingest fixes that.

## Residual risks

| # | Hypothesis | Next lever |
|---|------------|------------|
| R7 | Full-frame fontdue still too heavy at 30 Hz under Follow | Phase 2: strip damage / bottom-only paint |
| R8 | VTE/volatile rebuild dominates vs paint | Live-screen short-circuit (separate change) |

## Risks / Trade-offs

- [Risk] Paint every 33 ms looks slightly less “live” than every chunk → Mitigation: 33 ms matches TICK_FAST; native terminals also vsync.
- [Risk] Ingest-only ticks starve paint if dirty never set → Mitigation: cadence forces dirty after interval; also dirty on non-flood paths unchanged.
