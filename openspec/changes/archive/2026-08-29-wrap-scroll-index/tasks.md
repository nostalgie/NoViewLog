## 1. OpenSpec

- [x] 1.1 Proposal, design, specs, tasks for `wrap-scroll-index`
- [x] 1.2 Link R3 in `pty-flood-resilience/DIAGNOSIS.md` to this change

## 2. Engine — VisualRowIndex

- [x] 2.1 Implement `VisualRowIndex` (prefix sums, binary search, rebuild from flat lines)
- [x] 2.2 Store on `LogView`; invalidate on wrap/width/flat dirty
- [x] 2.3 Wire `cached_visual_rows` / collect mid-buffer through the index

## 3. Incremental updates

- [x] 3.1 Extend index on append; drop prefix on ring trim; patch volatile tail
- [x] 3.2 Unit tests: index matches naive count; mid collect does not visit all lines

## 4. Verify

- [x] 4.1 `cargo test -p noviewlog-core --lib`
- [x] 4.2 `cargo build --release -p noviewlog-slint`
- [ ] 4.3 Manual: Wrap ON, large Terminal scrollback, mid-history scrollbar/wheel usable
- [ ] 4.4 Manual: Open `~/big.log` via FILES, Wrap ON, mid-history scroll — no jump/jitter (same bar as Terminal)
