//! ANSI frame painter: tab bar, content rows with severity cues, and the
//! status/input lines. Layout lives here so mouse hit-testing can reuse it
//! (row 0 = tab bar; rows 1.. = content lines in paint order).
//!
//! Painting is diff-based: every logical row is rendered into a buffer and
//! written only when its bytes differ from the previous frame. Rewriting an
//! unchanged line is what makes emulators flicker, so a static screen must
//! produce zero output.

use std::io::Write;

use crossterm::style::{Color, Print, SetBackgroundColor, SetForegroundColor};
use crossterm::{cursor::MoveTo, queue, terminal::Clear, terminal::ClearType};

use noviewlog_core::core::types::{FlatLine, LogLevel, TextSegment};

use crate::App;

fn level_color(level: LogLevel) -> Color {
    match level {
        LogLevel::Error => Color::Red,
        LogLevel::Warn => Color::Yellow,
        LogLevel::Info => Color::DarkGreen,
        LogLevel::Debug => Color::Grey,
    }
}

fn level_tag(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "E",
        LogLevel::Warn => "W",
        LogLevel::Info => "I",
        LogLevel::Debug => "D",
    }
}

fn rgb(c: (u8, u8, u8)) -> Color {
    Color::Rgb {
        r: c.0,
        g: c.1,
        b: c.2,
    }
}

/// Append one row (MoveTo + line clear + content) to `buf`.
/// Colors are set explicitly per segment — never inherited from the previous
/// row or segment, or unstyled text prints in the last used color.
fn build_row(buf: &mut Vec<u8>, y: u16, paint: impl FnOnce(&mut Vec<u8>)) {
    let _ = queue!(buf, MoveTo(0, y), Clear(ClearType::CurrentLine));
    paint(buf);
    let _ = queue!(buf, SetForegroundColor(Color::Reset), SetBackgroundColor(Color::Reset));
}

/// Append segments; chars within `hl` (start..end col) get the selection
/// background. Chars are printed in runs of the same style.
fn queue_segments(
    buf: &mut Vec<u8>,
    segments: &[TextSegment],
    cols: usize,
    hl: Option<(usize, usize)>,
) {
    let mut col = 0usize;
    for seg in segments {
        if col >= cols {
            break;
        }
        for ch in seg.text.chars() {
            if col >= cols {
                break;
            }
            let selected = hl.is_some_and(|(s, e)| col >= s && col < e);
            let fg = match &seg.style {
                Some(style) => style.fg.map(rgb).unwrap_or(Color::Reset),
                None => Color::Reset,
            };
            let _ = if selected {
                queue!(
                    buf,
                    SetForegroundColor(fg),
                    SetBackgroundColor(Color::DarkBlue),
                    Print(ch)
                )
            } else {
                queue!(buf, SetForegroundColor(fg), Print(ch))
            };
            col += 1;
        }
    }
    let _ = queue!(buf, SetBackgroundColor(Color::Reset));
}

/// Selection column span for content row `i`, from the drag state.
fn highlight_span(app: &App, i: usize) -> Option<(usize, usize)> {
    let a = app.sel_anchor?;
    let c = app.sel_current?;
    let ((r0, c0), (r1, c1)) = if a <= c { (a, c) } else { (c, a) };
    if i < r0 || i > r1 {
        return None;
    }
    Some((
        if i == r0 { c0 } else { 0 },
        if i == r1 { c1 } else { usize::MAX },
    ))
}

