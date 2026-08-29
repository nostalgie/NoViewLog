## Why

With Wrap ON (the default), scrolling mid-history walks every flat line from index 0 to map `first_row`, then full-frame fontdue-paints. At 10–30k scrollback lines the Terminal becomes unusable. The same cost hits **FILES** when opening a large log (`big.log`) and scrolling mid-history. Flood resilience fixed ingest freezes; it did not fix this O(scrollback) layout cost. See residual R3 in `pty-flood-resilience/DIAGNOSIS.md`.

## What Changes

- Add a visual-row prefix index so Wrap ON mid-scroll is O(viewport) / O(log n), matching Wrap OFF feel.
- Use the index for total visual rows (`max_scroll`) and for jumping to the flat line at `first_row`.
- Maintain the index on append/trim/volatile patch; never rebuild it on scroll alone.
- Do not change default Wrap to OFF; do not add more PTY paint/ingest throttle knobs.

## Capabilities

### New Capabilities

- `engine/wrap-scroll-index`: Prefix-sum visual-row index for Wrap ON scroll/layout.

### Modified Capabilities

- (none)

## Impact

- `crates/noviewlog-core/` — `viewport_layout`, `log_view`, render/scroll paths that call `count_visual_rows` / `collect_visible_*`
- Docs: link from `pty-flood-resilience/DIAGNOSIS.md` R3 → this change
- Verify: `cargo test -p noviewlog-core --lib`, `cargo build --release -p noviewlog-slint`
- Manual: Terminal flood scroll **and** FILES open of `~/big.log` with Wrap ON mid-scroll

## Success criterion

Scrolling mid-history with **Wrap ON** is O(viewport) and feels usable like Wrap OFF — for live Terminal scrollback **and** after opening a large file via FILES — independent of flood budget settings.
