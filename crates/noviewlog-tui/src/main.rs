//! NoViewLog TUI host: the existing engine rendered as ANSI text on the
//! alternate screen, so the terminal + filters run inside any ANSI emulator
//! (VS Code / IDEA terminal panels, Windows Terminal, tmux).
//!
//! Interaction model (transparent terminal):
//! - Keys always go to the wrapped shell, like a normal terminal.
//! - Mouse: wheel scrolls our output; drag selects text (copy on release);
//!   tab-bar clicks switch/create filter tabs; clicking a collapsed record
//!   twice expands it.
//! - The only modal thing is the filter input line (Enter applies, Esc
//!   closes). Ctrl+Q quits with a y/N confirm.

mod render;
mod ssh;

use std::io::{self, stdout, Write};
use std::time::{Duration, Instant};

use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

use noviewlog_core::core::config::load_user_config;
use noviewlog_core::core::types::{
    FilterType, FlatLine, LaunchConfig, ShellPreference, SshProfile,
};
use noviewlog_core::spawn_resolve::resolve_interactive_shell;
use noviewlog_core::{parse_engine_event, Command, Engine, EngineEvent, StatsSnapshot};

use ssh::SessionChoice;

/// Frame budget: at most one paint per interval (flood-safe rendering).
const FRAME_BUDGET: Duration = Duration::from_millis(16);
/// Double-click window for record expand/collapse.
const DOUBLE_CLICK: Duration = Duration::from_millis(350);

/// Right-click context menu rendered as a text overlay on the grid.
struct Menu {
    /// Screen row/col of the box top-left.
    row: u16,
    col: u16,
    /// Actions for the record under the cursor (if any).
    record_id: Option<u64>,
    /// Whole text of the record's first line (for "copy line").
    line_text: String,
    items: Vec<&'static str>,
}

struct App {
    engine: Engine,
    stats: Option<StatsSnapshot>,
    /// Filter input line focus: while open, keys edit the pattern.
    input_focus: bool,
    filter_buf: String,
    confirm_quit: bool,
    /// Saved SSH profiles (user config `tui_ssh_profiles`).
    profiles: Vec<SshProfile>,
    /// The running (or last) session; `r` reconnects with the same argv.
    session: Option<SessionChoice>,
    /// True until the first session starts (connect overlay: nothing running).
    pending_start: bool,
    /// Session child exited: banner text; keys are gated (R/N/Q), never
    /// passed through to any shell (design D6).
    exited: Option<String>,
    /// SSH profile list overlay open (mouse-driven; Esc/outside = local).
    connect_open: bool,
    cols: u16,
    rows: u16,
    /// Cursor into the visible slice (keyboard record toggle); None = none.
    cursor: Option<usize>,
    /// Hit-testing for mouse clicks: (start_col, len, tab_index) per tab-bar cell.
    tab_spans: Vec<(u16, u16, usize)>,
    /// Column span of the "+" (new filter tab) cell in the tab bar.
    tab_add_span: Option<(u16, u16)>,
    /// record_id painted per content row (row 0 = first content row).
    row_records: Vec<Option<u64>>,
    /// Text selection drag state in content-cell coords (row, col), row 0 =
    /// first visible content row, col 0 = after the 2-char prefix.
    sel_anchor: Option<(usize, usize)>,
    sel_current: Option<(usize, usize)>,
    /// Last left-click (cell + time) for double-click detection.
    last_click: Option<((u16, u16), Instant)>,
    /// Visible flat lines snapshot (selection text source, menu context).
    visible: Vec<FlatLine>,
    /// Open context menu, if any.
    menu: Option<Menu>,
    /// Whether the previous frame had the menu open.
    menu_was_open: bool,
    /// Whether the previous frame had the connect overlay open.
    connect_was_open: bool,
    /// UI state changed (selection/menu/input) — repaint next frame even if
    /// the engine considers its viewport clean.
    ui_dirty: bool,
    /// Rendered byte buffers of the previous frame (diff painting).
    frame_prev: Vec<Vec<u8>>,
    last_paint: Option<Instant>,
}

