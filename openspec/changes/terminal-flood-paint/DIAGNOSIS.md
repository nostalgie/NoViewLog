# Terminal flood paint — diagnosis addendum

Extends archived `pty-flood-resilience` DIAGNOSIS for this change.

## Shipped (Phase 1)

- Paint dirty under continuous flood at most every `VIEWPORT_PAINT_MIN_INTERVAL` (33 ms), with Follow snap on every ingest.
- Host ingest-only ticks + reused RGBA buffer.
- Idle echo / flood catch-up still dirties immediately (`more_pending == false`).

## Residual

| ID | Issue | Status |
|----|-------|--------|
| R7 | Full-frame fontdue at ~30 Hz still too heavy vs native strip scroll | **Deferred** — Follow strip-damage (blit + paint new bottom rows) only if Phase 1 manual `cat` still feels far from gnome-terminal/kitty |
| R8 | VTE/volatile rebuild dominates | Separate change if profiling confirms |
