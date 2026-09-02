# noviewlog-slint

Active desktop UI for NoViewLog (Slint + Rust). Default product on `main`.

## Run

From the repo root:

```bash
bash scripts/run-slint.sh
bash scripts/run-slint.sh -- path/to/app.log
```

## Layout

| Path | Role |
|------|------|
| `src/main.rs` | Window wiring, callbacks, tick/render loop |
| `src/engine_bridge.rs` | Timer/occlusion helpers, JSON escape |
| `src/stats_sync.rs` | `StatsSnapshot` → Slint models/properties |
| `src/input.rs` | Keys, zoom, clipboard |
| `src/window_chrome.rs` | Title-drag / occlusion (winit) |
| `src/launch_args.rs` | CLI → `LaunchConfig` |
| `ui/app.slint` | `AppWindow` shell |
| `ui/chrome-menus.slint` | Title-bar / context menus |
| `ui/sidebar.slint` | Tabs, terminals, filters, scrollbar |
| `ui/theme.slint` | Color + spacing tokens |

Engine interaction: `noviewlog_core::Engine` (commands + stats + RGBA paint).
See [`docs/architecture.md`](../../docs/architecture.md).

## Build

Daily (incremental, `opt-level = 3`, no fat LTO):

```bash
cargo build --profile release-dev -p noviewlog-slint
```

Publish (fat LTO):

```bash
cargo build --release -p noviewlog-slint
```