impl App {
    fn new(cols: u16, rows: u16, profiles: Vec<SshProfile>) -> Result<Self, String> {
        let mut app = Self {
            engine: Engine::new(),
            stats: None,
            input_focus: false,
            filter_buf: String::new(),
            confirm_quit: false,
            profiles,
            session: None,
            pending_start: false,
            exited: None,
            connect_open: false,
            cols,
            rows,
            cursor: None,
            tab_spans: Vec::new(),
            tab_add_span: None,
            row_records: Vec::new(),
            sel_anchor: None,
            sel_current: None,
            last_click: None,
            visible: Vec::new(),
            menu: None,
            menu_was_open: false,
            connect_was_open: false,
            ui_dirty: true,
            frame_prev: Vec::new(),
            last_paint: None,
        };
        app.engine.finish_startup(LaunchConfig::default());
        // Whole flat lines are the TUI render unit (no font metrics here).
        app.engine
            .send_command(Command::SetWrapLines { wrap: false })?;
        Ok(app)
    }

    /// Start a session (local shell or ssh) on the engine PTY. Called from
    /// startup, the connect overlay, and `r` reconnect.
    fn start_session(&mut self, choice: SessionChoice) -> Result<(), String> {
        match choice.clone() {
            SessionChoice::Local => {
                let (shell, args, cwd) =
                    resolve_interactive_shell(&LaunchConfig::default(), ShellPreference::Auto)?;
                self.engine.send_command(Command::Start {
                    command: shell,
                    args,
                    cwd,
                })?;
            }
            SessionChoice::Ssh { label, argv } => {
                // No panic path on an empty argv (would currently be
                // unreachable, but build errors, not panics, surface it).
                let (command, rest) = argv
                    .split_first()
                    .ok_or_else(|| format!("ssh profile `{label}` has an empty command"))?;
                let args = rest.to_vec();
                self.engine.send_command(Command::Start {
                    command: command.clone(),
                    args,
                    cwd: None,
                })?;
                let _ = label; // shown via the tab/status lines
            }
        }
        self.session = Some(choice);
        self.pending_start = false;
        self.exited = None;
        self.connect_open = false;
        Ok(())
    }

    fn cmd(&mut self, c: Command) {
        if let Err(e) = self.engine.send_command(c) {
            // Surface engine rejections on the status line instead of crashing.
            if let Some(s) = self.stats.as_mut() {
                s.status = format!("cmd error: {e}");
            }
        }
    }

    /// Route a terminal paste: into the filter buffer (capped) while the
    /// input line is focused; otherwise to the wrapped shell as plain stdin.
    /// A dead session has no shell to receive it — drop silently instead of
    /// surfacing a cmd error banner (#241).
    fn handle_paste(&mut self, text: &str) {
        // Gate order mirrors handle_key: a dead session drops the paste
        // before the filter buffer is touched (#254).
        if self.exited.is_some() {
            return;
        }
        if self.input_focus {
            paste_append(&mut self.filter_buf, text);
        } else {
            self.cmd(Command::Stdin {
                text: String::new(),
                bytes: Some(text.as_bytes().to_vec()),
            });
        }
    }

    fn sync_geometry(&mut self) {
        // Cell units, not pixels: Command::Resize is bitmap-host pixel space.
        self.engine
            .set_terminal_grid(self.cols.max(1), self.content_rows().max(1));
    }

    fn content_rows(&self) -> u16 {
        self.rows.saturating_sub(3)
    }

    fn drain_events(&mut self) {
        while let Some(json) = self.engine.poll_event_json() {
            match parse_engine_event(&json) {
                Some(EngineEvent::Stats(s)) => self.stats = Some(s),
                Some(EngineEvent::Exit { code, message }) => {
                    let what = match self.session.as_ref() {
                        Some(SessionChoice::Ssh { label, .. }) => format!("ssh {label}"),
                        Some(SessionChoice::Local) => "shell".to_string(),
                        None => "session".to_string(),
                    };
                    let detail = if message.is_empty() {
                        format!("{what} exited (code {code})")
                    } else {
                        format!("{what} exited (code {code}): {message}")
                    };
                    self.exited = Some(detail);
                    self.ui_dirty = true;
                }
                Some(EngineEvent::Status { message }) => {
                    if let Some(s) = self.stats.as_mut() {
                        s.status = message;
                    }
                }
                Some(EngineEvent::Unknown) | None => {}
            }
        }
    }

