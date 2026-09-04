## Context

See proposal.md. `AppWindow` is a no-frame CSD Window. Caption X and File → Exit
call `root.close()`. Slint 1.17 has no Window `close-requested` callback in
`.slint`; `Window::on_close_requested` lives on the Rust `Window` API.
`WindowItem::close()` runs that callback, then `hide()` only if it returns
`HideWindow`. `hide()` itself does not re-enter the callback.

Engine ↔ Slint: quit is chrome-only. After `ui.run()` returns, `Engine` Drop
stops remaining PTYs. No `Command` or `noviewlog-core` change. Dual ANSI paths
are unused.

## Goals / Non-Goals

**Goals:**

- One overlay for every close path, including window-manager CloseRequested.
- Confirm uses `hide()` so the prompt cannot loop.

**Non-Goals:**

- Conditional prompt (running PTY / multi-Terminal).
- “Don’t ask again”, native MessageBox, or Fluent/Windows Terminal visuals.
- Persisting scrollback on quit.

## Decisions

1. **Rust `on_close_requested`, not a `.slint` close-requested handler** —
   WindowItem in 1.17 does not expose that callback to Slint. Caption X and
   File → Exit already call `root.close()`, which hits `request_close()`.
   Alternative considered: intercept only the caption button — would miss
   Alt+F4 / taskbar.

2. **Public `open-quit-confirm()` + `hide()` on confirm** — Rust shows the
   existing `PopupWindow` pattern (`FormDialogPanel` like Settings/About) and
   returns `KeepWindowShown`. Confirm closes the popup then `root.hide()`.
   Alternative considered: set an `allow-close` flag and call `close()` again —
   extra state for the same outcome.

3. **Close other overlays when prompting** — stacked `PopupWindow`s conflict
   with click-outside. `open-quit-confirm` closes menus, Projects, Edit launch,
   Settings, and About first. Same as File → Projects closing chrome menus.

4. **Copy and chrome** — Title `Close NoViewLog?`; body notes running Terminals
   stop and Project settings stay saved; Cancel + primary Close. `Theme.border`
   only; `AppButton` primary fill may use `Theme.accent`.

## Risks / Trade-offs

- [Enter on the overlay confirms Close] → Same as other `FormDialogPanel`s;
  Escape still cancels.
- [no-frame + some WMs] → Caption X still uses `close()`; CloseRequested covers
  Alt+F4 when the backend delivers it.
- [PTY still running until process exit] → Acceptable; hide ends `ui.run()` and
  Drop stops children.

## Migration Plan

None. Chrome-only; no store or engine protocol change.

## Open Questions

None.
