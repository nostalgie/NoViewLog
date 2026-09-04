## 1. Engine store and commands

- [x] 1.1 Load `ProjectsStore` on Engine startup; add save helper after mutations
- [x] 1.2 Add Commands: ProjectOpen, ProjectCreate, ProjectRename, ProjectDelete, ProgramSetLaunch; optional terminal_id on Stop
- [x] 1.3 Implement project_open (replace live terminals, restore launch+tabs, stopped); project_create snapshot; rename/delete
- [x] 1.4 Link program_id on TerminalState; sync tabs/launch/order/titles back while project active
- [x] 1.5 Expose projects + active_project + has_launch in stats

## 2. Lifecycle

- [x] 2.1 Fix PTY exit / Stop: no interactive shell respawn when `launch.command` is set
- [x] 2.2 Per-id stop and TerminalStart from inactive rows

## 3. Tests and docs

- [x] 3.1 Core tests: open restore, Run/Stop no-respawn, create/YAML round-trip
- [x] 3.2 Update docs/terminals.md for projects.yaml

## 4. Slint UI

- [x] 4.1 PROJECTS sidebar section (list/open/create/rename/delete)
- [x] 4.2 Run/Stop on TerminalRow wired to engine
- [x] 4.3 Edit Launch dialog (command, args, cwd)

## 5. Verify

- [x] 5.1 `cargo test -p noviewlog-core --lib`
- [x] 5.2 `cargo build --release -p noviewlog-slint`