    fn leave_follow(&mut self) {
        if self.stats.as_ref().is_some_and(|s| s.auto_follow) {
            self.cmd(Command::SetFollow { follow: false });
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<()> {
        // Any key may alter UI state (input line, confirm); repaint cheaply.
        self.ui_dirty = true;
        // Act on Press only; some hosts synthesize Release/Repeat events.
        if key.kind != KeyEventKind::Press {
            return Some(());
        }
        // Quit: Ctrl+Q with y/N confirm (typed text still reaches the shell).
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        {
            if self.confirm_quit {
                return None;
            }
            self.confirm_quit = true;
            return Some(());
        }
        if self.confirm_quit {
            self.confirm_quit = false;
            return Some(());
        }
        // Connect overlay: selection is mouse-only; Esc dismisses (local
        // shell when nothing runs yet, back to the banner otherwise).
        if self.connect_open {
            if key.code == KeyCode::Esc {
                self.connect_open = false;
                if self.session.is_none() {
                    let _ = self.start_session(SessionChoice::Local);
                }
            }
            return Some(());
        }
        // Session exited: keys are session controls only — nothing reaches a
        // shell until the user explicitly reconnects or starts a local one
        // (a disconnected SSH session must not leak keystrokes locally).
        if self.exited.is_some() {
            match key.code {
                KeyCode::Char('r' | 'R') => {
                    if let Some(choice) = self.session.clone() {
                        let _ = self.start_session(choice);
                    }
                }
                KeyCode::Char('n' | 'N') => {
                    self.connect_open = true;
                    self.ui_dirty = true;
                }
                KeyCode::Char('q' | 'Q') => return None,
                _ => {}
            }
            return Some(());
        }
        // The filter input line is the only modal element.
        if self.input_focus {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                KeyCode::Enter => {
                    let pattern = std::mem::take(&mut self.filter_buf);
                    if !pattern.is_empty() {
                        self.cmd(Command::FilterAdd {
                            filter_type: FilterType::Include,
                            pattern,
                            regex: false,
                        });
                    }
                    self.input_focus = false;
                }
                KeyCode::Esc => {
                    self.filter_buf.clear();
                    self.input_focus = false;
                }
                KeyCode::Backspace => {
                    self.filter_buf.pop();
                }
                // Ignore Ctrl-chords in the input line (Ctrl+A/E/U are edit
                // motions, not text) instead of typing the literal letter.
                KeyCode::Char(c) if !ctrl => self.filter_buf.push(c),
                _ => {}
            }
            return Some(());
        }
        // Transparent terminal: everything goes to the wrapped shell.
        let bytes = shell_key_bytes(&key);
        if !bytes.is_empty() {
            self.cmd(Command::Stdin {
                text: String::new(),
                bytes: Some(bytes),
            });
        }
        Some(())
    }

    /// Items of the connect overlay: profiles then the local shell entry.
    fn connect_items(&self) -> Vec<String> {
        let mut items: Vec<String> = self
            .profiles
            .iter()
            .map(|p| format!("ssh {}", p.target))
            .collect();
        items.push("local shell".to_string());
        items
    }

    /// Deterministic overlay geometry: centered box, 40 cols wide.
    /// (col, row, item_count) — same math as the painter in render.rs.
    fn connect_geo(&self) -> (u16, u16, usize) {
        let items = self.connect_items().len();
        let col = self.cols.saturating_sub(40) / 2;
        let row = self.rows.saturating_sub(items as u16 + 2) / 2;
        (col, row, items)
    }

    /// Inner width of the connect overlay box at left column `col` — the
    /// same clamp the painter uses, so hit-testing only maps to visibly
    /// drawn cells on narrow terminals (#241).
    pub(crate) fn connect_box_width(col: u16, cols: u16) -> u16 {
        38u16.min(cols.saturating_sub(col).saturating_sub(2))
    }

    /// Drawn width of the context menu box at left column `col` — the same
    /// clamp the painter uses, so the hit region only covers visibly drawn
    /// cells at any terminal width (#254).
    pub(crate) fn menu_width(col: u16, cols: u16) -> u16 {
        24u16.min(cols.saturating_sub(col))
    }

    fn handle_mouse(&mut self, m: MouseEvent) {
        // Selection/menu highlight must follow the pointer in real time.
        self.ui_dirty = true;
        match m.kind {
            MouseEventKind::ScrollUp => {
                self.leave_follow();
                self.cursor = None;
                self.cmd(Command::ScrollLines { delta: -3 });
            }
            MouseEventKind::ScrollDown => {
                // Windows Terminal behavior: reaching the bottom re-enters
                // follow so new output pins the view again.
                if self.engine.at_scroll_bottom() {
                    self.cmd(Command::SetFollow { follow: true });
                    self.cursor = None;
                } else {
                    self.leave_follow();
                    self.cursor = None;
                    self.cmd(Command::ScrollLines { delta: 3 });
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Right) => {
                if self.menu.is_some() {
                    self.menu = None;
                    return;
                }
                // Open a text menu over the grid at the click point.
                let row = m.row.min(self.rows.saturating_sub(6));
                let col = m.column.min(self.cols.saturating_sub(26));
                let content_row = m.row.saturating_sub(1) as usize;
                let ctx = self.row_records.get(content_row).copied().flatten();
                let line_text = self
                    .visible
                    .get(content_row)
                    .map(|l| {
                        l.segments
                            .iter()
                            .map(|s| s.text.as_str())
                            .collect::<String>()
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default();
                let collapsed = self.visible.get(content_row).is_some_and(|l| l.collapsed);
                self.menu = Some(Menu {
                    row,
                    col,
                    record_id: ctx,
                    line_text,
                    items: vec![
                        if collapsed {
                            "Expand record"
                        } else {
                            "Collapse record"
                        },
                        "Copy line",
                        "Filter include line",
                        "Cancel",
                    ],
                });
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // Connect overlay: click on an item starts that session;
                // Esc/outside falls back to the local shell.
                if self.connect_open {
                    // Box geometry matches the painter: title row at orow+1,
                    // items start at orow+2.
                    let (ocol, orow, items) = self.connect_geo();
                    let box_cols = Self::connect_box_width(ocol, self.cols);
                    let idx = if m.column >= ocol
                        && m.column < ocol.saturating_add(box_cols).saturating_add(2)
                        && m.row >= orow + 2
                        && ((m.row - orow - 2) as usize) < items
                    {
                        Some((m.row - orow - 2) as usize)
                    } else {
                        None
                    };
                    self.connect_open = false;
                    let choice = match idx {
                        Some(i) if i < self.profiles.len() => {
                            let p = &self.profiles[i];
                            let label = format!("{} ({})", p.name, p.target);
                            Some(SessionChoice::Ssh {
                                label,
                                argv: ssh::argv_for_profile(p),
                            })
                        }
                        Some(_) => Some(SessionChoice::Local),
                        None => None,
                    };
                    match choice {
                        Some(c) => {
                            let _ = self.start_session(c);
                        }
                        None if self.session.is_none() => {
                            // Dismissed with no session yet: default to local.
                            let _ = self.start_session(SessionChoice::Local);
                        }
                        _ => {}
                    }
                    return;
                }
                if let Some(menu) = &self.menu {
                    let items = menu.items.clone();
                    let record_id = menu.record_id;
                    let line_text = menu.line_text.clone();
                    let (mrow, mcol) = (menu.row, menu.col);
                    self.menu = None;
                    // Hit-test: item 0 = box top border row; items start one row below.
                    if m.column >= mcol
                        && m.column < mcol.saturating_add(Self::menu_width(mcol, self.cols))
                        && m.row > mrow
                        && (m.row - mrow - 1) as usize <= items.len()
                    {
                        match (m.row - mrow - 1) as usize {
                            0 if record_id.is_some() => {
                                self.cmd(Command::RecordCollapseToggle {
                                    record_id: record_id.unwrap(),
                                });
                            }
                            1 => copy_to_clipboard(&line_text),
                            2 if !line_text.is_empty() => {
                                self.cmd(Command::FilterAdd {
                                    filter_type: FilterType::Include,
                                    pattern: line_text,
                                    regex: false,
                                });
                            }
                            _ => {}
                        }
                    }
                    return;
                }
                if m.row == 0 {
                    if let Some(&(_, _, index)) = self
                        .tab_spans
                        .iter()
                        .find(|&&(s, l, _)| m.column >= s && m.column < s.saturating_add(l))
                    {
                        self.cmd(Command::TabSwitch { index });
                    } else if self
                        .tab_add_span
                        .is_some_and(|(s, l)| m.column >= s && m.column < s.saturating_add(l))
                    {
                        self.cmd(Command::TabAdd);
                        self.filter_buf.clear();
                        self.input_focus = true;
                    }
                    return;
                }
                if let Some(cell) = self.cell_at(&m) {
                    // Double-click toggles record expand/collapse; single
                    // clicks start a text selection.
                    let now = Instant::now();
                    let dbl = self.last_click.is_some_and(|((r, c), t)| {
                        (r, c) == (m.row, m.column) && now.duration_since(t) <= DOUBLE_CLICK
                    });
                    self.last_click = Some(((m.row, m.column), now));
                    if dbl {
                        if let Some(Some(id)) = self.row_records.get(cell.0) {
                            self.cmd(Command::RecordCollapseToggle { record_id: *id });
                        }
                        self.sel_anchor = None;
                        self.sel_current = None;
                        return;
                    }
                    self.sel_anchor = Some(cell);
                    self.sel_current = Some(cell);
                }
            }
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                if let Some(cell) = self.cell_at(&m) {
                    self.sel_current = Some(cell);
                }
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                // Keep the highlight; copy the selected text from the
                // visible slice on our own (no engine coordinate math).
                if let (Some(a), Some(b)) = (self.sel_anchor, self.sel_current) {
                    if a != b {
                        let text = self.selected_text(a.min(b), a.max(b));
                        if !text.is_empty() {
                            copy_to_clipboard(&text);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Text of the selected span across the visible slice (inclusive of the
    /// end cell's column on its last row; missing cells are skipped).
    fn selected_text(&self, a: (usize, usize), b: (usize, usize)) -> String {
        // Empty visible slice: `len().saturating_sub(1)` would saturate to
        // usize::MAX and the first index panics (#199).
        if self.visible.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for row in a.0..=b.0.min(self.visible.len().saturating_sub(1)) {
            let line = &self.visible[row];
            let text: String = line.segments.iter().map(|s| s.text.as_str()).collect();
            let start = if row == a.0 { a.1 } else { 0 };
            let end = if row == b.0 { b.1 } else { usize::MAX };
            let slice = text_in_cells(&text, start, end);
            if !slice.is_empty() {
                out.push_str(&slice);
            }
            if row != b.0 {
                out.push('\n');
            }
        }
        out
    }

    /// Content-cell coords of a mouse event: (row, col), row 0 = first
    /// visible content row, col 0 = after the 2-char prefix. None outside
    /// the content area.
    fn cell_at(&self, m: &MouseEvent) -> Option<(usize, usize)> {
        if m.row == 0 || m.row as usize > self.content_rows() as usize {
            return None;
        }
        Some(((m.row - 1) as usize, m.column.saturating_sub(2) as usize))
    }

    fn paint(&mut self) {
        self.tab_spans.clear();
        self.row_records.clear();
        let content = self.content_rows() as usize;
        let lines = self.engine.visible_flat_lines(content);
        self.visible = lines.clone();
        // Menu open/close transitions get one full repaint (overlay rows are
        // not part of the steady diff layout); same for the connect overlay.
        let full = (self.menu.is_some() != self.menu_was_open)
            || (self.connect_open != self.connect_was_open);
        self.menu_was_open = self.menu.is_some();
        self.connect_was_open = self.connect_open;
        let mut out = stdout();
        let _ = render::frame(&mut out, self, &lines, full);
        // Park the emulator caret where the wrapped shell's caret is, so
        // echoed/editing output lands where the user expects. While the
        // filter input is open the caret stays hidden (input line is ours).
        match self.engine.caret_visible_pos(content) {
            Some((r, c))
                if !self.input_focus
                    && self.exited.is_none()
                    && (r as u16) < self.content_rows() =>
            {
                let x = caret_x(c, self.cols);
                let _ = execute!(
                    out,
                    crossterm::cursor::Show,
                    crossterm::cursor::MoveTo(x, r as u16 + 1)
                );
            }
            _ => {
                let _ = execute!(out, crossterm::cursor::Hide);
            }
        }
        let _ = out.flush();
        self.ui_dirty = false;
        self.last_paint = Some(Instant::now());
    }
}

/// Part of `text` covering display-cell range `[start, end]` (end cell
/// inclusive). Selection coordinates are cells, not char indices: wide CJK
/// glyphs take two cells and zero-width marks take none, so chars are
/// included by the cell their glyph starts at (#254). Pure-ASCII lines keep
/// the exact span the old char-index slice produced.
fn text_in_cells(text: &str, start: usize, end: usize) -> String {
    let mut out = String::new();
    let mut cell = 0usize;
    for ch in text.chars() {
        if cell >= start && cell <= end {
            out.push(ch);
        }
        cell = cell.saturating_add(noviewlog_terminal::terminal::width::char_width(ch));
    }
    out
}

/// Screen x of the engine caret in column `c` (after the 2-char prefix):
/// clamp in usize first, then cast — clamping after `as u16` could truncate
/// to a wrong position instead of the right edge.
fn caret_x(c: usize, cols: u16) -> u16 {
    (c.min(usize::from(cols.saturating_sub(3))) as u16 + 2).min(cols.saturating_sub(1))
}

/// Paste cap for the filter buffer: a multi-megabyte clipboard paste must
/// not balloon the input line (the frame paints the whole buffer).
const FILTER_BUF_CAP: usize = 64 * 1024;

/// Append pasted text to the filter buffer up to [`FILTER_BUF_CAP`],
/// stopping on a char boundary (never a partial UTF-8 tail).
fn paste_append(buf: &mut String, text: &str) {
    for ch in text.chars() {
        if buf.len() + ch.len_utf8() > FILTER_BUF_CAP {
            break;
        }
        buf.push(ch);
    }
}

/// Encode a key event as PTY bytes (POSIX terminal encoding, valid for the
/// wrapped shell on both platforms).
fn shell_key_bytes(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => b"\x03".to_vec(),
        KeyCode::Char('d') if ctrl => b"\x04".to_vec(),
        KeyCode::Char(c) if ctrl => {
            // Ctrl + non-ASCII (e.g. Ctrl+ü) has no control-byte encoding —
            // send nothing instead of garbage ('ü' & 0x1f = FS) (#199).
            let upper = c.to_ascii_uppercase();
            if upper.is_ascii_alphabetic() {
                vec![upper as u8 & 0x1f]
            } else {
                Vec::new()
            }
        }
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Backspace => b"\x7f".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::Esc => b"\x1b".to_vec(),
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        _ => Vec::new(),
    }
}

/// Copy to the real clipboard: arboard first (native), OSC 52 fallback
/// (modern terminals sync their clipboard from it).
fn copy_to_clipboard(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        if cb.set_text(text.to_owned()).is_ok() {
            return;
        }
    }
    let mut out = stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64_encode(text.as_bytes()));
    let _ = out.flush();
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn main() {
    if let Err(e) = run() {
        let _ = restore_terminal();
        eprintln!("noviewlog-tui: {e}");
        std::process::exit(1);
    }
}

/// CLI: `--ssh <target>` (connect now), `--profile <name>` (saved profile),
/// `--connect` (profile list overlay), no args = local shell (unchanged).
enum CliChoice {
    Local,
    Ssh(SessionChoice),
    Connect,
}

fn parse_cli() -> Result<CliChoice, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = args.iter();
    let mut choice = CliChoice::Local;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--ssh" => {
                let target = iter.next().ok_or("--ssh requires a target (user@host)")?;
                ssh::probe_ssh_client()?;
                choice = CliChoice::Ssh(SessionChoice::Ssh {
                    label: target.clone(),
                    argv: ssh::build_ssh_argv(target, 0, ""),
                });
            }
            "--profile" => {
                let name = iter.next().ok_or("--profile requires a profile name")?;
                let profiles = load_profiles()?;
                let p = profiles.iter().find(|p| p.name == *name).ok_or_else(|| {
                    format!(
                        "no ssh profile `{name}` — add it to {} under tui_ssh_profiles",
                        noviewlog_core::core::config::user_config_path()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| "~/.config/noviewlog/config.yaml".into())
                    )
                })?;
                ssh::probe_ssh_client()?;
                choice = CliChoice::Ssh(SessionChoice::Ssh {
                    label: format!("{} ({})", p.name, p.target),
                    argv: ssh::argv_for_profile(p),
                });
            }
            "--connect" => choice = CliChoice::Connect,
            other => {
                return Err(format!(
                    "unknown argument `{other}` (use --ssh, --profile, --connect)"
                ))
            }
        }
    }
    Ok(choice)
}

fn load_profiles() -> Result<Vec<SshProfile>, String> {
    let (config, warning) = load_user_config();
    if let Some(w) = warning {
        eprintln!("noviewlog-tui: config warning: {w}");
    }
    Ok(config.map(|c| c.tui_ssh_profiles).unwrap_or_default())
}

fn run() -> Result<(), String> {
    let cli = parse_cli()?;
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut out = stdout();
    // Mouse capture is always on: the wheel scrolls our output like a
    // terminal buffer (on the alternate screen the host would otherwise
    // translate it to arrow keys = shell history). Selection is our own.
    // Bracketed paste: pastes arrive as Event::Paste (filter input) or are
    // forwarded to the shell as plain stdin instead of being replayed as a
    // stream of fake keystrokes (#199).
    execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
    .map_err(|e| e.to_string())?;
    // Any panic (and normal exit) must restore the user's terminal.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        prev_hook(info);
    }));

