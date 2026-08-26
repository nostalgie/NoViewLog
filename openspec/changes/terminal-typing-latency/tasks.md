## 1. Host wake and coalesce

- [x] 1.1 Wire PTY reader → Slint wake (coalesced) so Bytes/Exit trigger a prompt tick
- [x] 1.2 Coalesce consecutive `Bytes` per terminal in `poll_pty` into one `feed`
- [x] 1.3 Stop clearing `last_stats_at` on pure byte bursts; keep 250 ms stats throttle
- [x] 1.4 Remove console-key `force_render`; keep `bump_fast_timer`

## 2. Caret scheduling

- [x] 2.1 `reset_caret_blink`: dirty only when turning caret back on
- [x] 2.2 Stop perpetual `TICK_FAST` from focused caret alone; schedule blink wakes ~530 ms

## 3. Incremental volatile flat lines

- [x] 3.1 On active PTY volatile update, patch Console flat-lines tail instead of `mark_all_views_dirty`
- [x] 3.2 Leave non-console views dirty for later switch
- [x] 3.3 Unit test: many single-byte feeds with large scrollback avoid full rebuild

## 4. Glyph cache

- [x] 4.1 Cache fontdue bitmaps in `ViewportRenderer`; invalidate on font-size change

## 5. Verify

- [x] 5.1 `cargo test -p noviewlog-core --lib`
- [x] 5.2 `cargo build --release -p noviewlog-slint`
