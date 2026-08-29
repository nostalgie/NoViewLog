## Why

After PTY flood backpressure and the HOST_TICK busy-loop fix, `cat` of a large log no longer freezes the UI, but Follow still feels hitchy versus a native terminal: every 256 KB ingest forces a full-frame fontdue paint and ASAP retick. Native terminals scroll a cell grid and paint mostly new bottom rows; we must decouple ingest rate from paint rate while keeping Follow scroll snapped on every ingest.

## What Changes

- On live Terminal PTY ingest: update scrollback/flat/index and snap Follow scroll every chunk, but mark Viewport paint dirty at display cadence (~16–33 ms), not every budgeted ingest.
- Host: allow ingest-only HOST_TICK passes while PTY work remains; ASAP retick drains ingest, not paints.
- Host: reuse one viewport RGBA buffer (resize only on geometry change).
- Residual (not required for v1 of this change): Follow strip-damage (blit + paint new bottom rows only).
- No scrollback cap raise; no mid-tick wake busy-loop; no “use FILES instead.”

## Capabilities

### New Capabilities

- (none)

### Modified Capabilities

- `engine/pty-flood`: paint cadence under flood; Follow snap on every ingest even when paint is deferred; host may ingest without painting every tick.

## Impact

- `crates/noviewlog-core` — `poll_pty` / dirty marking / optional paint-due API
- `crates/noviewlog-slint` — HOST_TICK ingest-only path + buffer reuse
- Tests in `pty_flood.rs`; release build of `noviewlog-slint`
