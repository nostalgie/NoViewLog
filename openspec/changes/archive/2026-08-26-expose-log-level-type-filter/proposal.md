## Why

The engine already detects `LogLevel` on each Record (`detect_level`), but the UI never surfaces it: there is no one-click Errors/Warnings filter and no severity chrome in the Viewport. Competitive log UIs (e.g. Better Terminal Logs) make severity a primary reading aid; without it, users must hand-author include patterns or presets for a basic job.

## What Changes

- Per Tab/View (including Console): a severity / type filter (All, Errors, Warnings, Info, Debug, Unleveled) applied after existing include/exclude rules.
- Viewport chrome for leveled Records: muted severity cue (glyph or tint on the first physical line of the Record) without accent borders.
- Engine commands + stats so Slint can set and display the active severity filter.
- Existing include/exclude presets and `detect_level` heuristics stay; this layer is orthogonal and does not replace pattern filters.

## Capabilities

### New Capabilities

- `engine/severity-filter`: Engine applies per-view `LogLevel` visibility and exposes set/get via Command/stats.
- `ui/severity-chrome`: Slint control for severity filter; Viewport shows non-accent severity cues for leveled Records.

### Modified Capabilities

- (none — living `openspec/specs/` has no prior severity/filter requirements)

## Impact

- Crates: `noviewlog-core` (`log_view`, `core/visible` / filter path, `viewport` paint, Command/stats) and `noviewlog-slint` (toolbar/chrome + bridge).
- Docs: `docs/architecture.md` (filter pipeline order); optional README mention of severity filter.
- Verify: `cargo test -p noviewlog-core --lib`; `bash scripts/run-slint.sh` (or `cargo build --release -p noviewlog-slint`).
- No session persistence of severity filter in v1 (resets per view like search unless already persisted patterns exist — default: not in presets/config.yaml).
