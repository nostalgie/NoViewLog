## Why

NoViewLog already groups multiline stacks into one Record for filtering, but the Viewport still paints every physical line expanded. Dense live output stays hard to scan. CloudWatch-style collapsible entries (preview → expand) are a major readability win in Better Terminal Logs and fit naturally on top of our Record model.

## What Changes

- Multiline Records (≥2 physical lines) can collapse to a one-line preview plus a disclosure cue and hidden-line count.
- Users can expand/collapse individual Records and expand-all / collapse-all for the active Tab/View.
- Flat-line rebuild and search respect collapse state (search may auto-expand or match preview + hidden text — see design).
- Single-line Records remain unchanged (always fully shown).
- No WebView; collapse is engine flat-list + Viewport hit-testing / keyboard, Slint only for optional toolbar actions.

## Capabilities

### New Capabilities

- `engine/collapsible-records`: Per-view expand/collapse state for multiline Records; flat lines and Commands for toggle / expand-all / collapse-all.
- `ui/record-collapse-chrome`: Viewport disclosure cues and interaction; optional Slint expand/collapse-all controls without accent borders.

### Modified Capabilities

- (none)

## Impact

- Crates: `noviewlog-core` (`log_view`, `core/visible`, `viewport` / hit-test, Command/stats) and `noviewlog-slint` (optional toolbar + pointer wiring).
- Docs: `docs/architecture.md` (Record display / collapse).
- Verify: `cargo test -p noviewlog-core --lib`; `bash scripts/run-slint.sh` (or `cargo build --release -p noviewlog-slint`).
- Independent of severity-filter change; both may land in either order.
