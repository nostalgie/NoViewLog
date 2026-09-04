## Context

See proposal.md for motivation. Projects persist via Engine `ProjectsStore` (`~/.config/noviewlog/projects.yaml`). Slint bridges `project-open` / `project-create` / `project-rename` / `project-delete` to `Command::Project*` in `noviewlog-core` `engine/projects.rs`.

## Goals / Non-Goals

**Goals:**

- Relocate project list UX into a File-menu overlay without changing Engine command names.
- Avoid stacked `PopupWindow`s for create/rename (click-outside would dismiss the list).
- New Project is empty (no copy of the previous Project’s Programs).

**Non-Goals:**

- Second OS window (`Window`) or native menu bar.
- Delete confirmation dialog.
- Run-Stop / Edit Launch changes.
- ANSI coloring (`terminal.rs` vs `ansi.rs`) is not in scope.

## Decisions

1. **Overlay, not a second Window.** File → Settings already uses a centered `PopupWindow`. Projects follows that so CSD / no-frame stays one host window.

    Alternative considered: extra Slint `Window` — more chrome (CSD, focus) for little gain.

2. **One overlay, modes for create/rename.** Property `list` | `create` | `rename` inside a `ProjectsDialog` component. Nested popups with `close-on-click-outside` would close the manager when the name field opens.

    Alternative considered: keep standalone create/rename `PopupWindow`s — rejected for that close-policy conflict.

3. **Do not use `FormDialogPanel` for the list.** Its Enter→OK footer treats the manager as a form. List mode: title, New, rows, Close. Create/rename modes reuse `FormTextField` (square `Theme.border`, not fluent `LineEdit`).

4. **Open closes the overlay; rename/delete stay on the list.** Matches “open this project and go work” vs “keep managing.”

5. **Create is empty, then open.** `project_create` writes `programs: []` and calls `project_open`. Opening an empty Project already yields one stopped Terminal. Setting the new Project active without opening would let `sync_active_project_from_terminals` copy the previous live sessions.

    Alternative considered: snapshot current TERMINALS (previous behavior) — rejected; users need a blank Project.

## Risks / Trade-offs

- [Immediate delete] → Same as sidebar; no new confirm in this change.
- [Create opens immediately] → Overlay stays open; TERMINALS behind it reset to one stopped session.

## Migration Plan

Ship via feature branch PR. No data migration; `projects.yaml` format unchanged. Existing Projects keep their Programs. Rollback is revert of the Slint chrome + `project_create` change.
