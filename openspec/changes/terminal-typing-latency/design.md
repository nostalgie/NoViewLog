## Context

Terminal tab keys write to the PTY immediately. Display waits for shell echo drained on a Slint timer (`TICK_FAST` ≈ 33 ms), then runs `mark_all_views_dirty` → full `rebuild_flat_lines` and a full-frame fontdue paint with no glyph cache. Focused caret keeps the fast timer forever. Stats throttle is cleared on every PTY byte burst.

## Goals / Non-Goals

**Goals:**
- Echo paints without being capped solely by the 33 ms timer.
- Coalesce PTY feeds and avoid full scrollback flat rebuilds on volatile-only updates.
- Stop caret/key paths from forcing redundant full paints; idle caret without perpetual 30 Hz paints.
- Glyph cache so remaining full paints are cheap under auto-repeat.

**Non-Goals:**
- Optimistic local echo.
- Full cell-damage VT compositor.
- Changing `MIN_PTY_COLS` or dropping per-key `flush()` unless still needed after the above.

## Decisions

1. **PTY wake** — PTY reader notifies the Slint host (callback / `invoke_from_event_loop` / 0 ms single-shot) when Bytes/Exit are posted so `tick` runs promptly.
2. **Coalesce feeds** — In `poll_pty`, concatenate consecutive `Bytes` per terminal id (or feed once after merge) before `TerminalIngest::feed`.
3. **Stats** — Do not clear `last_stats_at` on pure byte bursts; clear only on chrome-relevant changes (cwd/exit/running).
4. **Keys** — Remove `force_render` on every Terminal tab key; keep `bump_fast_timer`. `reset_caret_blink` dirties only if caret was off.
5. **Caret schedule** — Drop perpetual `caret_urgency` → `TICK_FAST`; schedule blink wakes on ~530 ms; idle at `TICK_IDLE` when not dirty.
6. **Incremental flat lines** — On volatile-only active updates, patch Terminal tab `flat_lines` tail; leave other views dirty for later switch.
7. **Glyph cache** — `(font_id, glyph_index, px_size)` → bitmap in `ViewportRenderer`; clear on font-size change.

## Risks / Trade-offs

- Wake storms under heavy PTY output: coalesce wakes to one pending invoke.
- Incremental tail patch must stay consistent with `volatile_count` strip/restore; cover with unit tests.
- Glyph cache memory: bound or rely on modest mono glyph set at one size.
