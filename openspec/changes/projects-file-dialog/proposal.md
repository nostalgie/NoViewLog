## Why

The PROJECTS list in the left sidebar competes with TERMINALS and FILES for vertical space, even though users manage projects infrequently. Project open / rename / delete belong in a dedicated File menu window, leaving the sidebar for live sessions.

## What Changes

- Remove the collapsible PROJECTS section from the Slint sidebar.
- Add **File → Projects…**, which opens a centered overlay listing saved Projects.
- From that overlay the user can open a Project, rename it, delete it, or create a new empty one.
- Creating a Project no longer snapshots current TERMINALS; New starts empty and opens it.

## Capabilities

### New Capabilities

- (none)

### Modified Capabilities

- `ui/projects-sidebar`: PROJECTS listing and create / open / rename / delete move from the sidebar to a File → Projects overlay. Run/Stop on Terminal rows and Edit Launch stay as they are.
- `terminals/projects`: Creating a Project SHALL start empty (no snapshot of live TERMINALS) and open that Project.

## Impact

- `noviewlog-slint`: File menu, new Projects overlay component, sidebar PROJECTS block removed. `docs/terminals.md` Persistence/chrome note.
- `noviewlog-core`: `project_create` writes an empty Project then opens it.
- Verify: `cargo test -p noviewlog-core --lib`, `cargo build --release -p noviewlog-slint` (or `bash scripts/run-slint.sh`).