    let (cols, rows) = crossterm::terminal::size().map_err(|e| e.to_string())?;
    let mut app = App::new(cols, rows, load_profiles()?)?;
    app.sync_geometry();
    match cli {
        CliChoice::Local => app.start_session(SessionChoice::Local)?,
        CliChoice::Ssh(choice) => app.start_session(choice)?,
        CliChoice::Connect => app.connect_open = true,
    }
    app.ui_dirty = true;
    let _ = execute!(out, crossterm::cursor::Hide);

    let result = event_loop(&mut app);

    let _ = restore_terminal();
    result
}

fn event_loop(app: &mut App) -> Result<(), String> {
    loop {
        // Drain input (16ms poll doubles as the frame tick).
        if crossterm::event::poll(FRAME_BUDGET).map_err(|e| e.to_string())? {
            match crossterm::event::read().map_err(|e| e.to_string())? {
                Event::Key(k) => {
                    if app.handle_key(k).is_none() {
                        return Ok(());
                    }
                }
                Event::Mouse(m) => app.handle_mouse(m),
                Event::Resize(cols, rows) => {
                    app.cols = cols;
                    app.rows = rows;
                    app.sync_geometry();
                }
                Event::Paste(text) => app.handle_paste(&text),
                Event::FocusGained | Event::FocusLost => {}
            }
        }
        app.engine.tick();
        app.drain_events();
        let due = app.last_paint.is_none_or(|t| t.elapsed() >= FRAME_BUDGET)
            && (app.engine.needs_render() || app.ui_dirty);
        if due {
            app.paint();
            app.engine.note_viewport_painted();
        }
    }
}

