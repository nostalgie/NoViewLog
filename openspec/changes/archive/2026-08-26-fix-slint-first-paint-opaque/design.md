## Context

Slint 1.17.1 creates winit windows with `with_transparent(true)` so FemtoVG WGPU can pick a translucent swapchain. The Viewport is a fontdue RGBA bitmap bound to an `Image` in `AppWindow`. That `Image` starts empty (`viewport-image` default). An empty `Image` with `width/height: 100%` writes alpha 0 over the opaque cell `Rectangle`, so the launching terminal (or leftover VRAM) shows through. Wayland also skips Slint’s pre-render-before-map; `window_should_pause_paint` then drops the tick when `Occluded(true)` arrives because the shell still has focus.

Engine commands and PTY geometry are unchanged. This is Slint host glue only.

## Goals / Non-Goals

**Goals:**

- First mapped frame is opaque chrome + Viewport (no see-through hole).
- At least one Viewport bitmap is presented even if the compositor says occluded on map.
- PTY ingest continues while minimized/occluded after that first present.

**Non-Goals:**

- Bumping Slint past 1.17.1.
- Disabling `no-frame` CSD.
- Changing engine render, PTY winsize, or ANSI paths (`terminal.rs` / `ansi.rs`).

## Decisions

1. **Force an opaque winit surface** via `BackendSelector::with_winit_window_attributes_hook(|a| a.with_transparent(false))` before `AppWindow::new()`. Slint’s default `with_transparent(true)` is the durable root cause of the see-through first map; seed/gate alone cannot fix a translucent swapchain.

2. **Gate the `Image` on a non-empty source** in `image-cell` (`app.slint`). The cell `Rectangle` (`Theme.bg-window`) always paints. `if viewport-image.width > 0px` (and height) mounts the `Image`. Empty source never covers the cell.

3. **Seed an opaque placeholder before `ui.run()`** in `main.rs`: an 8×8 `#0d1117` `SharedPixelBuffer` (same RGB as `Theme.bg-window`) assigned via `set_viewport_image`. First Slint frame already has a real bitmap.

4. **`window_should_pause_paint` takes `presented_once`.** If false, return false (do not pause). After a successful `set_viewport_image` from `Engine::render`, set the flag and call `ui.window().request_redraw()` once. Occluded cadence (`TICK_OCCLUDED`) only after that.

5. **No hide-then-show.** On Wayland, creating the window maps it; delaying `show()` is not a fix.

## Risks / Trade-offs

- [A tiny seeded bitmap stretched with `image-fit: fill`] → brief stretch until the first real render; color matches `bg-window` so it is invisible. Alternative: skip seeding and rely on the `if` gate alone — still do both so the first mapped `Image` is opaque if the gate is late.
- [Ignoring occlusion until first present] → one extra paint while covered; required so launch-from-terminal is not a permanent hole.

## Migration Plan

No user-config or preset migration. Existing rename idle guard (`renaming-terminal-id != ""`) stays.

Verify: `bash scripts/run-slint.sh` from a focused terminal; first map must show title strip, TERMINALS, Console, opaque Viewport.