/// Diff-based frame paint. `lines` is the visible slice from the engine.
/// `full` forces one full repaint (used on menu open/close, since the menu
/// is a floating overlay outside the steady row layout).
pub fn frame(
    out: &mut impl Write,
    app: &mut App,
    lines: &[FlatLine],
    full: bool,
) -> std::io::Result<()> {
    let cols = usize::from(app.cols.max(1));
    let content_rows = app.content_rows().max(1) as usize;
    let mut rows: Vec<Vec<u8>> = Vec::new();
    if full {
        queue!(out, Clear(ClearType::All))?;
        app.frame_prev.clear();
    }

    // Row 0: tab bar with tabs then a "+" (new filter tab). Column spans are
    // recorded for mouse hit-testing.
    let mut row0 = Vec::new();
    let mut x: u16 = 0;
    let tabs: Vec<(usize, String, bool)> = app
        .stats
        .as_ref()
        .map(|s| {
            s.tabs
                .iter()
                .map(|t| (t.index, t.name.clone(), t.index == s.active_tab))
                .collect()
        })
        .unwrap_or_default();
    app.tab_spans.clear();
    {
        let buf = &mut row0;
        for (index, name, active) in &tabs {
            let label = if *active {
                format!(" [{name}] ")
            } else {
                format!("  {name}  ")
            };
            if x >= cols as u16 {
                break;
            }
            let color = if *active {
                Color::White
            } else {
                Color::DarkGrey
            };
            let _ = queue!(buf, SetForegroundColor(color), Print(&label));
            app.tab_spans.push((x, label.chars().count() as u16, *index));
            x = x.saturating_add(label.chars().count() as u16);
        }
        if x + 3 < cols as u16 {
            let _ = queue!(buf, SetForegroundColor(Color::Cyan), Print(" + "));
            app.tab_add_span = Some((x, 3));
        } else {
            app.tab_add_span = None;
        }
    }
    build_row(&mut row0, 0, |_| {});
    rows.push(row0);

    // Content rows 1..1+content_rows.
    app.row_records.clear();
    for (i, line) in lines.iter().enumerate().take(content_rows) {
        app.row_records.push(Some(line.record_id));
        let hl = highlight_span(app, i);
        let mut buf = Vec::new();
        let content = |buf: &mut Vec<u8>| {
            let mark = if line.collapsible {
                let _ = queue!(buf, SetForegroundColor(Color::DarkGrey));
                if line.collapsed {
                    '+'
                } else {
                    '-'
                }
            } else {
                ' '
            };
            let _ = queue!(buf, Print(mark));
            if let Some(level) = line.level {
                let _ = queue!(
                    buf,
                    SetForegroundColor(level_color(level)),
                    Print(level_tag(level))
                );
            } else {
                let _ = queue!(buf, SetForegroundColor(Color::Reset), Print(' '));
            }
            queue_segments(buf, &line.segments, cols.saturating_sub(2), hl);
        };
        build_row(&mut buf, (i + 1) as u16, content);
        rows.push(buf);
    }
    // Rows the previous frame used but this one doesn't (content shrank):
    // emit clear-row buffers; the diff writes them exactly once.
    let prev_len = app.frame_prev.len();
    for row in lines.len().min(content_rows)..prev_len.saturating_sub(2) {
        let y = (row + 1) as u16;
        if y.saturating_add(1) < app.rows {
            let mut buf = Vec::new();
            let _ = queue!(buf, MoveTo(0, y), Clear(ClearType::CurrentLine));
            rows.push(buf);
        }
    }

    // Input line (second to last): filter prompt while focused, else a hint.
    let status_row = app.rows.saturating_sub(1);
    let input_row = status_row.saturating_sub(1);
    let mut input_buf = Vec::new();
    build_row(&mut input_buf, input_row, |buf| {
        if app.input_focus {
            let _ = queue!(
                buf,
                SetForegroundColor(Color::DarkCyan),
                Print(truncate(
                    &format!("filter include: {}_  (Enter apply, Esc cancel)", app.filter_buf),
                    cols
                ))
            );
        } else if let Some(banner) = &app.exited {
            let _ = queue!(
                buf,
                SetForegroundColor(Color::Yellow),
                Print(truncate(
                    &format!("{banner} — R reconnect · N new · Q quit"),
                    cols
                ))
            );
        } else if app.confirm_quit {
            let _ = queue!(
                buf,
                SetForegroundColor(Color::Yellow),
                Print(truncate("Quit NoViewLog? Ctrl+Q again to confirm", cols))
            );
        } else {
            let _ = queue!(
                buf,
                SetForegroundColor(Color::DarkCyan),
                Print(truncate(
                    "select text = copy · wheel = scroll · [tab] tabs · + filter · Ctrl+Q quit",
                    cols
                ))
            );
        }
    });
    rows.push(input_buf);

    let status = match app.stats.as_ref() {
        Some(s) => format!(
            "{} | lines {} | follow {} | filters {}{}",
            if s.status.is_empty() {
                if s.running {
                    "running"
                } else {
                    "stopped"
                }
            } else {
                &s.status
            },
            s.lines,
            if s.auto_follow { "on" } else { "off" },
            s.filters.len(),
            if s.dropped > 0 {
                format!(" | dropped {}", s.dropped)
            } else {
                String::new()
            }
        ),
        None => "starting…".to_string(),
    };
    let mut status_buf = Vec::new();
    build_row(&mut status_buf, status_row, |buf| {
        let _ = queue!(
            buf,
            SetForegroundColor(Color::DarkGrey),
            Print(truncate(&status, cols))
        );
    });
    rows.push(status_buf);

    // Write only rows whose bytes changed.
    for (i, buf) in rows.iter().enumerate() {
        if app.frame_prev.get(i) != Some(buf) {
            out.write_all(buf)?;
        }
    }
    app.frame_prev = rows;

    // Context menu overlay (drawn last, on top).
    if let Some(menu) = &app.menu {
        let width = 24usize.min(cols.saturating_sub(usize::from(menu.col)));
        let mut buf = Vec::new();
        let _ = queue!(buf, SetForegroundColor(Color::White), SetBackgroundColor(Color::DarkBlue));
        let top: String = format!("+{:-<width$}+", "");
        let _ = queue!(buf, MoveTo(menu.col, menu.row), Print(top));
        for (idx, item) in menu.items.iter().enumerate() {
            let line = format!("| {:<width$} |", item);
            let _ = queue!(buf, MoveTo(menu.col, menu.row + idx as u16 + 1), Print(line));
        }
        let bottom: String = format!("+{:-<width$}+", "");
        let _ = queue!(
            buf,
            MoveTo(menu.col, menu.row + menu.items.len() as u16 + 1),
            Print(bottom)
        );
        let _ = queue!(buf, SetBackgroundColor(Color::Reset));
        out.write_all(&buf)?;
    }

    // Connect overlay: centered profile list (same geometry as
    // App::connect_geo — 40 cols, items = profiles + "local shell").
    if app.connect_open {
        let items = app.connect_items();
        let (col, row, count) = app.connect_geo();
        let width = 38usize.min(cols.saturating_sub(usize::from(col)).saturating_sub(2));
        let mut buf = Vec::new();
        let _ = queue!(buf, SetForegroundColor(Color::White), SetBackgroundColor(Color::DarkBlue));
        let top: String = format!("+{:-<width$}+", "");
        let _ = queue!(buf, MoveTo(col, row), Print(top));
        let title = if app.profiles.is_empty() {
            " no ssh profiles — add tui_ssh_profiles ".to_string()
        } else {
            " connect ".to_string()
        };
        let _ = queue!(buf, MoveTo(col, row + 1), Print(format!("|{title:^width$}|")));
        for (idx, item) in items.iter().enumerate().take(count) {
            let line = format!("| {:<width$} |", truncate(item, width.saturating_sub(2)));
            let _ = queue!(buf, MoveTo(col, row + idx as u16 + 2), Print(line));
        }
        let bottom: String = format!("+{:-<width$}+", "");
        let _ = queue!(buf, MoveTo(col, row + count as u16 + 2), Print(bottom));
        let _ = queue!(buf, SetBackgroundColor(Color::Reset));
        out.write_all(&buf)?;
    }
    Ok(())
}

fn truncate(s: &str, cols: usize) -> String {
    s.chars().take(cols).collect()
}
