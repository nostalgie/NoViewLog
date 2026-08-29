## 1. Engine — live overlay

- [x] 1.1 Stop putting the live VT screen into `RecordBuffer` (`feed` / `idle_flush` / `resize` / `ensure_live_screen`); commit only scrolled-off rows; `finish` still flushes the screen at exit
- [x] 1.2 Expose overlay `FlatLine`s from VTE cells/pens (no whole-screen ANSI round-trip); `volatile_count` = overlay height for caret
- [x] 1.3 Patch Terminal tab: incremental committed prefix + replace overlay; trim `dropped_h` from committed prefix only
- [x] 1.4 Map caret base to committed flat length + overlay line

## 2. Engine — cheap committed ingest

- [x] 2.1 Stamp `received_at` once per ingest chunk in `RecordParser::flush`; leave `level: None` on ingest
- [x] 2.2 Build Terminal tab `FlatLine` from the committed ANSI string once (no `rebuild_flat_lines_for_records` on that tail)
- [x] 2.3 Severity: `detect_level` on visible Terminal tab rows at paint; filter/severity views classify on rebuild
- [x] 2.4 If straightforward, drop `raw_lines` clone in `RecordBuffer::add` and walk `record.lines` in `set_format`; otherwise leave `raw_lines`

## 3. Tests and verify

- [x] 3.1 Update `volatile_patch.rs` / ingest tests: overlay not in `records_len`; echo patch; ring trim
- [x] 3.2 Update `pty_flood.rs`: flood still cadenced; filter tab does not see uncommitted spinner
- [x] 3.3 `cargo test -p noviewlog-core --lib`
- [x] 3.4 `cargo build --release -p noviewlog-slint`

## 4. Follow paints the VT grid (replaces overlay-in-LogView)

- [x] 4.1 Skip Terminal tab overlay/committed patch while Follow + running + no search
- [x] 4.2 Paint Follow from physical VTE rows (`grid_flat_lines`); caret from grid cell
- [x] 4.3 Materialize committed prefix + overlay when leaving Follow (scroll up / search / SetFollow off)
- [x] 4.4 Do not rebuild overlay on every `feed` (no ingest-time overlay cache)

## 5. Tests and verify (grid Follow)

- [x] 5.1 `cargo test -p noviewlog-core --lib`
- [x] 5.2 `cargo build --release -p noviewlog-slint`

## 6. Residuals (do not implement)

- [ ] 6.1 Phase B strip-damage blit of the RGBA framebuffer — still later
