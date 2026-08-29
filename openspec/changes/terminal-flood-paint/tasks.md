## 1. Engine paint cadence

- [x] 1.1 Add `PAINT_MIN_INTERVAL` (~33 ms) and `last_viewport_paint_at` on `Engine`
- [x] 1.2 On active PTY ingest: always snap Follow; mark dirty only when paint interval elapsed (or force-immediate paths)
- [x] 1.3 `note_viewport_painted()` after successful render; ensure flood-end / idle echo still dirties promptly
- [x] 1.4 Unit tests: Follow scroll at max across many budgeted ticks; dirty rate bounded under synthetic flood

## 2. Slint host

- [x] 2.1 HOST_TICK: ingest-only path when `!needs_render()` but `pty_work_pending`
- [x] 2.2 Reuse viewport `SharedPixelBuffer` across paints; recreate on size change
- [x] 2.3 ASAP `schedule_host_tick` for drain; paint only when dirty

## 3. Verify

- [x] 3.1 `cargo test -p noviewlog-core --lib pty_flood`
- [x] 3.2 `cargo build --release -p noviewlog-slint`
- [x] 3.3 Phase 2 strip-damage left as residual (R7): ship Phase 1 first; open only if manual `cat` still far from native Follow feel
