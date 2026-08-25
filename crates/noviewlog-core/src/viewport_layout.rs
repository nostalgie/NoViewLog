use crate::color_emoji::{display_cell_count, is_zero_width_emoji_mark};
use crate::core::types::FlatLine;
use crate::viewport::ViewportMetrics;

pub const LEFT_PAD: u32 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextPos {
    pub line_index: usize,
    pub byte_offset: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: TextPos,
    pub caret: TextPos,
}

impl TextSelection {
    pub fn new(anchor: TextPos, caret: TextPos) -> Self {
        Self { anchor, caret }
    }

    pub fn normalized(&self) -> (TextPos, TextPos) {
        let (a, b) = (self.anchor, self.caret);
        if a < b {
            (a, b)
        } else {
            (b, a)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.caret
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualLine {
    pub flat_index: usize,
    pub start: usize,
    pub end: usize,
}

pub fn content_width(viewport_width: u32) -> u32 {
    viewport_width.saturating_sub(LEFT_PAD)
}

pub fn max_cols(content_width: u32, cell_width: u32) -> usize {
    (content_width / cell_width.max(1)).max(1) as usize
}

pub fn build_visual_lines(
    lines: &[FlatLine],
    wrap: bool,
    viewport_width: u32,
    cell_width: u32,
) -> Vec<VisualLine> {
    if wrap {
        let cols = max_cols(content_width(viewport_width), cell_width);
        lines
            .iter()
            .enumerate()
            .flat_map(|(flat_index, line)| wrap_flat_line(flat_index, &line.raw, cols))
            .collect()
    } else {
        lines
            .iter()
            .enumerate()
            .map(|(flat_index, line)| VisualLine {
                flat_index,
                start: 0,
                end: line.raw.len(),
            })
            .collect()
    }
}

fn wrap_flat_line(flat_index: usize, raw: &str, max_cols: usize) -> Vec<VisualLine> {
    if raw.is_empty() {
        return vec![VisualLine {
            flat_index,
            start: 0,
            end: 0,
        }];
    }

    let mut out = Vec::new();
    let mut chunk_start = 0usize;
    let mut col = 0usize;

    for (byte_idx, ch) in raw.char_indices() {
        if is_zero_width_emoji_mark(ch) {
            continue;
        }
        if col >= max_cols {
            out.push(VisualLine {
                flat_index,
                start: chunk_start,
                end: byte_idx,
            });
            chunk_start = byte_idx;
            col = 0;
        }
        col += 1;
    }
    out.push(VisualLine {
        flat_index,
        start: chunk_start,
        end: raw.len(),
    });
    out
}

pub fn max_scroll_x(lines: &[FlatLine], viewport_width: u32, cell_width: u32) -> f32 {
    let cw = cell_width.max(1) as f32;
    let available = content_width(viewport_width) as f32;
    let max_line_px = lines
        .iter()
        .map(|l| display_cell_count(&l.raw) as f32 * cw)
        .fold(0.0f32, f32::max);
    (max_line_px - available).max(0.0)
}

pub fn pos_at_pixel(
    x: f32,
    y: f32,
    scroll_y: f32,
    scroll_x: f32,
    wrap: bool,
    metrics: &ViewportMetrics,
    visual_lines: &[VisualLine],
    flat_lines: &[FlatLine],
) -> TextPos {
    if visual_lines.is_empty() {
        return TextPos::default();
    }

    let row = ((scroll_y + y.max(0.0)) / metrics.row_stride)
        .floor()
        .max(0.0) as usize;
    let row = row.min(visual_lines.len().saturating_sub(1));
    let visual = &visual_lines[row];
    let flat = &flat_lines[visual.flat_index];
    let slice = &flat.raw[visual.start..visual.end];

    let rel_x = x - LEFT_PAD as f32 + if wrap { 0.0 } else { scroll_x };
    let col = (rel_x / metrics.cell_width as f32).floor().max(0.0) as usize;
    let byte_in_slice = byte_offset_for_char_col(slice, col);
    TextPos {
        line_index: visual.flat_index,
        byte_offset: visual.start + byte_in_slice,
    }
}

pub fn byte_offset_for_char_col(text: &str, col: usize) -> usize {
    if col == 0 {
        return 0;
    }
    let mut chars = 0usize;
    for (byte_idx, _) in text.char_indices() {
        if chars >= col {
            return byte_idx;
        }
        chars += 1;
    }
    text.len()
}

pub fn selection_slice_range(
    sel: &TextSelection,
    flat_index: usize,
    slice_start: usize,
    slice_end: usize,
) -> Option<(usize, usize)> {
    if sel.is_empty() {
        return None;
    }
    let (start, end) = sel.normalized();
    if start.line_index > flat_index || end.line_index < flat_index {
        return None;
    }

    let sel_start = if start.line_index == flat_index {
        start.byte_offset
    } else {
        slice_start
    };
    let sel_end = if end.line_index == flat_index {
        end.byte_offset
    } else {
        slice_end
    };

    let from = sel_start.max(slice_start);
    let to = sel_end.min(slice_end);
    if from >= to {
        return None;
    }
    Some((from - slice_start, to - slice_start))
}

pub fn selection_plain_text(flat_lines: &[FlatLine], sel: &TextSelection) -> String {
    if sel.is_empty() {
        return String::new();
    }
    let (start, end) = sel.normalized();
    if start.line_index == end.line_index {
        let line = &flat_lines[start.line_index].raw;
        return line[start.byte_offset..end.byte_offset].to_string();
    }

    let mut out = String::new();
    for (i, line) in flat_lines
        .iter()
        .enumerate()
        .skip(start.line_index)
        .take(end.line_index - start.line_index + 1)
    {
        if i == start.line_index {
            out.push_str(&line.raw[start.byte_offset..]);
        } else if i == end.line_index {
            out.push_str(&line.raw[..end.byte_offset]);
        } else {
            out.push_str(&line.raw);
        }
        if i < end.line_index {
            out.push('\n');
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Select the word (or whitespace / punctuation run) under `pos`.
pub fn word_selection_at(flat_lines: &[FlatLine], pos: TextPos) -> Option<TextSelection> {
    let line = flat_lines.get(pos.line_index)?;
    let raw = &line.raw;
    if raw.is_empty() {
        return Some(TextSelection::new(
            TextPos {
                line_index: pos.line_index,
                byte_offset: 0,
            },
            TextPos {
                line_index: pos.line_index,
                byte_offset: 0,
            },
        ));
    }

    let chars: Vec<(usize, char)> = raw.char_indices().collect();
    let char_idx = if pos.byte_offset >= raw.len() {
        chars.len().saturating_sub(1)
    } else {
        chars
            .iter()
            .position(|(b, _)| *b == pos.byte_offset)
            .or_else(|| {
                chars
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, (b, _))| *b <= pos.byte_offset)
                    .map(|(i, _)| i)
            })
            .unwrap_or(0)
    };

    let ch = chars[char_idx].1;
    let mut start = char_idx;
    let mut end = char_idx + 1;

    if ch.is_whitespace() {
        while start > 0 && chars[start - 1].1.is_whitespace() {
            start -= 1;
        }
        while end < chars.len() && chars[end].1.is_whitespace() {
            end += 1;
        }
    } else {
        let word = is_word_char(ch);
        while start > 0 {
            let prev = chars[start - 1].1;
            if prev.is_whitespace() || is_word_char(prev) != word {
                break;
            }
            start -= 1;
        }
        while end < chars.len() {
            let next = chars[end].1;
            if next.is_whitespace() || is_word_char(next) != word {
                break;
            }
            end += 1;
        }
    }

    let start_b = chars[start].0;
    let end_b = if end < chars.len() {
        chars[end].0
    } else {
        raw.len()
    };
    Some(TextSelection::new(
        TextPos {
            line_index: pos.line_index,
            byte_offset: start_b,
        },
        TextPos {
            line_index: pos.line_index,
            byte_offset: end_b,
        },
    ))
}

/// Select every FlatLine sharing the same `record_id` as the line under `pos`.
pub fn record_selection_at(flat_lines: &[FlatLine], pos: TextPos) -> Option<TextSelection> {
    let line = flat_lines.get(pos.line_index)?;
    let record_id = line.record_id;
    let mut first = pos.line_index;
    let mut last = pos.line_index;
    while first > 0 && flat_lines[first - 1].record_id == record_id {
        first -= 1;
    }
    while last + 1 < flat_lines.len() && flat_lines[last + 1].record_id == record_id {
        last += 1;
    }
    Some(TextSelection::new(
        TextPos {
            line_index: first,
            byte_offset: 0,
        },
        TextPos {
            line_index: last,
            byte_offset: flat_lines[last].raw.len(),
        },
    ))
}

pub fn slice_segments(
    segments: &[crate::core::types::TextSegment],
    start: usize,
    end: usize,
) -> Vec<crate::core::types::TextSegment> {
    use crate::core::types::TextSegment;

    if start >= end {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut cursor = 0usize;
    for seg in segments {
        let seg_start = cursor;
        let seg_end = cursor + seg.text.len();
        cursor = seg_end;

        if seg_end <= start || seg_start >= end {
            continue;
        }

        let local_start = start.saturating_sub(seg_start);
        let local_end = (end - seg_start).min(seg.text.len());
        out.push(TextSegment {
            text: seg.text[local_start..local_end].to_string(),
            style: seg.style,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{FlatLine, TextSegment};

    fn flat_line(text: &str) -> FlatLine {
        FlatLine {
            record_id: 1,
            line_index: 0,
            segments: vec![TextSegment {
                text: text.to_string(),
                style: None,
            }],
            raw: text.to_string(),
        }
    }

    #[test]
    fn wrap_splits_long_line_into_visual_rows() {
        let line = flat_line("abcdefghijklmnopqrstuvwxyz");
        let visual = build_visual_lines(&[line.clone()], true, 80, 8);
        assert!(
            visual.len() >= 3,
            "expected multiple wrapped rows, got {}",
            visual.len()
        );
        assert!(visual.iter().all(|v| v.flat_index == 0));
        let joined: String = visual
            .iter()
            .map(|v| &line.raw[v.start..v.end])
            .collect();
        assert_eq!(joined, "abcdefghijklmnopqrstuvwxyz");
    }

    #[test]
    fn wrap_on_vs_off_differ_for_long_line() {
        let line = flat_line(&"x".repeat(200));
        let wrapped = build_visual_lines(&[line.clone()], true, 80, 8);
        let nowrap = build_visual_lines(&[line], false, 80, 8);
        assert!(
            wrapped.len() > 1,
            "Wrap ON must soft-wrap a long line, got {} visual rows",
            wrapped.len()
        );
        assert_eq!(
            nowrap.len(),
            1,
            "Wrap OFF must keep a single visual row for H-scroll"
        );
    }

    #[test]
    fn wrap_tracks_viewport_width_not_fixed_ceiling() {
        let line = flat_line(&"a".repeat(150));
        // content_width 72 / cell 8 → 9 cols; content_width 792 → 99 cols
        let narrow = build_visual_lines(&[line.clone()], true, 80, 8);
        let wide = build_visual_lines(&[line], true, 800, 8);
        assert!(
            narrow.len() > wide.len(),
            "wider viewport must wrap less (narrow={}, wide={})",
            narrow.len(),
            wide.len()
        );
        assert!(wide.len() >= 2, "150 chars still wrap at 99 cols");
    }

    #[test]
    fn nowrap_plus_scroll_x_reveals_tail_of_line() {
        let renderer = crate::viewport::ViewportRenderer::new();
        let metrics = renderer.metrics();
        let cell = metrics.cell_width;
        let line = flat_line("http://localhost:1337/admin/dashboard");
        let visual = build_visual_lines(&[line.clone()], false, 120, cell);
        assert_eq!(visual.len(), 1);

        let scroll_x = max_scroll_x(&[line.clone()], 120, cell);
        assert!(scroll_x > 0.0);

        let tail_col = line.raw.chars().count().saturating_sub(4);
        let pos = pos_at_pixel(
            LEFT_PAD as f32 + tail_col as f32 * cell as f32 - scroll_x,
            0.0,
            0.0,
            scroll_x,
            false,
            &metrics,
            &visual,
            &[line.clone()],
        );
        assert_eq!(pos.line_index, 0);
        assert!(pos.byte_offset >= line.raw.len().saturating_sub(8));
    }

    #[test]
    fn pos_at_pixel_maps_click_to_char_index() {
        let renderer = crate::viewport::ViewportRenderer::new();
        let metrics = renderer.metrics();
        let cell = metrics.cell_width;
        let line = flat_line("hello world");
        let visual = build_visual_lines(&[line.clone()], true, 200, cell);

        let pos = pos_at_pixel(
            LEFT_PAD as f32 + 6.0 * cell as f32,
            metrics.row_stride * 0.5,
            0.0,
            0.0,
            true,
            &metrics,
            &visual,
            &[line],
        );
        assert_eq!(pos.line_index, 0);
        assert_eq!(pos.byte_offset, "hello ".len());
    }

    #[test]
    fn selection_plain_text_strips_to_raw_plain() {
        let lines = vec![
            flat_line("line one"),
            flat_line("line two"),
        ];
        let sel = TextSelection::new(
            TextPos {
                line_index: 0,
                byte_offset: 5,
            },
            TextPos {
                line_index: 1,
                byte_offset: 4,
            },
        );
        assert_eq!(selection_plain_text(&lines, &sel), "one\nline");
    }

    #[test]
    fn word_selection_picks_identifier_under_cursor() {
        let lines = vec![flat_line("foo bar_baz qux")];
        // Click inside bar_baz
        let pos = TextPos {
            line_index: 0,
            byte_offset: "foo ".len() + 2,
        };
        let sel = word_selection_at(&lines, pos).unwrap();
        assert_eq!(selection_plain_text(&lines, &sel), "bar_baz");
    }

    #[test]
    fn record_selection_spans_all_lines_of_record() {
        let mut a = flat_line("line A1");
        a.record_id = 10;
        a.line_index = 0;
        let mut b = flat_line("line A2");
        b.record_id = 10;
        b.line_index = 1;
        let mut c = flat_line("line B");
        c.record_id = 11;
        c.line_index = 0;
        let lines = vec![a, b, c];
        let sel = record_selection_at(
            &lines,
            TextPos {
                line_index: 1,
                byte_offset: 0,
            },
        )
        .unwrap();
        assert_eq!(selection_plain_text(&lines, &sel), "line A1\nline A2");
    }
}
