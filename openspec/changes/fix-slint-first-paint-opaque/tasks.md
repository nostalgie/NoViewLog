## 1. Slint viewport cell

- [x] 1.1 In `crates/noviewlog-slint/ui/app.slint` `image-cell`, mount `Image` only when `viewport-image` has non-zero width and height
- [x] 1.2 Keep the cell `Rectangle` fill as `Theme.bg-window` so an empty source cannot cover it

## 2. Host glue

- [x] 2.1 Seed an opaque `#0d1117` `SharedPixelBuffer` on `viewport-image` in `main.rs` before `ui.run()`
- [x] 2.2 Extend `window_should_pause_paint` so occlusion/minimize cannot skip paint until one Viewport frame has been uploaded
- [x] 2.3 After that first `set_viewport_image` from `Engine::render`, set presented-once and `request_redraw()` once
- [x] 2.4 Leave `renaming-terminal-id != ""` idle guard unchanged

## 3. Verify

- [x] 3.1 `bash scripts/run-slint.sh` from a focused terminal: first map shows chrome + opaque Viewport, not terminal/VRAM fragments
