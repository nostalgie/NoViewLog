## Why

Caption close and File → Exit currently call `Window.close()` with no prompt.
Closing kills live PTY children and drops in-memory scrollback. Users need a
chance to cancel, including when only one stopped Terminal is open.

## What Changes

- Intercept every quit path (caption X, File → Exit, Alt+F4 / taskbar) and show
  an in-app confirm overlay before the window hides.
- Always prompt (even a single stopped Terminal). Cancel / Escape / click
  outside keep the app open. Confirm hides the window.
- Reuse existing `FormDialogPanel` / `AppButton` chrome (not a Fluent/Windows
  Terminal clone). No “don’t ask again”.

## Capabilities

### New Capabilities

- `ui/quit-confirm`: Always-on quit confirmation overlay; all window-close
  paths SHALL prompt; Cancel SHALL not quit.

### Modified Capabilities

- (none)

## Impact

- `noviewlog-slint`: `app.slint` overlay; `Window::on_close_requested` in
  `main.rs`; confirm uses `hide()` so the prompt does not re-enter
- `noviewlog-core`: none (PTYs already drop with the engine after `ui.run()`)
- Docs: [`docs/terminals.md`](../../../docs/terminals.md)
- Verify: `cargo test -p noviewlog-slint --test chrome_icon_wiring`;
  `cargo test -p noviewlog-slint --test inline_rename_wiring`;
  `cargo test -p noviewlog-slint --test quit_confirm_wiring`;
  Windows daily run `.\scripts\run-slint-windows.ps1`
