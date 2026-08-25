//! Terminal SCREEN + SCROLLBACK emulation (**live VT layer**).
//!
//! # Dual ANSI stack (read this before changing color / escape handling)
//!
//! | Layer | Module | Owns |
//! |-------|--------|------|
//! | **Live VT** (this file) | `core::terminal` | `vte` grid + scrollback; cursor, erase, OSC 7 |
//! | **Line SGR** | [`crate::core::ansi`] | Parse/strip/overlay SGR on stored record lines |
//!
//! This module re-serializes each committed / live row to ANSI so
//! [`crate::core::ansi::parse_ansi_line`] can style records. Fix live-screen
//! bugs here; fix filter/display coloring of stored lines in `ansi.rs`.
//!
//! ora / listr2 / ink render progress by manipulating the terminal *screen*
//! (cursor up/down, erase line, carriage return, redraw a block of lines).
//! A line-oriented buffer with `\r`/CSI collapse heuristics cannot reproduce
//! this — it either glues frames together or loses finalized lines.
//!
//! This module models a real terminal: a fixed-height grid the cursor moves
//! around on, plus a scrollback of lines that have scrolled off the top. Lines
//! that scroll off are *committed* permanently to the log buffer; the active
//! on-screen region is rendered live and repaints in place, so spinners replace
//! correctly while every finalized line (tables, ✔ steps, banners) survives.

use vte::{Params, Parser, Perform};

use crate::core::ansi::strip_ansi;
use crate::core::buffer::RecordBuffer;
use crate::core::parser::RecordParser;
use crate::core::types::{detect_level, LogRecord};

const DEFAULT_COLS: usize = 120;
const DEFAULT_ROWS: usize = 40;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    /// Basic SGR code (30-37 / 90-97 for fg, 40-47 / 100-107 for bg).
    Basic(u16),
    /// 256-colour palette index.
    Ext(u16),
    Rgb(u8, u8, u8),
}

#[derive(Clone, PartialEq, Eq, Default)]
struct Pen {
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    dim: bool,
    underline: bool,
}

impl Pen {
    fn is_default(&self) -> bool {
        *self == Pen::default()
    }

    /// Emit the SGR parameter body (without `ESC[` / `m`) for this pen.
    /// Always leads with `0` so any previously-active style is reset first.
    fn sgr_body(&self) -> String {
        let mut parts: Vec<String> = vec!["0".to_string()];
        if self.bold {
            parts.push("1".to_string());
        }
        if self.dim {
            parts.push("2".to_string());
        }
        if self.underline {
            parts.push("4".to_string());
        }
        if let Some(c) = self.fg {
            parts.push(color_sgr(c, true));
        }
        if let Some(c) = self.bg {
            parts.push(color_sgr(c, false));
        }
        parts.join(";")
    }
}

fn color_sgr(c: Color, _fg: bool) -> String {
    match c {
        // Basic codes already encode fg vs bg (30.. vs 40..).
        Color::Basic(code) => code.to_string(),
        Color::Ext(n) => {
            if _fg {
                format!("38;5;{n}")
            } else {
                format!("48;5;{n}")
            }
        }
        Color::Rgb(r, g, b) => {
            if _fg {
                format!("38;2;{r};{g};{b}")
            } else {
                format!("48;2;{r};{g};{b}")
            }
        }
    }
}

#[derive(Clone)]
struct Cell {
    ch: char,
    pen: Pen,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            pen: Pen::default(),
        }
    }
}

impl Cell {
    fn is_blank(&self) -> bool {
        self.ch == ' ' && self.pen.is_default()
    }
}

#[derive(Clone)]
struct Row {
    cells: Vec<Cell>,
    /// True when the row overflowed into the next one via auto-wrap (no explicit
    /// newline). Used to re-join wrapped rows into one logical log line.
    wrapped: bool,
}

impl Row {
    fn new(cols: usize) -> Self {
        Row {
            cells: vec![Cell::default(); cols],
            wrapped: false,
        }
    }

    fn clear(&mut self) {
        for c in &mut self.cells {
            *c = Cell::default();
        }
        self.wrapped = false;
    }

