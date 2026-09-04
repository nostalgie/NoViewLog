## 1. Slint Projects overlay

- [x] 1.1 Add `ProjectsDialog` (list / create / rename modes) in `crates/noviewlog-slint/ui/projects-dialog.slint`
- [x] 1.2 Wire File → Projects… overlay in `app.slint`; open closes overlay; New / rename / delete stay on the list
- [x] 1.3 Remove PROJECTS sidebar section and unused expand state from `AppWindow`
- [x] 1.4 Show the active Project name above TERMINALS when a Project is open
- [x] 1.5 Create Project as empty (no TERMINALS snapshot) and open it

## 2. Docs

- [x] 2.1 Note File → Projects… in `docs/terminals.md` Persistence / UI chrome

## 3. Verify

- [x] 3.1 Host OS daily run or `cargo build --profile release-dev -p noviewlog-slint`
- [x] 3.2 `cargo test -p noviewlog-core --lib`