fn restore_terminal() -> io::Result<()> {
    let mut out = stdout();
    disable_raw_mode()?;
    execute!(
        out,
        crossterm::cursor::Show,
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_x_clamps_before_cast() {
        // Clamp happens in usize: an extreme engine column saturates to the
        // right edge instead of truncating through u16 to a bogus x (#241).
        assert_eq!(caret_x(usize::MAX, 80), 79);
        assert_eq!(caret_x(5_000, 80), 79);
        assert_eq!(caret_x(usize::MAX, 40), 39);
        // In-range columns keep the +2 prefix offset.
        assert_eq!(caret_x(0, 80), 2);
        assert_eq!(caret_x(10, 80), 12);
    }

    #[test]
    fn paste_append_caps_filter_buffer() {
        let mut buf = String::new();
        let chunk = "a".repeat(FILTER_BUF_CAP);
        paste_append(&mut buf, &chunk);
        paste_append(&mut buf, &chunk);
        assert_eq!(buf.len(), FILTER_BUF_CAP);
        assert_eq!(buf.chars().count(), FILTER_BUF_CAP);
    }

    #[test]
    fn paste_append_stops_on_char_boundary() {
        // Two-byte chars: the cap must never split a UTF-8 sequence.
        let mut buf = String::new();
        let wide = "é".repeat(FILTER_BUF_CAP);
        paste_append(&mut buf, &wide);
        assert!(buf.chars().all(|c| c == 'é'));
        assert!(buf.len() <= FILTER_BUF_CAP);
        assert!(std::str::from_utf8(buf.as_bytes()).is_ok());
    }

    #[test]
    fn connect_box_width_matches_draw_clamp() {
        // 40-col terminal: full box fits, inner width 38 (draw and hit test
        // must agree so phantom clicks cannot select unseen items).
        assert_eq!(App::connect_box_width(0, 40), 38);
        // 20-col terminal: the box clamps to the terminal width.
        assert_eq!(App::connect_box_width(0, 20), 18);
    }

    #[test]
    fn menu_width_matches_draw_clamp() {
        // Wide terminal: fixed 24-col menu.
        assert_eq!(App::menu_width(0, 80), 24);
        // cols=20 with the menu at col=10: only the 10 drawn cells are
        // clickable (draw and hit test share this helper, #254).
        assert_eq!(App::menu_width(10, 20), 10);
        assert_eq!(App::menu_width(20, 20), 0);
        // No overflow at the terminal edge.
        assert_eq!(App::menu_width(u16::MAX, 80), 0);
    }

    #[test]
    fn text_in_cells_slices_ascii_like_char_indices() {
        // Pure ASCII: identical to the old chars[start..end] slice.
        assert_eq!(text_in_cells("hello world", 0, 4), "hello");
        assert_eq!(text_in_cells("hello", 1, 3), "ell");
        assert_eq!(text_in_cells("abc", 5, 9), "");
    }

    #[test]
    fn text_in_cells_maps_wide_chars_to_cells() {
        // "日志ab": 日 = cells 0..1, 志 = cells 2..3, a = 4, b = 5.
        // Cell selection 0..=3 must copy exactly the two CJK glyphs — the
        // old char-index slice returned "日日" (4 chars for 4 "indices").
        assert_eq!(text_in_cells("日志ab", 0, 3), "日志");
        assert_eq!(text_in_cells("日志ab", 2, 4), "志a");
        assert_eq!(text_in_cells("日志ab", 4, 5), "ab");
        assert_eq!(text_in_cells("日志", 1, 2), "志");
    }

    #[test]
    fn selected_text_uses_cell_coords_on_mixed_lines() {
        // End-to-end through selected_text: one CJK+ASCII line, drag over a
        // known cell range; the copied substring must be exact.
        let app = app_with_visible_lines(&["日志ab"]);
        assert_eq!(app.selected_text((0, 0), (0, 3)), "日志");
        assert_eq!(app.selected_text((0, 2), (0, 4)), "志a");
        assert_eq!(app.selected_text((0, 4), (0, 5)), "ab");
    }

    /// Minimal App for selection tests: engine built off-session, one
    /// pre-populated visible slice (no terminal, no PTY).
    fn app_with_visible_lines(lines: &[&str]) -> App {
        use noviewlog_core::core::types::{LogLevel, TextSegment};
        let mut app = App::new(80, 24, Vec::new()).expect("app");
        app.visible = lines
            .iter()
            .map(|l| FlatLine {
                record_id: 0,
                line_index: 0,
                raw: (*l).to_string(),
                hidden_line_count: 0,
                collapsible: false,
                collapsed: false,
                level: Some(LogLevel::Info),
                segments: vec![TextSegment {
                    text: (*l).to_string(),
                    style: None,
                }],
            })
            .collect();
        app
    }

    #[test]
    fn handle_paste_drops_before_filter_buf_when_exited() {
        // Gate order: an exited session swallows the paste even with the
        // filter input focused (#254).
        let mut app = App::new(80, 24, Vec::new()).expect("app");
        app.input_focus = true;
        app.exited = Some("ssh x exited (code 0)".to_string());
        app.handle_paste("leak");
        assert!(app.filter_buf.is_empty());
    }
}
