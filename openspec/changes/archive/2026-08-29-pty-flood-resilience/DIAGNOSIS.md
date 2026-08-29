# Terminal flood / scroll — diagnosis log

Purpose: avoid re-running the same loop. Update this when a hypothesis is confirmed or killed.

## Product bar

Live Terminal must feel like a normal OS terminal under floods (`cat` ~80 MB): UI interactive, scroll stable, Follow smooth. Filters/search are secondary chrome. Full-file browsing is FILES, not Terminal scrollback.

## Already shipped (do not redo as “the” fix)

1. Bounded PTY `sync_channel` + reader backpressure.
2. Per-tick ingest budget (256 KB) + hold/retick.
3. `VecDeque` ring trim (O(dropped)).
4. Flat-line prefix drop on trim.
5. **Scroll anchor** when prefix drops and Follow is off (`scroll_offset_y -= dropped_height`).
6. **Removed** flood paint throttle (always dirty on active ingest).
7. Slint: skip redundant `set_scroll_y` when unchanged (~0.5px).
8. **Follow snap on ingest** — pin `scroll_offset_y` to `max_scroll` right after PTY patch.
9. **Wrap scroll index** — [`wrap-scroll-index`](../wrap-scroll-index/) (mid-scroll Wrap ON is O(viewport)).
10. **FILES mid-drag black flashes** — global scrollbar ↔ window map (`scroll_file_to_global_offset`), stats global Y, prefetch step ≤ half window, paint clamp. **Confirmed fixed** (manual: flashes gone).

## Repro checklist

1. `cat ~/big.log` with Follow on — tail should advance without freezes or large visual teleports.
2. Same flood, disable Follow / scroll up mid-stream — line under the top edge should stay put as ring evicts.
3. Wrap ON and OFF for (2).
4. **Open `~/big.log` via FILES** — mid-history scroll without black jumps; scrollbar must reach EOF; status bar shows `current / total` lines.
5. If still bad after EOF fix: redesign FILES windowing from scratch (do not stack more knobs).

## Residual / open

| ID | Issue | Status |
|----|-------|--------|
| R3 | Wrap ON mid-scroll O(scrollback) | **Done** — wrap-scroll-index |
| R2 black | FILES mid-drag black flashes | **Done** — confirmed manually |
| R2 EOF | FILES scrollbar cannot reach / stick at EOF | **Done** — last-window pin, pending scroll_y update, visual max on last window, stats clamp |
| Line pos | Status bar current/total lines | **Done** — `viewport_line` / `viewport_line_total` → right-side status label |
| Thumb | V-scrollbar thumb dives under status at EOF | **Done** — `scrollbar_math` pixel travel |
| **R6** | `cat ~/big.log` freezes UI (~70% CPU, black viewport) while File→Open works | **Root cause:** HOST_TICK tight-looped ingest+paint when `poll_pty` woke mid-tick (`more_pending` → wake → `needs_retick` → `continue`). Starved Slint event loop. **Fix:** set `pty_drain_pending` instead of mid-tick wake; one pass per HOST_TICK; schedule next drain via event-loop invoke after yield. Terminal-first: stays interactive like a normal terminal under flood. |

Do **not** reopen flood throttle/budget or wrap-index for scrollbar issues. Redesign FILES windowing only if EOF/line position still fail after manual check.

**Product bar reminder:** Live Terminal is the primary surface. Fixes that make Terminal laggier than a normal OS terminal (busy-loop “drain”, paint throttle that skips Follow frames, etc.) are regressions even if FILES improves.
