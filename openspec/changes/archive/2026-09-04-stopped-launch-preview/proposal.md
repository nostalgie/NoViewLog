## Why

When a live Terminal is stopped, the viewport is a black bitmap plus a generic
center hint. Start/Stop on the TERMINALS row uses the saved launch config, but
that config is only visible in Edit launch. After Project open or Stop it is
unclear whether Start will run a saved command (WSL, cwd, args) or an interactive
shell.

## What Changes

- Show a compact one-line chrome strip above the viewport when the active live
  Terminal is stopped, summarizing the saved Start action (command/args, WSL,
  cwd).
- Hide the strip while the process is running and on FILES sessions.
- Keep existing centered empty-buffer hints; Start/Stop stays on the TERMINALS
  row (no extra play/stop icons on the strip).

## Capabilities

### New Capabilities

- `ui/launch-preview`: Stopped live Terminals show a launch-summary strip above
  the viewport so Start is predictable.

### Modified Capabilities

- (none)

## Impact

- `noviewlog-slint`: formatter + `stats_sync` properties; strip in `app.slint`
- `noviewlog-core`: none (launch fields already on stats)
- Docs: [`docs/terminals.md`](../../../docs/terminals.md)
- Verify: `cargo test -p noviewlog-slint --lib`;
  `cargo test -p noviewlog-slint --test chrome_icon_wiring`;
  `cargo test -p noviewlog-slint --test inline_rename_wiring`;
  Windows daily run `.\scripts\run-slint-windows.ps1`
