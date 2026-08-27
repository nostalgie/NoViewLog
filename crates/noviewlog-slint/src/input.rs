//! Keyboard, clipboard, and zoom key helpers.

use noviewlog_core::Engine;

pub(crate) fn is_zoom_in_key(text: &str) -> bool {
    text == "=" || text == "+"
}

pub(crate) fn clipboard_has_text() -> bool {
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return false;
    };
    clipboard
        .get_text()
        .map(|t| !t.is_empty())
        .unwrap_or(false)
}

pub(crate) fn is_key_char(text: &str, want: char) -> bool {
    text.chars()
        .next()
        .map(|c| c.eq_ignore_ascii_case(&want))
        .unwrap_or(false)
}

pub(crate) fn copy_selection_to_clipboard(engine: &Engine) -> bool {
    let Some(text) = engine.selection_text() else {
        return false;
    };
    if text.is_empty() {
        return false;
    }
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return false;
    };
    clipboard.set_text(text).is_ok()
}

pub(crate) fn paste_clipboard_to_terminal(engine: &mut Engine) {
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return;
    };
    let Ok(text) = clipboard.get_text() else {
        return;
    };
    if text.is_empty() {
        return;
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    engine.handle_key(normalized.as_bytes());
}

pub(crate) fn handle_key_event(engine: &mut Engine, text: &str, ctrl_or_meta: bool) -> bool {
    if text.is_empty() {
        return false;
    }

    if ctrl_or_meta {
        let ch = text.chars().next().unwrap_or('\0');
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_lowercase() {
            let byte = (lower as u8) - b'a' + 1;
            engine.handle_key(&[byte]);
            return true;
        }
        return false;
    }

    if let Some(bytes) = map_special_key(text) {
        engine.handle_key(bytes);
        return true;
    }

    if is_terminal_control_text(text) {
        return true;
    }

    engine.handle_key(text.as_bytes());
    true
}

pub(crate) fn is_terminal_control_text(text: &str) -> bool {
    text.chars().all(|ch| {
        matches!(ch, '\r' | '\n' | '\t' | '\u{7f}' | '\u{8}') || ch < ' '
    })
}

pub(crate) fn map_special_key(text: &str) -> Option<&'static [u8]> {
    const BACKSPACE: &str = "\u{8}";
    const TAB: &str = "\u{9}";
    const RETURN: &str = "\n";
    const ESCAPE: &str = "\u{1b}";
    const DELETE: &str = "\u{7f}";
    const UP: &str = "\u{f700}";
    const DOWN: &str = "\u{f701}";
    const LEFT: &str = "\u{f702}";
    const RIGHT: &str = "\u{f703}";
    const INSERT: &str = "\u{f727}";
    const HOME: &str = "\u{f729}";
    const END: &str = "\u{f72b}";
    const PAGE_UP: &str = "\u{f72c}";
    const PAGE_DOWN: &str = "\u{f72d}";

    match text {
        RETURN | "\r" => Some(b"\r"),
        BACKSPACE => Some(&[0x7f]),
        DELETE => Some(b"\x1b[3~"),
        TAB => Some(&[0x09]),
        ESCAPE => Some(&[0x1b]),
        UP => Some(b"\x1b[A"),
        DOWN => Some(b"\x1b[B"),
        RIGHT => Some(b"\x1b[C"),
        LEFT => Some(b"\x1b[D"),
        HOME => Some(b"\x1b[H"),
        END => Some(b"\x1b[F"),
        PAGE_UP => Some(b"\x1b[5~"),
        PAGE_DOWN => Some(b"\x1b[6~"),
        INSERT => None,
        _ => None,
    }
}