    /// Serialize to an ANSI string. Trailing blanks are trimmed only when the
    /// row is a true line end; auto-wrapped rows keep their full width so a
    /// space that landed on the wrap column is not lost when re-joining.
    fn serialize(&self) -> String {
        let last = if self.wrapped {
            self.cells.len()
        } else {
            self.cells
                .iter()
                .rposition(|c| !c.is_blank())
                .map(|i| i + 1)
                .unwrap_or(0)
        };
        let mut out = String::new();
        let mut cur = Pen::default();
        for cell in &self.cells[..last] {
            if cell.pen != cur {
                out.push_str("\x1b[");
                out.push_str(&cell.pen.sgr_body());
                out.push('m');
                cur = cell.pen.clone();
            }
            out.push(cell.ch);
        }
        if !cur.is_default() {
            out.push_str("\x1b[0m");
        }
        out
    }
}

/// Caret position within [`TerminalEmulator::screen_lines`] (logical rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenCursor {
    /// Index into the logical screen-line list (joined wraps).
    pub line: usize,
    /// Cell column within that logical line (may past end of trimmed text).
    pub col: usize,
}

/// A terminal screen with scrollback commit + colour-preserving serialization.
pub struct TerminalEmulator {
    cols: usize,
    rows: usize,
    screen: Vec<Row>,
    cursor_row: usize,
    cursor_col: usize,
    cursor_visible: bool,
    pen: Pen,
    /// Pending auto-wrap: next print wraps first (DEC autowrap).
    wrap_pending: bool,
    /// Finalized lines that scrolled off the top, ready to drain.
    committed: Vec<String>,
    /// Accumulator for a logical line whose rows are still auto-wrapping.
    wrap_accum: Option<String>,
    /// Latest OSC 7 working directory (file://…), if reported by the shell.
    pending_cwd: Option<String>,
}

impl Default for TerminalEmulator {
    fn default() -> Self {
        Self::new(DEFAULT_COLS, DEFAULT_ROWS)
    }
}

impl TerminalEmulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        TerminalEmulator {
            cols,
            rows,
            screen: vec![Row::new(cols); rows],
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            pen: Pen::default(),
            wrap_pending: false,
            committed: Vec::new(),
            wrap_accum: None,
            pending_cwd: None,
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Resize the live screen grid to match the viewport / PTY winsize.
    ///
    /// Column changes reflow auto-wrapped logical lines onto the new grid so
    /// soft-wrap / horizontal scroll still see one long line instead of
    /// fragmented hard-wrap slices. Already-committed scrollback is untouched.
    /// Height shrink commits rows that scroll off the top; growth pads blanks.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }

        if cols != self.cols {
            let logical = self.plain_screen_lines();
            let cursor = self.screen_cursor();
            let cursor_visible = self.cursor_visible;
            let pen = self.pen.clone();

            self.cols = cols;
            self.rows = rows;
            self.screen = vec![Row::new(cols); rows];
            self.cursor_row = 0;
            self.cursor_col = 0;
            self.wrap_pending = false;
            self.wrap_accum = None;
            self.pen = Pen::default();

            for (i, line) in logical.iter().enumerate() {
                if i > 0 {
                    self.wrap_pending = false;
                    self.cursor_col = 0;
                    self.line_feed();
                }
                for ch in line.chars() {
                    self.write_char(ch);
                }
            }

            // Best-effort caret restore within the reflowed logical lines.
            self.cursor_row = 0;
            self.cursor_col = 0;
            self.wrap_pending = false;
            for _ in 0..cursor.line {
                if self.cursor_row + 1 < self.rows {
                    self.cursor_row += 1;
                }
            }
            // Walk to the target column, following auto-wrap like write_char.
            let mut remaining = cursor.col;
            while remaining > 0 {
                if self.cursor_col + 1 >= self.cols {
                    self.screen[self.cursor_row].wrapped = true;
                    self.cursor_col = 0;
                    self.line_feed();
                } else {
                    self.cursor_col += 1;
                }
                remaining -= 1;
            }
            self.cursor_visible = cursor_visible;
            self.pen = pen;
            return;
        }

        while self.screen.len() > rows {
            if self.cursor_row + 1 > rows {
                let row = self.screen.remove(0);
                self.commit_row(&row);
                self.cursor_row = self.cursor_row.saturating_sub(1);
            } else {
                self.screen.pop();
            }
        }
        while self.screen.len() < rows {
            self.screen.push(Row::new(self.cols));
        }
        self.rows = rows;
        self.cursor_col = self.cursor_col.min(self.cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(self.rows.saturating_sub(1));
        self.wrap_pending = false;
    }

