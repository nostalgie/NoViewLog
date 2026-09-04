## 1. Slint overlay

- [x] 1.1 Add the quit `PopupWindow` (`FormDialogPanel`, English copy, Cancel + Close) and `public function open-quit-confirm` (close other overlays first) and verify `quit_confirm_wiring` plus `chrome_icon_wiring` pass

## 2. Rust close intercept

- [x] 2.1 Wire `Window::on_close_requested` to `invoke_open_quit_confirm` + `KeepWindowShown`; confirm calls `hide()` and verify `quit_confirm_wiring` asserts the Rust hook exists

## 3. Docs and verify

- [x] 3.1 Note the always-on quit prompt in `docs/terminals.md`
- [x] 3.2 `cargo test -p noviewlog-slint --test chrome_icon_wiring`; `cargo test -p noviewlog-slint --test inline_rename_wiring`; `cargo test -p noviewlog-slint --test quit_confirm_wiring`
- [x] 3.3 Windows `release-dev` GUI: caption X, File → Exit, and Alt+F4 show the overlay; Cancel keeps the app; Close hides it
