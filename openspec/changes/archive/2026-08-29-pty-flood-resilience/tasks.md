## 1. OpenSpec and docs

- [x] 1.1 Proposal, design, specs, tasks for `pty-flood-resilience`
- [x] 1.2 Note in `docs/terminals.md`: live floods use backpressure; large files → FILES
- [x] 1.3 `DIAGNOSIS.md` residual risks R1–R5 (anti-loop)
- [x] 1.4 Spec: scroll anchor + no paint throttle under flood

## 2. Engine — backpressure and tick budget

- [x] 2.1 Replace unbounded PTY `mpsc::channel` with bounded `sync_channel` (~1–2 MB pending)
- [x] 2.2 Cap coalesce/`feed` per `poll_pty` tick (256 KB); leave remainder queued
- [x] 2.3 Signal/retick while pending PTY bytes remain so Follow catches up

## 3. Engine — ring buffer and flat lines

- [x] 3.1 Convert `RecordBuffer` to `VecDeque` with O(dropped) trim
- [x] 3.2 Prefix-drop Terminal tab `flat_lines` on trim; keep volatile-tail patch
- [x] 3.3 Unit tests: trim under cap; volatile patch + trim; budget leaves remainder
- [x] 3.4 Anchor `scroll_offset_y` (and selection) when prefix drops and Follow is off

## 4. Engine — flood paint / Follow

- [x] 4.1 **Removed** flood paint throttle (always dirty on active ingest — throttle caused jumps)
- [x] 4.2 Incremental or cached visual-row totals for wrap+Follow under append/trim
- [x] 4.3 Slint: skip redundant `set_scroll_y` when unchanged

## 5. Verify

- [x] 5.1 `cargo test -p noviewlog-core --lib` (176 passed)
- [x] 5.2 `cargo build --release -p noviewlog-slint`
- [ ] 5.3 Manual: `cat ~/big.log` — Follow smooth; scroll up mid-stream stays anchored
