## Why

Holding a key in the Terminal tab feels several times slower than a native Ubuntu terminal. Stdin already writes to the PTY immediately; the lag is display — echo waits on a ~33 ms UI tick, then pays for a full flat-line rebuild and full-frame fontdue paint. A slow Terminal tab makes the product feel unusable.

## What Changes

- Wake the Slint host when PTY bytes/exit arrive so echo is not capped by the fast timer alone.
- Coalesce per-tick PTY `Bytes` feeds and keep stats throttled during byte bursts.
- Stop forcing a full viewport paint on every key before echo; tighten caret blink dirtying and scheduling.
- Update Terminal tab flat lines incrementally for volatile VT tail changes instead of full scrollback rebuilds.
- Add a fontdue glyph cache in the viewport painter so repeated full-frame paints are cheaper.

## Capabilities

### New Capabilities

- `engine/console-latency`: Terminal tab typing/echo display latency and incremental volatile updates for the live screen.

### Modified Capabilities

- (none)

## Impact

- `crates/noviewlog-core/` — PTY poll/ingest, flat-line rebuild, caret blink, viewport glyph cache
- `crates/noviewlog-slint/` — timer/wake on PTY activity, key path `force_render`, caret urgency
- Verify: `cargo test -p noviewlog-core --lib`, `cargo build --release -p noviewlog-slint`
