//! Source-level UI wiring: dead-space and click-away must stay hooked.
//! A stretch Rectangle under FILES previously swallowed clicks.

use std::fs;
use std::path::PathBuf;

fn app_slint() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/app.slint");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn sidebar_slint() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/sidebar.slint");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn sidebar_dead_space_is_toucharea_that_dismisses_rename() {
    let src = app_slint();
    assert!(
        src.contains("sidebar-dead-space := TouchArea"),
        "leftover sidebar height must be a TouchArea (not a silent Rectangle)"
    );
    let idx = src
        .find("sidebar-dead-space := TouchArea")
        .expect("sidebar-dead-space");
    let chunk = &src[idx..idx.saturating_add(500).min(src.len())];
    assert!(
        chunk.contains("dismiss-any-rename-if-any()"),
        "sidebar-dead-space must call dismiss-any-rename-if-any"
    );
    assert!(
        !chunk.contains("background: transparent;"),
        "do not replace dead-space with a non-interactive fill"
    );
}

#[test]
fn viewport_press_dismisses_rename() {
    let src = app_slint();
    assert!(src.contains("function dismiss-any-rename-if-any()"));
    assert!(src.contains("viewport-host.focus();"));
    let down = src
        .find("if (event.kind == PointerEventKind.down)")
        .expect("viewport pointer down");
    let window = &src[down..down.saturating_add(220).min(src.len())];
    assert!(
        window.contains("dismiss-any-rename-if-any()"),
        "viewport pointer-down must dismiss rename"
    );
}

#[test]
fn files_and_terminals_headers_dismiss_rename() {
    let src = app_slint();
    assert!(src.contains("dismiss-any-rename-if-any(); root.toggle-files-section()"));
    assert!(src.contains("dismiss-any-rename-if-any(); root.toggle-terminals-section()"));
}

#[test]
fn files_rows_cannot_rename() {
    let src = app_slint();
    let files = src.find("for file in root.files-model: TerminalRow").expect("files TerminalRow");
    let chunk = &src[files..files.saturating_add(1200).min(src.len())];
    assert!(
        chunk.contains("can-rename: false"),
        "FILES TerminalRow must set can-rename: false"
    );
    assert!(
        !chunk.contains("start-terminal-rename(file.id"),
        "FILES must not wire start-terminal-rename"
    );
}

#[test]
fn terminals_rows_can_rename() {
    let src = app_slint();
    let terms = src
        .find("for term in root.terminals-model: TerminalRow")
        .expect("terminals TerminalRow");
    let chunk = &src[terms..terms.saturating_add(1200).min(src.len())];
    assert!(
        chunk.contains("can-rename: true") || chunk.contains("start-terminal-rename(term.id"),
        "TERMINALS rows must allow rename"
    );
}

#[test]
fn terminal_row_keeps_one_title_subtitle_stack() {
    let src = sidebar_slint();
    let idx = src.find("title-slot := Rectangle").expect("title-slot");
    let chunk = &src[idx..idx.saturating_add(2500).min(src.len())];
    assert!(
        chunk.contains("height: Theme.rename-terminal-height"),
        "title line must have a fixed height in idle and rename"
    );
    assert!(
        !chunk.contains("if !root.renaming: VerticalLayout"),
        "do not swap a second VerticalLayout for rename — subtitle would jump"
    );
}

#[test]
fn empty_files_list_height_stays_zero_in_slint() {
    let src = app_slint();
    assert!(
        src.contains("if (!expanded || count <= 0)") && src.contains("return 0px;"),
        "empty FILES list stays 0px — dead-space TouchArea is the hit target"
    );
}

#[test]
fn status_bar_press_dismisses_rename() {
    let src = app_slint();
    let idx = src
        .rfind("root.status-text")
        .expect("status-text");
    let window = &src[idx.saturating_sub(900)..idx];
    assert!(
        window.contains("dismiss-any-rename-if-any()"),
        "status bar must dismiss rename on pointer-down"
    );
}

#[test]
fn rename_fields_use_even_inner_padding() {
    let theme = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/theme.slint");
    let theme = fs::read_to_string(&theme).unwrap();
    assert!(theme.contains("rename-pad:"));
    assert!(theme.contains("field-pad-x:"));
    assert!(theme.contains("field-pad-y:"));
    let src = sidebar_slint();
    assert!(
        src.contains("padding-left: Theme.field-pad-x")
            && src.contains("padding-top: Theme.field-pad-y")
            && src.contains("padding-right: Theme.field-pad-x")
            && src.contains("padding-bottom: Theme.field-pad-y"),
        "tab rename must use even Theme.field-pad inset"
    );
    let idx = src.find("title-slot := Rectangle").expect("title-slot");
    let chunk = &src[idx..idx.saturating_add(1800).min(src.len())];
    assert!(
        chunk.matches("x: Theme.rename-pad").count() >= 2
            && chunk.matches("width: parent.width - 2 * Theme.rename-pad").count() >= 2,
        "TERMINALS rename and idle title must inset by Theme.rename-pad on left and right"
    );
}

#[test]
fn form_text_field_has_even_inner_padding() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/form-dialogs.slint");
    let src = fs::read_to_string(&path).unwrap();
    assert!(src.contains("padding-left: Theme.field-pad-x"));
    assert!(src.contains("padding-right: Theme.field-pad-x"));
    assert!(src.contains("padding-top: Theme.field-pad-y"));
    assert!(src.contains("padding-bottom: Theme.field-pad-y"));
    assert!(src.contains("vertical-alignment: center"));
}