    /// Like [`Self::screen_lines`], but plain text from cells (no SGR) — used
    /// when reflowing the live grid on a column resize.
    fn plain_screen_lines(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut acc = String::new();
        for row in &self.screen {
            let last = if row.wrapped {
                row.cells.len()
            } else {
                row.cells
                    .iter()
                    .rposition(|c| !c.is_blank())
                    .map(|i| i + 1)
                    .unwrap_or(0)
            };
            for cell in &row.cells[..last] {
                acc.push(cell.ch);
            }
            if !row.wrapped {
                out.push(std::mem::take(&mut acc));
            }
        }
        if !acc.is_empty() {
            out.push(acc);
        }
        let keep_through = self.screen_cursor().line;
        while out.len() <= keep_through {
            out.push(String::new());
        }
        while out.len() > keep_through + 1
            && out.last().map(|s| s.is_empty()).unwrap_or(false)
        {
            out.pop();
        }
        out
    }

    /// Drain lines that have scrolled off the screen (finalized).
    pub fn take_committed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.committed)
    }

    pub fn take_pending_cwd(&mut self) -> Option<String> {
        self.pending_cwd.take()
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// Map the grid cursor into the logical screen-line coordinate system
    /// (auto-wrapped rows joined), matching [`Self::screen_lines`].
    pub fn screen_cursor(&self) -> ScreenCursor {
        let mut line = 0usize;
        let mut col_base = 0usize;
        for r in 0..self.cursor_row {
            if self.screen[r].wrapped {
                col_base += self.cols;
            } else {
                line += 1;
                col_base = 0;
            }
        }
        ScreenCursor {
            line,
            col: col_base + self.cursor_col.min(self.cols.saturating_sub(1)),
        }
    }

    /// The current live on-screen rows (auto-wrapped rows re-joined).
    /// Trailing blank lines are trimmed, except those needed so the caret
    /// row remains present (empty prompt line, cursor below content, etc.).
    pub fn screen_lines(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut acc = String::new();
        for row in &self.screen {
            acc.push_str(&row.serialize());
            if !row.wrapped {
                out.push(std::mem::take(&mut acc));
            }
        }
        if !acc.is_empty() {
            out.push(acc);
        }
        let keep_through = self.screen_cursor().line;
        while out.len() <= keep_through {
            out.push(String::new());
        }
        while out.len() > keep_through + 1
            && out.last().map(|s| s.is_empty()).unwrap_or(false)
        {
            out.pop();
        }
        out
    }

    /// Flush the screen into `committed` (called at process exit / EOF), dropping
    /// the trailing unused blank rows of the grid.
    pub fn flush_all(&mut self) {
        let last = (0..self.rows).rev().find(|&i| !self.row_is_empty(i));
        if let Some(last) = last {
            for i in 0..=last {
                let row = std::mem::replace(&mut self.screen[i], Row::new(self.cols));
                self.commit_row(&row);
            }
        }
        if let Some(rem) = self.wrap_accum.take() {
            self.committed.push(rem);
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.cursor_visible = true;
    }

    fn row_is_empty(&self, i: usize) -> bool {
        let row = &self.screen[i];
        !row.wrapped && row.cells.iter().all(Cell::is_blank)
    }

    // ---- internal grid ops ----

    fn commit_row(&mut self, row: &Row) {
        let s = row.serialize();
        let joined = match self.wrap_accum.take() {
            Some(prev) => prev + &s,
            None => s,
        };
        if row.wrapped {
            self.wrap_accum = Some(joined);
        } else {
            self.committed.push(joined);
        }
    }

    fn scroll_up(&mut self) {
        let row = std::mem::replace(&mut self.screen[0], Row::new(self.cols));
        self.commit_row(&row);
        self.screen.remove(0);
        self.screen.push(Row::new(self.cols));
    }

    fn line_feed(&mut self) {
        if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        } else {
            self.scroll_up();
        }
    }

    fn write_char(&mut self, ch: char) {
        if self.wrap_pending {
            // Finish the auto-wrap deferred from the previous cell.
            self.screen[self.cursor_row].wrapped = true;
            self.wrap_pending = false;
            self.cursor_col = 0;
            self.line_feed();
        }
        let col = self.cursor_col.min(self.cols - 1);
        let row = &mut self.screen[self.cursor_row];
        row.cells[col] = Cell {
            ch,
            pen: self.pen.clone(),
        };
        if self.cursor_col + 1 >= self.cols {
            // Stay on the last column; defer the wrap until the next glyph.
            self.wrap_pending = true;
        } else {
            self.cursor_col += 1;
        }
    }

    fn erase_line(&mut self, mode: u16) {
        self.wrap_pending = false;
        let col = self.cursor_col.min(self.cols - 1);
        let row = &mut self.screen[self.cursor_row];
        match mode {
            1 => {
                for c in &mut row.cells[..=col] {
                    *c = Cell::default();
                }
            }
            2 => {
                row.clear();
            }
            _ => {
                for c in &mut row.cells[col..] {
                    *c = Cell::default();
                }
            }
        }
    }

    /// CSI `P` — Delete Character (DCH): shift cells left from the cursor.
    fn delete_chars(&mut self, count: usize) {
        self.wrap_pending = false;
        let col = self.cursor_col.min(self.cols - 1);
        let n = count.min(self.cols - col);
        if n == 0 {
            return;
        }
        let row = &mut self.screen[self.cursor_row];
        for i in col..(self.cols - n) {
            row.cells[i] = row.cells[i + n].clone();
        }
        for c in &mut row.cells[(self.cols - n)..] {
            *c = Cell::default();
        }
    }

    /// CSI `@` — Insert Character (ICH): shift cells right from the cursor.
    fn insert_chars(&mut self, count: usize) {
        self.wrap_pending = false;
        let col = self.cursor_col.min(self.cols - 1);
        let n = count.min(self.cols - col);
        if n == 0 {
            return;
        }
        let row = &mut self.screen[self.cursor_row];
        for i in ((col + n)..self.cols).rev() {
            row.cells[i] = row.cells[i - n].clone();
        }
        for c in &mut row.cells[col..(col + n)] {
            *c = Cell::default();
        }
    }

    /// CSI `X` — Erase Character (ECH): blank cells without shifting.
    fn erase_chars(&mut self, count: usize) {
        self.wrap_pending = false;
        let col = self.cursor_col.min(self.cols - 1);
        let end = (col + count).min(self.cols);
        let row = &mut self.screen[self.cursor_row];
        for c in &mut row.cells[col..end] {
            *c = Cell::default();
        }
    }

    fn erase_display(&mut self, mode: u16) {
        self.wrap_pending = false;
        match mode {
            1 => {
                for r in 0..self.cursor_row {
                    self.screen[r].clear();
                }
                self.erase_line(1);
            }
            2 | 3 => {
                for r in 0..self.rows {
                    self.screen[r].clear();
                }
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            _ => {
                self.erase_line(0);
                for r in (self.cursor_row + 1)..self.rows {
                    self.screen[r].clear();
                }
            }
        }
    }

    fn apply_sgr(&mut self, codes: &[u16]) {
        if codes.is_empty() {
            self.pen = Pen::default();
            return;
        }
        let mut i = 0;
        while i < codes.len() {
            match codes[i] {
                0 => self.pen = Pen::default(),
                1 => self.pen.bold = true,
                2 => self.pen.dim = true,
                4 => self.pen.underline = true,
                22 => {
                    self.pen.bold = false;
                    self.pen.dim = false;
                }
                24 => self.pen.underline = false,
                39 => self.pen.fg = None,
                49 => self.pen.bg = None,
                n @ (30..=37 | 90..=97) => self.pen.fg = Some(Color::Basic(n)),
                n @ (40..=47 | 100..=107) => self.pen.bg = Some(Color::Basic(n)),
                38 | 48 => {
                    let is_fg = codes[i] == 38;
                    let color = match codes.get(i + 1).copied() {
                        Some(5) => {
                            let n = codes.get(i + 2).copied().unwrap_or(0);
                            i += 2;
                            Some(Color::Ext(n))
                        }
                        Some(2) => {
                            let r = codes.get(i + 2).copied().unwrap_or(0) as u8;
                            let g = codes.get(i + 3).copied().unwrap_or(0) as u8;
                            let b = codes.get(i + 4).copied().unwrap_or(0) as u8;
                            i += 4;
                            Some(Color::Rgb(r, g, b))
                        }
                        _ => None,
                    };
                    if let Some(c) = color {
                        if is_fg {
                            self.pen.fg = Some(c);
                        } else {
                            self.pen.bg = Some(c);
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

fn first_param(params: &Params) -> u16 {
    params.iter().next().and_then(|p| p.first().copied()).unwrap_or(0)
}

fn nth_param(params: &Params, n: usize) -> u16 {
    params.iter().nth(n).and_then(|p| p.first().copied()).unwrap_or(0)
}

impl Perform for TerminalEmulator {
    fn print(&mut self, c: char) {
        self.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => {
                self.wrap_pending = false;
                self.line_feed();
            }
            b'\r' => {
                self.wrap_pending = false;
                self.cursor_col = 0;
            }
            b'\t' => {
                let next = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next.min(self.cols - 1);
                self.wrap_pending = false;
            }
            0x08 => {
                self.cursor_col = self.cursor_col.saturating_sub(1);
                self.wrap_pending = false;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        // Most DEC private modes (bracketed paste, etc.) are ignored; cursor
        // visibility (`?25`) is tracked so the viewport caret can hide.
        let private = intermediates.contains(&b'?');
        match action {
            'm' if !private => {
                let mut codes: Vec<u16> = Vec::new();
                for p in params.iter() {
                    if p.is_empty() {
                        codes.push(0);
                    } else {
                        codes.extend_from_slice(p);
                    }
                }
                self.apply_sgr(&codes);
            }
            'h' if private => {
                if first_param(params) == 25 {
                    self.cursor_visible = true;
                }
            }
            'l' if private => {
                if first_param(params) == 25 {
                    self.cursor_visible = false;
                }
            }
            'A' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.wrap_pending = false;
            }
            'B' | 'e' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
                self.wrap_pending = false;
            }
            'C' | 'a' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
                self.wrap_pending = false;
            }
            'D' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_col = self.cursor_col.saturating_sub(n);
                self.wrap_pending = false;
            }
            'E' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
                self.cursor_col = 0;
                self.wrap_pending = false;
            }
            'F' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
                self.wrap_pending = false;
            }
            'G' | '`' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_col = (n - 1).min(self.cols - 1);
                self.wrap_pending = false;
            }
            'd' => {
                let n = first_param(params).max(1) as usize;
                self.cursor_row = (n - 1).min(self.rows - 1);
                self.wrap_pending = false;
            }
            'H' | 'f' => {
                let r = nth_param(params, 0).max(1) as usize;
                let c = nth_param(params, 1).max(1) as usize;
                self.cursor_row = (r - 1).min(self.rows - 1);
                self.cursor_col = (c - 1).min(self.cols - 1);
                self.wrap_pending = false;
            }
            'J' => self.erase_display(first_param(params)),
            'K' => self.erase_line(first_param(params)),
            // Character edit — required for readline mid-line Delete (CSI 3~ → DCH).
            'P' if !private => {
                let n = first_param(params).max(1) as usize;
                self.delete_chars(n);
            }
            '@' if !private => {
                let n = first_param(params).max(1) as usize;
                self.insert_chars(n);
            }
            'X' if !private => {
                let n = first_param(params).max(1) as usize;
                self.erase_chars(n);
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'D' => {
                self.wrap_pending = false;
                self.line_feed();
            }
            b'M' => {
                // Reverse index: cursor up, scroll down at top.
                self.wrap_pending = false;
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                }
            }
            b'E' => {
                self.wrap_pending = false;
                self.cursor_col = 0;
                self.line_feed();
            }
            b'c' => {
                // RIS full reset.
                self.flush_all();
                self.pen = Pen::default();
                for r in 0..self.rows {
                    self.screen[r].clear();
                }
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 7 ; <uri> — shell reports current working directory.
        if params.first().is_some_and(|p| p == b"7") {
            if let Some(uri) = params.get(1) {
                if let Ok(s) = std::str::from_utf8(uri) {
                    if let Some(path) = parse_osc7_cwd(s) {
                        self.pending_cwd = Some(path);
                    }
                }
            }
        }
    }
}

/// Parse OSC 7 payloads like `file://hostname/path` or plain paths into a local path.
pub fn parse_osc7_cwd(payload: &str) -> Option<String> {
    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }
    if let Some(rest) = payload.strip_prefix("file://") {
        let path = if let Some(slash) = rest.find('/') {
            &rest[slash..]
        } else {
            rest
        };
        let decoded = percent_decode_basic(path);
        if decoded.is_empty() {
            return None;
        }
        return Some(decoded);
    }
    if payload.starts_with('/') || payload.starts_with('~') {
        return Some(payload.to_string());
    }
    None
}

fn percent_decode_basic(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn make_volatile_record(id: u64, ansi_line: &str) -> LogRecord {
    let text = strip_ansi(ansi_line);
    let level = detect_level(&text);
    LogRecord {
        id,
        lines: vec![ansi_line.to_string()],
        text,
        received_at: chrono::Utc::now(),
        level,
        overwrite: true,
    }
}

/// Drives the [`TerminalEmulator`] into a [`RecordBuffer`] + [`RecordParser`].
///
/// Committed (scrolled-off) rows flow through the parser so multiline records
/// (stack traces) still group and filters apply. The live on-screen
/// region is appended as a *volatile tail* of single-line records that are
/// replaced on every update — that is what makes spinners repaint in place
/// without gluing or losing finalized lines.
pub struct TerminalIngest {
    parser: Parser,
    emu: TerminalEmulator,
    volatile_count: usize,
    volatile_id: u64,
}

impl Default for TerminalIngest {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalIngest {
    pub fn new() -> Self {
        TerminalIngest {
            parser: Parser::new(),
            emu: TerminalEmulator::default(),
            volatile_count: 0,
            volatile_id: u64::MAX / 2,
        }
    }

    pub fn new_with_size(cols: usize, rows: usize) -> Self {
        TerminalIngest {
            parser: Parser::new(),
            emu: TerminalEmulator::new(cols, rows),
            volatile_count: 0,
            volatile_id: u64::MAX / 2,
        }
    }

    /// Match emulator geometry to the viewport / PTY. Refreshes the volatile
    /// tail so live screen lines reflect the new grid.
    pub fn resize(
        &mut self,
        cols: usize,
        rows: usize,
        buffer: &mut RecordBuffer,
        parser: &mut RecordParser,
    ) {
        if cols == self.emu.cols() && rows == self.emu.rows() {
            return;
        }
        self.strip_volatile(buffer);
        self.emu.resize(cols, rows);
        self.commit_available(buffer, parser);
        self.restore_volatile(buffer);
    }

    fn strip_volatile(&mut self, buffer: &mut RecordBuffer) {
        if self.volatile_count > 0 {
            buffer.pop_last(self.volatile_count);
            self.volatile_count = 0;
        }
    }

    fn restore_volatile(&mut self, buffer: &mut RecordBuffer) {
        let lines = self.emu.screen_lines();
        self.volatile_count = lines.len();
        for line in lines {
            self.volatile_id = self.volatile_id.wrapping_add(1);
            buffer.add(make_volatile_record(self.volatile_id, &line));
        }
    }

    fn commit_available(&mut self, buffer: &mut RecordBuffer, parser: &mut RecordParser) {
        for line in self.emu.take_committed() {
            for record in parser.push_line(line) {
                buffer.add(record);
            }
        }
        // Scrolled-off lines that are still "open" in the RecordParser would
        // otherwise sit invisible in pending until the next line or idle_flush
        // (~120ms) — Follow sees them vanish and reappear. Flush immediately.
        if let Some(rec) = parser.flush_pending() {
            buffer.add(rec);
        }
    }

    /// Feed a raw PTY byte chunk. Returns true if the buffer changed.
    /// Also applies any OSC 7 cwd update into `cwd_out` when provided.
    pub fn feed(
        &mut self,
        bytes: &[u8],
        buffer: &mut RecordBuffer,
        parser: &mut RecordParser,
    ) -> bool {
        self.strip_volatile(buffer);
        self.parser.advance(&mut self.emu, bytes);
        self.commit_available(buffer, parser);
        self.restore_volatile(buffer);
        true
    }

    pub fn take_cwd_update(&mut self) -> Option<String> {
        self.emu.take_pending_cwd()
    }

    /// Flush a still-pending parser record while keeping the volatile tail last.
    /// Called on idle so the most-recently-scrolled-off line is not stuck pending.
    pub fn idle_flush(&mut self, buffer: &mut RecordBuffer, parser: &mut RecordParser) -> bool {
        if !parser.has_pending() {
            return false;
        }
        self.strip_volatile(buffer);
        if let Some(rec) = parser.flush_pending() {
            buffer.add(rec);
        }
        self.restore_volatile(buffer);
        true
    }

    /// Finalize at process exit: commit the whole screen permanently.
    pub fn finish(&mut self, buffer: &mut RecordBuffer, parser: &mut RecordParser) {
        self.strip_volatile(buffer);
        self.emu.flush_all();
        self.commit_available(buffer, parser);
        if let Some(rec) = parser.flush_pending() {
            buffer.add(rec);
        }
    }

    pub fn reset(&mut self) {
        self.reset_with_size(DEFAULT_COLS, DEFAULT_ROWS);
    }

    pub fn reset_with_size(&mut self, cols: usize, rows: usize) {
        self.parser = Parser::new();
        self.emu = TerminalEmulator::new(cols, rows);
        self.volatile_count = 0;
    }

    pub fn size(&self) -> (usize, usize) {
        (self.emu.cols(), self.emu.rows())
    }

    /// Ensure the volatile tail exists even before the first PTY byte (empty
    /// screen with a visible caret at 0,0).
    pub fn ensure_live_screen(&mut self, buffer: &mut RecordBuffer) {
        if self.volatile_count > 0 {
            return;
        }
        self.restore_volatile(buffer);
    }

    /// Number of live (volatile) records currently appended to the buffer.
    pub fn volatile_count(&self) -> usize {
        self.volatile_count
    }

    /// Caret for the Console viewport, relative to the volatile tail.
    /// `None` when the cursor is hidden (`CSI ?25l`).
    pub fn viewport_caret(&self) -> Option<ScreenCursor> {
        if !self.emu.cursor_visible() {
            return None;
        }
        Some(self.emu.screen_cursor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::buffer::RecordBuffer;
    use crate::core::formats::get_builtin_format;
    use crate::core::parser::RecordParser;

    fn feed(emu: &mut TerminalEmulator, bytes: &[u8]) {
        let mut parser = Parser::new();
        parser.advance(emu, bytes);
    }

    #[test]
    fn cursor_advances_on_print() {
        let mut emu = TerminalEmulator::new(80, 24);
        feed(&mut emu, b"hi");
        assert_eq!(emu.screen_cursor(), ScreenCursor { line: 0, col: 2 });
        assert_eq!(emu.screen_lines(), vec!["hi".to_string()]);
    }

    #[test]
    fn cursor_moves_with_csi_and_backspace() {
        let mut emu = TerminalEmulator::new(80, 24);
        feed(&mut emu, b"abc\x08\x08");
        assert_eq!(emu.screen_cursor(), ScreenCursor { line: 0, col: 1 });
        feed(&mut emu, b"\x1b[C\x1b[C");
        assert_eq!(emu.screen_cursor(), ScreenCursor { line: 0, col: 3 });
        feed(&mut emu, b"\x1b[5;5H");
        assert_eq!(emu.screen_cursor(), ScreenCursor { line: 4, col: 4 });
    }

    #[test]
    fn screen_lines_keep_blank_row_for_cursor() {
        let mut emu = TerminalEmulator::new(80, 24);
        feed(&mut emu, b"one\r\n\x1b[2;1H");
        let lines = emu.screen_lines();
        assert!(lines.len() >= 2, "expected cursor row kept: {lines:?}");
        assert_eq!(lines[0], "one");
        assert_eq!(emu.screen_cursor().line, 1);
    }

    #[test]
    fn cursor_visibility_dec_mode() {
        let mut emu = TerminalEmulator::new(80, 24);
        assert!(emu.cursor_visible());
        feed(&mut emu, b"\x1b[?25l");
        assert!(!emu.cursor_visible());
        feed(&mut emu, b"\x1b[?25h");
        assert!(emu.cursor_visible());
    }

    #[test]
    fn dch_shifts_line_left_like_bash_delete() {
        // Bash mid-line Delete emits CSI 1 P, then may reprint the shifted
        // glyph and BS (`\x1b[1Pt\x08`). Without DCH that becomes `tett`.
        let mut emu = TerminalEmulator::new(80, 24);
        feed(&mut emu, b"test");
        feed(&mut emu, b"\x08\x08");
        assert_eq!(emu.screen_cursor(), ScreenCursor { line: 0, col: 2 });
        feed(&mut emu, b"\x1b[1Pt\x08");
        assert_eq!(emu.screen_lines(), vec!["tet".to_string()]);
        assert_eq!(emu.screen_cursor(), ScreenCursor { line: 0, col: 2 });
    }

    #[test]
    fn ich_and_ech_edit_cells() {
        let mut emu = TerminalEmulator::new(80, 24);
        feed(&mut emu, b"abcd\x08\x08\x08"); // cursor on 'b'
        feed(&mut emu, b"\x1b[@");
        assert_eq!(emu.screen_lines(), vec!["a bcd".to_string()]);
        feed(&mut emu, b"\x1b[P");
        assert_eq!(emu.screen_lines(), vec!["abcd".to_string()]);

        let mut emu = TerminalEmulator::new(80, 24);
        feed(&mut emu, b"abc\x08\x08"); // cursor on 'b'
        feed(&mut emu, b"\x1b[2X");
        assert_eq!(emu.screen_lines(), vec!["a".to_string()]);
        assert_eq!(emu.screen_cursor(), ScreenCursor { line: 0, col: 1 });
    }

    #[test]
    fn ingest_exposes_viewport_caret() {
        let mut ingest = TerminalIngest::new();
        let mut buffer = RecordBuffer::new(1000);
        let mut parser = RecordParser::new(get_builtin_format("node-default"));
        ingest.feed(b"$ hello", &mut buffer, &mut parser);
        let caret = ingest.viewport_caret().expect("visible");
        assert_eq!(caret, ScreenCursor { line: 0, col: 7 });
        assert_eq!(ingest.volatile_count(), 1);
    }

    #[test]
    fn resize_widens_autowrap_column() {
        let mut emu = TerminalEmulator::new(10, 5);
        feed(&mut emu, b"abcdefghijXYZ");
        // At 10 cols the "XYZ" starts a wrapped continuation row.
        assert!(emu.screen_lines()[0].len() >= 10);
        emu.resize(40, 5);
        assert_eq!(emu.cols(), 40);
        assert_eq!(emu.rows(), 5);
        // Reflow must keep one logical line (not fragment at the old wrap).
        assert_eq!(
            emu.screen_lines(),
            vec!["abcdefghijXYZ".to_string()],
            "column resize should reflow auto-wrapped content"
        );
        // After widen, new prints should not wrap at the old 10-col boundary.
        feed(&mut emu, b"\r\n");
        feed(&mut emu, &b"12345678901234567890".to_vec());
        let lines = emu.screen_lines();
        assert!(
            lines.iter().any(|l| l.contains("12345678901234567890")),
            "expected full 20-char line without 10-col hard wrap: {lines:?}"
        );
    }

    #[test]
    fn resize_narrows_reflows_logical_line() {
        let mut emu = TerminalEmulator::new(40, 5);
        feed(&mut emu, b"abcdefghijklmnopqrstuvwxyz");
        assert_eq!(emu.screen_lines().len(), 1);
        emu.resize(10, 8);
        let lines = emu.screen_lines();
        assert_eq!(lines.len(), 1, "still one logical line after narrow: {lines:?}");
        assert_eq!(lines[0], "abcdefghijklmnopqrstuvwxyz");
    }

    #[test]
    fn ingest_resize_updates_size() {
        let mut ingest = TerminalIngest::new_with_size(80, 24);
        let mut buffer = RecordBuffer::new(1000);
        let mut parser = RecordParser::new(get_builtin_format("node-default"));
        assert_eq!(ingest.size(), (80, 24));
        ingest.resize(160, 48, &mut buffer, &mut parser);
        assert_eq!(ingest.size(), (160, 48));
    }

    #[test]
    fn scrolled_off_line_is_committed_immediately() {
        // Tiny grid so the next LF scrolls the top line off. That line must
        // land in the buffer in the same feed() — not sit invisible in parser
        // pending until idle_flush (which caused disappear/reappear jumps).
        let mut ingest = TerminalIngest::new_with_size(40, 2);
        let mut buffer = RecordBuffer::new(1000);
        let mut parser = RecordParser::new(get_builtin_format("node-default"));

        ingest.feed(b"[LOG] first\r\n[LOG] second\r\n", &mut buffer, &mut parser);
        // Third line scrolls "first" off the 2-row screen.
        ingest.feed(b"[LOG] third\r\n", &mut buffer, &mut parser);

        assert!(
            !parser.has_pending(),
            "scrolled-off lines must not remain only in parser pending"
        );
        let volatile_floor = u64::MAX / 2;
        let committed: Vec<String> = buffer
            .records()
            .iter()
            .filter(|r| r.id < volatile_floor)
            .flat_map(|r| r.lines.iter().cloned())
            .map(|l| crate::core::ansi::strip_ansi(&l))
            .collect();
        assert!(
            committed.iter().any(|l| l.contains("[LOG] first")),
            "scrolled-off line must be in buffer immediately, got {committed:?}"
        );
        assert!(
            committed.iter().any(|l| l.contains("[LOG] second")),
            "expected second line committed, got {committed:?}"
        );
    }
}
