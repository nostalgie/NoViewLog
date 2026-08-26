## ADDED Requirements

### Requirement: First mapped window is opaque
The host SHALL create the Slint window with an opaque winit surface (`with_transparent(false)`). The first presented Slint frame SHALL fill the Window with opaque chrome (title strip, TERMINALS sidebar, tab strip) and an opaque Viewport cell. An empty viewport `Image` source SHALL NOT cover the Viewport cell. Until a bitmap with non-zero width and height is bound, the cell SHALL show only the opaque window background fill.

#### Scenario: First map does not show the launching terminal through the window
- **GIVEN** the user starts `noviewlog-slint` from a focused terminal or a .desktop launcher
- **WHEN** the Window maps for the first time
- **THEN** the visible surface is NoViewLog chrome and an opaque Viewport, not terminal or VRAM fragments from behind the window

#### Scenario: Window surface is opaque
- **GIVEN** the host selects the Slint winit backend before creating `AppWindow`
- **WHEN** the native window is created
- **THEN** window attributes use `with_transparent(false)` so empty regions do not composite the desktop through the swapchain

#### Scenario: Empty Image does not punch a hole
- **GIVEN** `viewport-image` has not yet been assigned a bitmap with pixels
- **WHEN** Slint paints the Viewport cell
- **THEN** the cell is the opaque `Theme.bg-window` fill and no `Image` child is mounted

### Requirement: Occlusion must not skip the first presented Viewport frame
The engine tick SHALL ingest PTY output while the Window is occluded or minimized. The tick SHALL NOT skip Viewport bitmap upload until at least one Viewport frame has been presented in this process. After that first present, occlusion MAY slow the tick cadence and skip further RGBA paints. After the first present, the UI SHALL request one Slint redraw so chrome commits with the bitmap.

#### Scenario: Occluded first map still presents one Viewport frame
- **GIVEN** the compositor reports the Window occluded or unfocused on first map (typical when launched from a terminal that keeps focus)
- **WHEN** the first engine tick that has a usable Viewport size runs
- **THEN** a Viewport bitmap is uploaded and the Window is redrawn
- **AND** later occluded ticks MAY skip paint while still ingesting PTY
