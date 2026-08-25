//! Line-oriented SGR parse / strip / overlay (**non-VT layer**).
//!
//! For live PTY screen emulation (cursor, erase, scrollback), see
//! [`crate::core::terminal`]. That module re-emits ANSI rows; this module
//! turns those (or file) lines into styled [`TextSegment`]s for filters and
//! the viewport. Do not add VT cursor semantics here.

use crate::core::types::{TextSegment, TextStyle};

fn is_csi_param(c: char) -> bool {
    matches!(c as u8, 0x30..=0x3F)
}

fn is_csi_intermediate(c: char) -> bool {
    matches!(c as u8, 0x20..=0x2F)
}

fn is_csi_final(c: char) -> bool {
    matches!(c as u8, 0x40..=0x7E)
}

/// Strip all ANSI escape sequences (for filtering / parsing).
pub fn strip_ansi(input: &str) -> String {
    parse_ansi_line(input)
        .into_iter()
        .map(|s| s.text)
        .collect()
}

/// Parse a line into styled segments, keeping SGR colors and dropping other CSI/OSC.
pub fn parse_ansi_line(input: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut current = TextStyle::default();
    let mut text = String::new();
    let mut chars = input.chars().peekable();

    let flush = |text: &mut String, current: &TextStyle, segments: &mut Vec<TextSegment>| {
        if text.is_empty() {
            return;
        }
        let style = if current == &TextStyle::default() {
            None
        } else {
            Some(*current)
        };
        segments.push(TextSegment {
            text: std::mem::take(text),
            style,
        });
    };

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    let mut params = String::new();
                    let mut intermediate = String::new();
                    let mut final_byte = None;
                    while let Some(c) = chars.next() {
                        if is_csi_param(c) {
                            params.push(c);
                        } else if is_csi_intermediate(c) {
                            intermediate.push(c);
                        } else if is_csi_final(c) {
                            final_byte = Some(c);
                            break;
                        } else {
                            break;
                        }
                    }
                    if final_byte == Some('m') && intermediate.is_empty() && !params.contains('?') {
                        flush(&mut text, &current, &mut segments);
                        apply_sgr(&mut current, &params);
                    }
                    // Non-SGR CSI (cursor, erase, etc.) is dropped.
                }
                Some(']') => {
                    // OSC: skip until BEL or ST
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\u{07}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                Some(_) => {
                    // Other 2-byte ESC — drop next char
                    chars.next();
                }
                None => {}
            }
            continue;
        }

        if ch == '\r' {
            // CR overwrite within a finished line: keep only text after last CR.
            text.clear();
            segments.clear();
            current = TextStyle::default();
            continue;
        }

        if ch == '\t' || !ch.is_control() {
            text.push(ch);
        }
    }

    flush(&mut text, &current, &mut segments);

    if segments.is_empty() {
        vec![TextSegment {
            text: String::new(),
            style: None,
        }]
    } else {
        segments
    }
}

