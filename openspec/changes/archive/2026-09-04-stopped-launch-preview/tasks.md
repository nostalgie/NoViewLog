## 1. Formatter and stats bind

- [x] 1.1 Add a launch-preview formatter (saved command+args, WSL/distro, cwd, shell-only vs WSL-empty) and verify unit tests cover those cases
- [x] 1.2 Bind `show-launch-preview` and `launch-preview-text` from stats (`!file` && `!running`) and verify stats_sync updates both

## 2. Slint strip

- [x] 2.1 Add the ~22px strip above the viewport Image (`Theme.bg-bar`, elide, rename dismiss, no accent/icons) and verify chrome_icon_wiring and inline_rename_wiring pass

## 3. Docs and verify

- [x] 3.1 Note the stopped launch strip in `docs/terminals.md`
- [x] 3.2 `cargo test -p noviewlog-slint --lib`
- [x] 3.3 Windows `release-dev` GUI: stopped live Terminal shows the strip; Start hides it; FILES never shows it; Edit launch save updates the line
