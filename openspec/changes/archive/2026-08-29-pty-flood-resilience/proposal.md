## Why

`cat` of a modest (~80 MB) log in a live Terminal freezes the UI, while a normal Linux terminal stays responsive. The PTY reader drains unboundedly into an unbounded channel and the UI thread VT-ingests everything in one tick; scrollback trim is also O(n). Flood resilience is needed before large-file work can rely on the shell at all — FILES already windows multi-GB opens, but shell floods remain broken.

## What Changes

- Bound the PTY event queue so the reader blocks under load and the kernel stalls the writer (`cat`), like a real terminal.
- Cap PTY ingest work per UI tick; leave remainder queued and keep ticking until drained.
- Replace `RecordBuffer` front-`Vec` drain with a true ring (`VecDeque`) so trim at the scrollback cap is O(dropped).
- Drop matching `flat_lines` prefix on ring trim instead of full rebuild when only the head moved.
- Under flood: throttle viewport paints; keep Follow+wrap from O(n)-scanning all flat lines every dirty paint.
- Scrollback cap stays 10–30k; full-file viewing remains the FILES path.

## Capabilities

### New Capabilities

- `engine/pty-flood`: Live PTY backpressure, per-tick ingest budget, and scrollback ring behavior under output floods.

### Modified Capabilities

- (none)

## Impact

- `crates/noviewlog-core/` — `pty.rs`, engine PTY channel/`poll_pty`, `RecordBuffer`, flat-line patch path, viewport Follow/wrap accounting, paint dirtying under flood
- `crates/noviewlog-slint/` — only if retick/wake needs a host hook for pending PTY work (prefer engine-driven)
- Docs: `docs/terminals.md` note that live Terminal floods use backpressure; large files → FILES
- Verify: `cargo test -p noviewlog-core --lib`, `bash scripts/run-slint.sh` or `cargo build --release -p noviewlog-slint`