fn apply_sgr(style: &mut TextStyle, params: &str) {
    if params.is_empty() {
        *style = TextStyle::default();
        return;
    }

    let codes: Vec<u32> = params
        .split(';')
        .filter_map(|p| {
            if p.is_empty() {
                Some(0)
            } else {
                p.parse().ok()
            }
        })
        .collect();

    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => *style = TextStyle::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            4 => style.underline = true,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            24 => style.underline = false,
            39 => style.fg = None,
            49 => style.bg = None,
            n @ 30..=37 => style.fg = Some(ansi_basic_color(n - 30, false)),
            n @ 90..=97 => style.fg = Some(ansi_basic_color(n - 90, true)),
            n @ 40..=47 => style.bg = Some(ansi_basic_color(n - 40, false)),
            n @ 100..=107 => style.bg = Some(ansi_basic_color(n - 100, true)),
            38 | 48 => {
                let is_fg = codes[i] == 38;
                if i + 1 < codes.len() {
                    match codes[i + 1] {
                        5 if i + 2 < codes.len() => {
                            let color = ansi_256_color(codes[i + 2]);
                            if is_fg {
                                style.fg = Some(color);
                            } else {
                                style.bg = Some(color);
                            }
                            i += 2;
                        }
                        2 if i + 4 < codes.len() => {
                            let color = (
                                codes[i + 2].min(255) as u8,
                                codes[i + 3].min(255) as u8,
                                codes[i + 4].min(255) as u8,
                            );
                            if is_fg {
                                style.fg = Some(color);
                            } else {
                                style.bg = Some(color);
                            }
                            i += 4;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
}

fn ansi_basic_color(index: u32, bright: bool) -> (u8, u8, u8) {
    // GitHub-dark-ish palette close to common terminal themes.
    let colors = if bright {
        [
            (110, 118, 129), // black/bright gray
            (255, 123, 114), // red
            (86, 211, 100),  // green
            (227, 179, 65),  // yellow
            (121, 192, 255), // blue
            (219, 114, 235), // magenta
            (86, 210, 217),  // cyan
            (255, 255, 255), // white
        ]
    } else {
        [
            (72, 79, 88),    // black
            (248, 81, 73),   // red
            (63, 185, 80),   // green
            (210, 153, 34),  // yellow
            (88, 166, 255),  // blue
            (210, 96, 230),  // magenta
            (57, 197, 207),  // cyan
            (230, 237, 243), // white
        ]
    };
    colors[index as usize % 8]
}

fn ansi_256_color(index: u32) -> (u8, u8, u8) {
    match index {
        0..=7 => ansi_basic_color(index, false),
        8..=15 => ansi_basic_color(index - 8, true),
        16..=231 => {
            let n = index - 16;
            let r = n / 36;
            let g = (n % 36) / 6;
            let b = n % 6;
            let level = |v: u32| if v == 0 { 0 } else { 55 + 40 * v };
            (level(r) as u8, level(g) as u8, level(b) as u8)
        }
        232..=255 => {
            let v = (8 + (index - 232) * 10).min(255) as u8;
            (v, v, v)
        }
        _ => (230, 237, 243),
    }
}

/// Overlay user/search styles onto ANSI base segments by character ranges.
pub fn overlay_styles(base: &[TextSegment], overlays: &[TextSegment]) -> Vec<TextSegment> {
    if overlays.is_empty() {
        return base.to_vec();
    }
    if base.is_empty() {
        return overlays.to_vec();
    }

    let plain: String = base.iter().map(|s| s.text.as_str()).collect();
    let overlay_plain: String = overlays.iter().map(|s| s.text.as_str()).collect();
    if plain != overlay_plain {
        // Fallback: prefer overlays (user rules) if texts diverge.
        return overlays.to_vec();
    }

    // Expand to per-byte style lanes, then merge.
    let mut base_styles: Vec<Option<TextStyle>> = Vec::with_capacity(plain.len());
    for seg in base {
        for _ in seg.text.bytes() {
            base_styles.push(seg.style);
        }
    }
    let mut over_styles: Vec<Option<TextStyle>> = Vec::with_capacity(plain.len());
    for seg in overlays {
        for _ in seg.text.bytes() {
            over_styles.push(seg.style);
        }
    }

    let mut out: Vec<TextSegment> = Vec::new();
    let bytes = plain.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Advance to next char boundary.
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j] & 0b1100_0000) == 0b1000_0000 {
            j += 1;
        }
        let mut style = base_styles[i];
        if let Some(over) = over_styles[i] {
            style = Some(merge_style(style.unwrap_or_default(), over));
        }
        let ch = &plain[i..j];
        if let Some(last) = out.last_mut() {
            if last.style == style {
                last.text.push_str(ch);
            } else {
                out.push(TextSegment {
                    text: ch.to_string(),
                    style,
                });
            }
        } else {
            out.push(TextSegment {
                text: ch.to_string(),
                style,
            });
        }
        i = j;
    }

    if out.is_empty() {
        vec![TextSegment {
            text: plain,
            style: None,
        }]
    } else {
        out
    }
}

fn merge_style(base: TextStyle, over: TextStyle) -> TextStyle {
    TextStyle {
        fg: over.fg.or(base.fg),
        bg: over.bg.or(base.bg),
        bold: over.bold || base.bold,
        dim: over.dim || base.dim,
        underline: over.underline || base.underline,
        search: over.search || base.search,
        search_current: over.search_current || base.search_current,
        selected: over.selected || base.selected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_green() {
        let segs = parse_ansi_line("\u{1b}[32m✔ Building...\u{1b}[0m");
        let joined: String = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "✔ Building...");
        assert!(segs.iter().any(|s| s.style.is_some_and(|st| st.fg == Some((63, 185, 80)))));
    }

    #[test]
    fn strip_preserves_leading_spaces() {
        assert_eq!(strip_ansi("    at foo.js:1:1"), "    at foo.js:1:1");
    }

    #[test]
    fn strips_cursor_but_keeps_color() {
        let segs = parse_ansi_line("\u{1b}[?25l\u{1b}[32mOK\u{1b}[0m\u{1b}[?25h");
        assert_eq!(strip_ansi("\u{1b}[?25l\u{1b}[32mOK\u{1b}[0m\u{1b}[?25h"), "OK");
        assert_eq!(segs[0].text, "OK");
        assert!(segs[0].style.unwrap().fg.is_some());
    }

    #[test]
    fn strips_device_attribute_csi() {
        let segs = parse_ansi_line("To access the server \u{1b}[>0;10;1cgo to:");
        let joined: String = segs.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "To access the server go to:");
    }

    #[test]
    fn strip_ansi_plain() {
        assert_eq!(strip_ansi("hello"), "hello");
    }
}
