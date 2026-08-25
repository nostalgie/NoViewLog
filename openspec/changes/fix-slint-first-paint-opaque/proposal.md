## Why

On first map, the frameless Slint window can show the launching terminal (or leftover GPU pixels) through a transparent hole instead of NoViewLog chrome. End users who start the app from a shell or .desktop file hit this; the empty viewport `Image` plus a translucent winit swapchain, and skipping the first paint when Wayland reports `Occluded`, are enough to leave the window see-through.

## What Changes

- Viewport `Image` is shown only when the source has pixels; the cell stays an opaque `Theme.bg-window` fill otherwise.
- Rust seeds an opaque `#0d1117` bitmap on the viewport `Image` before `ui.run()`.
- The engine tick still ingests PTY while occluded, but it MUST present at least one viewport frame before occlusion/minimize may skip paint.
- After that first present, request one Slint redraw so chrome commits with the bitmap.

## Capabilities

### New Capabilities

- `ui/first-paint`: First mapped window is opaque chrome plus viewport; occlusion MUST NOT skip the first presented viewport frame.

### Modified Capabilities

- (none)

## Impact

- Crate: `noviewlog-slint` only (`ui/app.slint`, `src/main.rs`, `src/engine_bridge.rs`).
- No engine/parser/PTY API changes; `noviewlog-core` untouched.
- Verify: `bash scripts/run-slint.sh` (or `cargo build --release -p noviewlog-slint`).
- Docs: `docs/architecture.md` viewport bitmap path (no API change).
