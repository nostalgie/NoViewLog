use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;

use crate::core::config::{
    build_runtime_config, load_bundled_config, load_config_from_yaml, load_preset,
    load_user_config, save_user_config,
};
use crate::core::formats::{get_builtin_format, merge_formats};
use crate::core::parser::{reparse_lines, RecordParser};
use crate::core::types::{
    clamp_max_scrollback_lines, clamp_viewport_font_size, compile_filter, next_filter_id, AppConfig,
    FilterRule, FilterType, LaunchConfig, LogFormat, PresetConfig, DEFAULT_MAX_SCROLLBACK_LINES,
};
#[cfg(test)]
use crate::core::types::TabConfig;
use crate::file_index::{PREFETCH_RAW_LINES, WINDOW_RAW_LINES};
use crate::file_load::{FileLoadState, FILE_VIEW_WINDOW_LINES};
use crate::log_view::LogView;
use crate::terminal_state::{next_terminal_id, PendingFileWindow, TerminalState, MAX_CLOSED_TABS};
use crate::pty::{PtyActivityWake, PtyEvent, PtyManager};
use crate::spawn_resolve::{resolve_interactive_shell, resolve_process_launch};
use crate::viewport::ViewportRenderer;
use crate::viewport_layout::{
    build_visual_lines, content_width, count_visual_rows, max_cols, max_scroll_x, pos_at_pixel,
    selection_plain_text, record_selection_at, word_selection_at, TextSelection, LEFT_PAD,
};
use crate::core::visible::SearchPattern;
use portable_pty::PtySize;

/// Default / legacy alias for the scrollback retention cap (records ≈ lines).
pub const MAX_RECORDS: usize = DEFAULT_MAX_SCROLLBACK_LINES;
const PENDING_IDLE_FLUSH: Duration = Duration::from_millis(120);
/// Terminal tab block caret blink half-period (~classic terminal rate).
pub const CARET_BLINK_PERIOD: Duration = Duration::from_millis(530);
/// Minimum PTY/emulator column width.
///
/// Soft-wrap is display-only and always uses the real viewport width. The PTY
/// must stay *at least* this wide (and at least the viewport) so child tools
/// that honour `COLUMNS` do not hard-break lines at the viewport edge — that
/// made Wrap ON/OFF look identical (nothing left for soft-wrap / H-scroll).
const MIN_PTY_COLS: u16 = 500;
/// Prefetch an adjacent file chunk when scroll is within this many pixels of a window edge.
pub(crate) const PREFETCH_SCROLL_PX: f32 = 120.0;
/// Raw file lines read per tick while swapping the in-memory sliding window.
pub(crate) const FILE_WINDOW_LINES_PER_TICK: usize = 2_000;
/// Bound pending PTY `Bytes` events (~4 KB each) so the reader blocks under flood.
/// 384 × 4 KB ≈ 1.5 MB of queued output before kernel backpressure stalls the writer.
pub(crate) const PTY_QUEUE_CAPACITY: usize = 384;
/// Max PTY bytes fed through VTE / scrollback on a single UI tick.
/// Kept below ~512 KB so Follow paints stay smooth while still draining floods promptly.
pub(crate) const PTY_INGEST_BYTES_PER_TICK: usize = 256 * 1024;


mod commands;
mod events;
mod stats;
mod file_session;
mod scroll_selection;
mod terminal_lifecycle;
#[cfg(test)]
mod test_api;

pub use commands::Command;
pub use events::{parse_engine_event, EngineEvent, StatsSnapshot, StatsTab, StatsTerminal};

use terminal_lifecycle::StartAction;

pub struct Engine {
    pub(crate) terminals: Vec<TerminalState>,
    pub(crate) active_terminal: usize,
    pub(crate) config: AppConfig,
    pub(crate) preset_name: String,
    pub(crate) format_id: String,
    pub(crate) formats: HashMap<String, LogFormat>,
    pub(crate) ptys: HashMap<String, PtyManager>,
    pub(crate) pty_rx: Receiver<PtyEvent>,
    pub(crate) pty_tx: SyncSender<PtyEvent>,
    /// Host callback when PTY bytes/exit are posted (coalesced wake).
    pub(crate) pty_activity_wake: Option<PtyActivityWake>,
    /// Leftover `Bytes`/`Exit` held after a budgeted `poll_pty` (cannot push back to mpsc).
    pub(crate) pty_hold: Option<PtyEvent>,
    /// Set when `poll_pty` hits the ingest budget with more work left.
    /// Host must schedule another tick via its timer — do **not** wake mid-tick
    /// (that caused a HOST_TICK busy-loop under `cat` floods).
    pub(crate) pty_drain_pending: bool,
    pub(crate) status_message: String,
    pub(crate) events: VecDeque<String>,
    pub(crate) viewport_width: u32,
    pub(crate) viewport_height: u32,
    pub(crate) renderer: ViewportRenderer,
    pub(crate) last_stats_at: Option<Instant>,
    pub(crate) viewport_dirty: bool,
    /// When true, the first tick auto-starts the active program (CLI launch only).
    pub(crate) auto_start_launch: bool,
    /// Terminal tab block-caret blink: visible when true; toggled on [`CARET_BLINK_PERIOD`].
    pub(crate) caret_blink_on: bool,
    pub(crate) caret_blink_at: Instant,
    /// Host viewport has keyboard focus — caret only blinks when true.
    pub(crate) viewport_focused: bool,
    /// FILTERS draft preview (UI-global): pattern text + compiled highlight.
    pub(crate) filter_draft_query: String,
    pub(crate) filter_draft_regex: bool,
    pub(crate) filter_draft_pattern: Option<SearchPattern>,
}

impl Engine {
    pub fn new() -> Self {
        let (pty_tx, pty_rx) = std::sync::mpsc::sync_channel(PTY_QUEUE_CAPACITY);
        let mut config = load_bundled_config();
        if let Some(user) = load_user_config() {
            config = user;
        }
        config.max_scrollback_lines = clamp_max_scrollback_lines(config.max_scrollback_lines);
        config.viewport_font_size = clamp_viewport_font_size(config.viewport_font_size);
        let max_scrollback = config.max_scrollback_lines;
        let viewport_font_size = config.viewport_font_size;
        let preset_name = config.default_preset.clone();
        let runtime = build_runtime_config(&config, Some(&preset_name));
        let formats = merge_formats(
            &crate::core::config::all_format_presets(&config),
            &HashMap::new(),
        );
        let format_id = runtime.format_id.clone();
        let default_format = formats
            .get(&format_id)
            .cloned()
            .unwrap_or_else(|| get_builtin_format("node-default"));

        let id = next_terminal_id(&[]);
        let terminal = TerminalState::new(
            id,
            LaunchConfig::default(),
            &runtime,
            &default_format,
            max_scrollback,
        );

        // No PTY yet — [`Self::set_launch`] starts the CLI command, log file, or
        // interactive shell so a leftover boot-shell Exit cannot steal the session.
        Self {
            terminals: vec![terminal],
            active_terminal: 0,
            config,
            preset_name,
            format_id,
            formats,
            ptys: HashMap::new(),
            pty_rx,
            pty_tx,
            pty_activity_wake: None,
            pty_hold: None,
            pty_drain_pending: false,
            status_message: String::new(),
            events: VecDeque::new(),
            viewport_width: 800,
            viewport_height: 600,
            renderer: ViewportRenderer::with_font_size(viewport_font_size),
            last_stats_at: None,
            viewport_dirty: true,
            auto_start_launch: false,
            caret_blink_on: true,
            caret_blink_at: Instant::now(),
            viewport_focused: false,
            filter_draft_query: String::new(),
            filter_draft_regex: false,
            filter_draft_pattern: None,
        }
    }

    pub(crate) fn has_active_terminal(&self) -> bool {
        !self.terminals.is_empty() && self.active_terminal < self.terminals.len()
    }

    pub(crate) fn ensure_valid_state(&mut self) {
        if self.terminals.is_empty() {
            let runtime = build_runtime_config(&self.config, Some(&self.preset_name));
            let format = self.current_format();
            let id = next_terminal_id(&[]);
            self.terminals.push(TerminalState::new(
                id,
                LaunchConfig::default(),
                &runtime,
                &format,
                self.config.max_scrollback_lines,
            ));
            self.active_terminal = 0;
        }
        self.active_terminal = self.active_terminal.min(self.terminals.len() - 1);
        let runtime = build_runtime_config(&self.config, Some(&self.preset_name));
        self.terminals[self.active_terminal].ensure_terminal_tab_view(&runtime);
    }

    pub(crate) fn active_terminal(&self) -> &TerminalState {
        let idx = self.active_terminal.min(self.terminals.len().saturating_sub(1));
        self.terminals
            .get(idx)
            .expect("active_terminal called with no terminals")
    }

    pub(crate) fn active_terminal_mut(&mut self) -> &mut TerminalState {
        let idx = self.active_terminal.min(self.terminals.len().saturating_sub(1));
        self.terminals
            .get_mut(idx)
            .expect("active_terminal_mut called with no terminals")
    }

    pub(crate) fn command_needs_active_terminal(cmd: &Command) -> bool {
        !matches!(
            cmd,
            Command::Resize { .. }
                | Command::TerminalAdd
                | Command::TerminalClose { .. }
                | Command::TerminalSwitch { .. }
                | Command::TerminalMove { .. }
                | Command::TerminalRename { .. }
                | Command::TerminalStart { .. }
                | Command::LoadFile { .. }
                | Command::SetSettings { .. }
                | Command::SetViewportFontSize { .. }
                | Command::SetViewportFocus { .. }
                | Command::FilterDraftSet { .. }
        )
    }

    pub fn tick(&mut self) {
        self.ensure_valid_state();
        if !self.has_active_terminal() {
            if self.status_message.is_empty() {
                self.status_message = "No terminal".to_string();
            }
            self.poll_pty();
            self.emit_stats();
            return;
        }
        if self.auto_start_launch {
            let start_action = {
                let terminal = self.active_terminal();
                if terminal.process_started {
                    None
                } else if let Some(path) = terminal.launch.log_file.clone() {
                    Some(StartAction::File(path))
                } else if terminal.launch.command.is_some() {
                    Some(StartAction::Launch)
                } else {
                    None
                }
            };
            if let Some(action) = start_action {
                self.active_terminal_mut().process_started = true;
                match action {
                    StartAction::File(path) => self.start_log_file_load(&path),
                    StartAction::Launch => self.start_launch_process(),
                }
            }
        }
        self.poll_pty();
        self.advance_file_load();
        self.advance_pending_file_window();
        self.advance_file_match_scan();
        self.maybe_prefetch_file_window();
        if self.rebuild_if_needed() {
            self.mark_viewport_dirty();
        }
        self.tick_caret_blink();
        self.emit_stats();
    }

    pub fn terminal_caret_active(&self) -> bool {
        self.viewport_focused
            && self.has_active_terminal()
            && self.active_terminal().running
            && self.active_terminal().active_view == 0
            && self.active_terminal().ingest.viewport_caret().is_some()
    }

    /// Device-pixel block caret rect `(x, y, w, h)` for the Slint overlay, or `None`
    /// when the Terminal tab cannot accept input or the caret is off-screen.
    pub fn terminal_caret_rect(&self, width: u32, height: u32) -> Option<(f32, f32, f32, f32)> {
        if !self.terminal_caret_active() || width == 0 || height == 0 {
            return None;
        }
        let terminal = self.active_terminal();
        let view = terminal.active_view();
        let flat_lines = view.flat_lines.as_ref();
        let wrap_lines = view.wrap_lines;
        let scroll_x = if wrap_lines {
            0.0
        } else {
            terminal.scroll_x
        };
        let mut scroll_y = terminal.scroll_offset_y;
        let metrics = self.renderer.metrics();
        let rows = view.cached_visual_rows(width, metrics.cell_width, count_visual_rows);
        if view.auto_follow && view.search_query.is_empty() && !terminal.is_file_session() {
            let content_h = rows as f32 * metrics.row_stride;
            scroll_y = (content_h - height as f32).max(0.0);
        }
        let screen = terminal.ingest.viewport_caret()?;
        let base = flat_lines
            .len()
            .saturating_sub(terminal.ingest.volatile_count());
        let caret = crate::viewport::ViewportCaret {
            flat_index: base.saturating_add(screen.line),
            col: screen.col,
        };
        let first_row = (scroll_y / metrics.row_stride).floor() as usize;
        let y_offset = scroll_y - first_row as f32 * metrics.row_stride;
        let x_base = if wrap_lines {
            LEFT_PAD as i32
        } else {
            LEFT_PAD as i32 - scroll_x as i32
        };
        let max_rows = (height as f32 / metrics.row_stride).ceil() as usize + 1;
        let index = view.ensure_visual_row_index(width, metrics.cell_width);
        let visual = crate::viewport_layout::collect_visible_visual_lines_with_total(
            flat_lines,
            wrap_lines,
            width,
            metrics.cell_width,
            first_row,
            max_rows,
            Some(index.total_rows()),
            Some(index.as_ref()),
        );
        let (cx, cy) = crate::viewport::caret_pixel_pos(
            flat_lines,
            &visual,
            caret,
            0,
            y_offset,
            x_base,
            metrics.row_stride,
            metrics.cell_width,
            height,
        )?;
        let w = metrics.cell_width.max(1) as f32;
        let h = metrics.row_height.max(1.0);
        Some((cx as f32, cy, w, h))
    }

    /// Register a host wake when PTY bytes or exit are posted (from the reader thread).
    pub fn set_pty_activity_wake(&mut self, wake: PtyActivityWake) {
        self.pty_activity_wake = Some(wake);
    }

    /// True when budgeted PTY ingest left work for a later tick (`pty_hold` or drain flag).
    pub fn pty_work_pending(&self) -> bool {
        self.pty_drain_pending || self.pty_hold.is_some()
    }

    /// Clear and return whether a drain was requested after the last `poll_pty`.
    pub fn take_pty_drain_pending(&mut self) -> bool {
        let pending = self.pty_drain_pending || self.pty_hold.is_some();
        self.pty_drain_pending = false;
        pending
    }

    pub(crate) fn set_viewport_focus(&mut self, focused: bool) {
        if self.viewport_focused == focused {
            return;
        }
        self.viewport_focused = focused;
        // Overlay caret is host-drawn; content paint is unchanged by focus alone.
        // Unfocus: no need to dirty the bitmap (caret overlay hides independently).
        let _ = focused;
    }

    /// Host owns blink phase; engine no longer dirties the viewport for caret blink.
    pub(crate) fn tick_caret_blink(&mut self) {
        // retained for tick() call site stability — blink is Slint-side now
    }

    /// Reset blink phase (host shows overlay immediately while typing / on focus).
    pub fn reset_caret_blink(&mut self) {
        self.caret_blink_on = true;
        self.caret_blink_at = Instant::now();
    }

    pub fn needs_render(&self) -> bool {
        // Paint only when something actually dirtied the viewport.
        // Live follow must not force perpetual redraw: PTY ingest, rebuild,
        // caret blink, scroll, resize, and tab/terminal switches call
        // mark_viewport_dirty() when content or chrome changes.
        self.viewport_dirty
    }

    pub(crate) fn mark_viewport_dirty(&mut self) {
        self.viewport_dirty = true;
    }

    /// Character grid size for the PTY / VT emulator.
    ///
    /// Rows track the viewport. Cols are `max(viewport_cols, MIN_PTY_COLS)` so a
    /// wide window is not capped at the old fixed 120, while a narrow window
    /// still gets a wide logical line buffer for soft-wrap / horizontal scroll.
    pub(crate) fn viewport_pty_size(&self) -> PtySize {
        let metrics = self.renderer.metrics();
        let viewport_cols = max_cols(content_width(self.viewport_width), metrics.cell_width)
            .clamp(1, u16::MAX as usize) as u16;
        let cols = viewport_cols.max(MIN_PTY_COLS);
        let rows = ((self.viewport_height as f32) / metrics.row_stride.max(1.0))
            .floor()
            .clamp(1.0, u16::MAX as f32) as u16;
        let cell_w = metrics.cell_width.max(1);
        let cell_h = metrics.row_stride.max(1.0).ceil() as u32;
        PtySize {
            cols,
            rows: rows.max(1),
            pixel_width: (cell_w as u32)
                .saturating_mul(cols as u32)
                .min(u16::MAX as u32) as u16,
            pixel_height: cell_h
                .saturating_mul(rows as u32)
                .min(u16::MAX as u32) as u16,
        }
    }

    /// Keep PTY winsize + terminal emulator cols/rows in sync with the viewport.
    /// Soft-wrap remains display-only (viewport pixels); PTY cols use a wide
    /// floor so child hard-wrap does not steal the Wrap toggle's job.
    pub(crate) fn sync_terminal_geometry(&mut self) {
        let size = self.viewport_pty_size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        for pty in self.ptys.values_mut() {
            let _ = pty.set_size(size);
        }
        if self.terminals.is_empty() {
            return;
        }
        let mut any = false;
        for term in &mut self.terminals {
            if term.ingest.size() != (cols, rows) {
                term.ingest
                    .resize(cols, rows, &mut term.buffer, &mut term.parser);
                any = true;
            }
        }
        if any {
            self.mark_all_views_dirty();
            self.mark_viewport_dirty();
        }
    }

    pub fn render(&mut self, width: u32, height: u32, out: &mut [u8]) -> Result<(), String> {
        let size_changed = width != self.viewport_width || height != self.viewport_height;
        self.viewport_width = width;
        self.viewport_height = height;
        if size_changed {
            self.sync_terminal_geometry();
        }
        self.ensure_valid_state();
        if !self.has_active_terminal() {
            self.renderer.render_center_message(
                out,
                width,
                height,
                "No terminal",
            )?;
            self.viewport_dirty = false;
            return Ok(());
        }

        let scroll_row = {
            let terminal = self.active_terminal_mut();
            terminal.scroll_to_row.take()
        };
        if let Some(row) = scroll_row {
            self.scroll_to_row_index(row);
        }

        // FILES: never paint with local scroll past the loaded window (black frames).
        if self.active_terminal().is_file_session() && self.active_terminal().file_backed.is_some()
        {
            let local_max = self.local_window_max_scroll();
            let terminal = self.active_terminal_mut();
            if terminal.scroll_offset_y > local_max {
                terminal.scroll_offset_y = local_max;
            }
        }

        let (auto_follow, wrap_lines, flat_lines, search_pattern, active_match, running, scroll_offset_y, scroll_x, selection) = {
            let terminal = self.active_terminal();
            let view = terminal.active_view();
            (
                // Find chrome owns search: a live query pins the viewport on matches
                // (no Follow). Closing Find must SearchSet empty or this stays frozen.
                // File sessions never Follow.
                !terminal.is_file_session()
                    && view.auto_follow
                    && view.search_query.is_empty(),
                view.wrap_lines,
                Arc::clone(&view.flat_lines),
                view.search_pattern.clone(),
                view.search_matches.get(view.search_match_index).copied(),
                terminal.running,
                terminal.scroll_offset_y,
                terminal.scroll_x,
                terminal.selection,
            )
        };
        let filter_draft_pattern = self.filter_draft_pattern.clone();

        let mut scroll_offset_y = scroll_offset_y;
        if auto_follow {
            let metrics = self.renderer.metrics();
            let rows = self.active_view().cached_visual_rows(
                width,
                metrics.cell_width,
                count_visual_rows,
            );
            let content_h = rows as f32 * metrics.row_stride;
            let new_scroll = (content_h - height as f32).max(0.0);
            if (new_scroll - scroll_offset_y).abs() > 0.01 {
                self.mark_viewport_dirty();
            }
            scroll_offset_y = new_scroll;
            self.active_terminal_mut().scroll_offset_y = scroll_offset_y;
        }

        if !running && flat_lines.is_empty() {
            let on_terminal_tab = self.active_terminal().active_view == 0;
            let msg = if on_terminal_tab {
                "Type to open a shell — or ▶ Start for the saved command"
            } else {
                "Press ▶ Start to run"
            };
            self.renderer.render_center_message(out, width, height, msg)?;
            self.viewport_dirty = false;
            return Ok(());
        }
        let effective_scroll_x = if wrap_lines { 0.0 } else { scroll_x };

        let metrics = self.renderer.metrics();
        let index = self
            .active_view()
            .ensure_visual_row_index(width, metrics.cell_width);
        let total_rows = index.total_rows();

        // Caret is drawn by the Slint host overlay — keep the bitmap content-only.
        self.renderer.render_with_total(
            out,
            width,
            height,
            &flat_lines,
            scroll_offset_y,
            effective_scroll_x,
            wrap_lines,
            selection.as_ref(),
            search_pattern.as_ref(),
            filter_draft_pattern.as_ref(),
            active_match,
            None,
            Some(total_rows),
            Some(index.as_ref()),
        )?;
        self.viewport_dirty = false;
        Ok(())
    }

    /// Apply a typed host → engine command (same guards as JSON path).
    pub fn send_command(&mut self, cmd: Command) -> Result<(), String> {
        self.ensure_valid_state();
        if Self::command_needs_active_terminal(&cmd) && !self.has_active_terminal() {
            return Ok(());
        }
        self.apply_command(cmd)
    }

    /// Thin JSON → [`Command`] → [`Self::send_command`] (FFI / legacy hosts).
    pub fn send_command_json(&mut self, json: &str) -> Result<(), String> {
        let cmd: Command = serde_json::from_str(json).map_err(|e| e.to_string())?;
        self.send_command(cmd)
    }

    pub fn poll_event_json(&mut self) -> Option<String> {
        self.events.pop_front()
    }

    pub fn peek_event_json(&self) -> Option<&str> {
        self.events.front().map(String::as_str)
    }

    pub fn handle_key(&mut self, bytes: &[u8]) {
        if !self.has_active_terminal() {
            return;
        }
        // Terminal tab: auto-start an interactive shell so typing works without ▶ Start.
        if !self.active_terminal().running {
            if self.active_terminal().active_view != 0 {
                return;
            }
            if self.active_terminal().is_file_session() {
                return;
            }
            self.start_interactive_shell();
            if !self.active_terminal().running {
                return;
            }
        }
        // Typing in the Terminal tab while scrolled up: Follow on and jump to
        // the live prompt (same as a conventional terminal emulator).
        if self.active_terminal().active_view == 0
            && !bytes.is_empty()
            && !self.active_view().auto_follow
        {
            self.scroll_to_end();
        }
        let id = self.active_terminal().id.clone();
        let write_result = self
            .ptys
            .get_mut(&id)
            .map(|pty| pty.write_bytes(bytes))
            .unwrap_or_else(|| Err("no pty for terminal".to_string()));
        if let Err(err) = write_result {
            self.push_event(json!({"type":"status","message": format!("stdin: {err}")}));
        } else {
            // Keep the caret visible while typing (same as a real terminal).
            self.reset_caret_blink();
        }
    }

    pub fn set_launch(&mut self, launch: LaunchConfig) {
        if let Some(path) = &launch.config_path {
            if let Ok(text) = std::fs::read_to_string(path) {
                self.config = load_config_from_yaml(&text);
                self.formats = merge_formats(
                    &crate::core::config::all_format_presets(&self.config),
                    &HashMap::new(),
                );
            }
        }
        if let Some(preset) = launch.preset.clone() {
            self.preset_apply(&preset);
        }

        self.auto_start_launch = launch.has_process_launch();
        self.ensure_valid_state();
        let default_format = self.current_format();
        let term_size = self.viewport_pty_size();
        let id = self.active_terminal().id.clone();

        // Update active terminal's launch and reset its session state.
        {
            let terminal = self.active_terminal_mut();
            if let Some(cwd) = launch.cwd.clone().filter(|s| !s.is_empty()) {
                terminal.cwd = cwd;
            }
            terminal.launch = launch;
            terminal.process_started = false;
            terminal.running = false;
            terminal.exit_code = None;
            terminal.buffer.clear();
            terminal.ingest
                .reset_with_size(term_size.cols as usize, term_size.rows as usize);
            terminal.reset_viewport();
            for view in &mut terminal.views {
                view.clear_flat_lines();
            }
            if terminal.views.is_empty() {
                let name = terminal.primary_tab_name();
                terminal.views = vec![LogView::from_runtime(&name, Vec::new())];
                terminal.active_view = 0;
            }
            terminal.sync_primary_tab_identity();
            if terminal.is_file_session() {
                terminal.disable_follow_all_views();
            }
            terminal.parser = RecordParser::new(default_format);
        }

        if let Some(mut pty) = self.ptys.remove(&id) {
            pty.stop();
        }

        let log_file = self.active_terminal().launch.log_file.clone();
        let has_command = self.active_terminal().launch.command.is_some();
        if let Some(path) = log_file {
            self.configure_active_as_file_session(&path);
            self.active_terminal_mut().process_started = true;
            self.start_log_file_load(&path);
        } else if has_command {
            self.active_terminal_mut().process_started = true;
            self.start_launch_process();
        } else {
            #[cfg(not(test))]
            {
                self.start_interactive_shell();
            }
        }
    }

    pub(crate) fn active_view(&self) -> &LogView {
        self.active_terminal().active_view()
    }

    pub(crate) fn active_view_mut(&mut self) -> &mut LogView {
        self.active_terminal_mut().active_view_mut()
    }

    pub(crate) fn current_format(&self) -> LogFormat {
        self.formats
            .get(&self.format_id)
            .cloned()
            .unwrap_or_else(|| get_builtin_format("node-default"))
    }

    pub(crate) fn push_event(&mut self, value: serde_json::Value) {
        if let Ok(text) = serde_json::to_string(&value) {
            self.events.push_back(text);
        }
    }

    pub(crate) fn rebuild_if_needed(&mut self) -> bool {
        if !self.has_active_terminal() {
            return false;
        }

        // File filter tabs with a match index rebuild via apply_match_window.
        if self.active_terminal().is_file_session() && self.active_view().uses_match_index() {
            if self.active_view().match_scan_pos.is_some() {
                return false;
            }
            if self.active_view().is_flat_lines_dirty() {
                self.apply_match_window();
            }
            let terminal = self.active_terminal_mut();
            let view = terminal.active_view_mut();
            let search_was_dirty = view.is_search_dirty();
            let scroll_row = view.refresh_search_if_dirty();
            if let Some(row) = scroll_row {
                // Search hit is within the match window's flat_lines.
                terminal.scroll_to_row = Some(row);
            }
            return search_was_dirty || scroll_row.is_some();
        }

        let terminal = self.active_terminal_mut();
        let active = terminal.active_view;
        let Some(view) = terminal.views.get_mut(active) else {
            return false;
        };
        let before_records = view.flat_lines_record_cursor;
        let before_lines = view.flat_lines.len();
        let search_was_dirty = view.is_search_dirty();
        // Partial borrow: view + buffer are distinct fields.
        let scroll_row = {
            let TerminalState { views, buffer, .. } = terminal;
            let view = views.get_mut(active).expect("active view");
            view.rebuild(buffer)
        };
        let view = terminal.views.get_mut(active).expect("active view");
        let changed = search_was_dirty
            || view.flat_lines_record_cursor != before_records
            || view.flat_lines.len() != before_lines;
        if let Some(row) = scroll_row {
            terminal.scroll_to_row = Some(row);
        }
        changed
    }

    pub(crate) fn mark_all_views_dirty(&mut self) {
        let terminal = self.active_terminal_mut();
        for view in &mut terminal.views {
            view.mark_flat_lines_dirty();
        }
    }

    pub(crate) fn add_tab(&mut self) {
        let tab_count = self.active_terminal().views.len();
        let name = format!("Tab {}", tab_count + 1);
        let is_file = self.active_terminal().is_file_session();
        let terminal = self.active_terminal_mut();
        // Filter tabs start empty (full stream); user adds include/exclude rules.
        let mut view = LogView::from_runtime(&name, Vec::new());
        if is_file {
            view.auto_follow = false;
        }
        terminal.views.push(view);
        terminal.active_view = terminal.views.len() - 1;
        terminal.scroll_offset_y = 0.0;
        terminal.scroll_x = 0.0;
        terminal.selection = None;
        self.mark_viewport_dirty();
        // Flush tab strip chrome on the next tick (do not wait for the 250ms stats throttle).
        self.last_stats_at = None;
    }

    pub(crate) fn close_tab(&mut self, index: usize) {
        let terminal = self.active_terminal_mut();
        // Tab 0 is the Terminal tab — never close it.
        if index == 0 || terminal.views.len() <= 1 || index >= terminal.views.len() {
            return;
        }
        let tab = terminal.views[index].to_tab_config();
        terminal.views.remove(index);
        terminal.closed_tabs.push_back(tab);
        while terminal.closed_tabs.len() > MAX_CLOSED_TABS {
            terminal.closed_tabs.pop_front();
        }
        if terminal.active_view >= terminal.views.len() {
            terminal.active_view = terminal.views.len() - 1;
        } else if index < terminal.active_view {
            terminal.active_view -= 1;
        }
        terminal.scroll_offset_y = 0.0;
        terminal.scroll_x = 0.0;
        terminal.selection = None;
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn restore_tab(&mut self) {
        let terminal = self.active_terminal_mut();
        let Some(tab) = terminal.closed_tabs.pop_back() else {
            return;
        };
        terminal.views.push(LogView::from_tab_config(tab));
        terminal.active_view = terminal.views.len() - 1;
        terminal.scroll_offset_y = 0.0;
        terminal.scroll_x = 0.0;
        terminal.selection = None;
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn switch_tab(&mut self, index: usize) {
        let terminal = self.active_terminal_mut();
        if index < terminal.views.len() && index != terminal.active_view {
            terminal.active_view = index;
            terminal.scroll_offset_y = 0.0;
            terminal.scroll_x = 0.0;
            terminal.selection = None;
            self.mark_viewport_dirty();
            self.last_stats_at = None;
        }
    }

    pub(crate) fn rename_tab(&mut self, index: usize, name: &str) {
        let name = name.trim();
        let terminal = self.active_terminal_mut();
        // Tab 0 is the Terminal tab — never rename it (UI also blocks; this is defense in depth).
        if name.is_empty() || index >= terminal.views.len() || index == 0 {
            return;
        }
        terminal.views[index].name = name.to_string();
    }

    /// Reorder filter tabs. The Terminal tab stays at index 0 (`from`/`to` of 0 are no-ops).
    pub(crate) fn tab_move(&mut self, from_index: usize, to_index: usize) {
        let terminal = self.active_terminal_mut();
        let len = terminal.views.len();
        if len < 2 || from_index == 0 || to_index == 0 {
            return;
        }
        if from_index >= len || to_index >= len || from_index == to_index {
            return;
        }
        let item = terminal.views.remove(from_index);
        terminal.views.insert(to_index, item);
        let active = terminal.active_view;
        terminal.active_view = if active == from_index {
            to_index
        } else if from_index < active && to_index >= active {
            active - 1
        } else if from_index > active && to_index <= active {
            active + 1
        } else {
            active
        };
        // Tab strip chrome; viewport content may change if active moved.
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn search_set(
        &mut self,
        query: &str,
        regex: bool,
        case_sensitive: bool,
        whole_word: bool,
    ) {
        let view = self.active_view_mut();
        // Identical query: do not mark search dirty. Hybrid UI flushes SearchSet
        // before every next/prev; re-marking would set search_jump_to_last and
        // reset the match index to the last hit on the next rebuild.
        if view.search_query == query
            && view.search_regex == regex
            && view.search_case_sensitive == case_sensitive
            && view.search_whole_word == whole_word
        {
            return;
        }
        view.search_query = query.to_string();
        view.search_regex = regex;
        view.search_case_sensitive = case_sensitive;
        view.search_whole_word = whole_word;
        view.mark_search_changed();
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn filter_draft_set(&mut self, pattern: &str, use_regex: bool) {
        if self.filter_draft_query == pattern && self.filter_draft_regex == use_regex {
            return;
        }
        self.filter_draft_query = pattern.to_string();
        self.filter_draft_regex = use_regex;
        self.filter_draft_pattern =
            crate::core::visible::compile_filter_draft_pattern(pattern, use_regex);
        self.mark_viewport_dirty();
    }

    pub(crate) fn search_goto(&mut self, delta: i32) {
        let scroll_row = {
            let terminal = self.active_terminal_mut();
            let view = &mut terminal.views[terminal.active_view];
            let n = view.search_matches.len();
            if n == 0 {
                return;
            }
            if delta < 0 {
                view.search_match_index = (view.search_match_index + n - 1) % n;
            } else {
                view.search_match_index = (view.search_match_index + 1) % n;
            }
            view.search_matches
                .get(view.search_match_index)
                .map(|m| m.line_index)
        };
        if let Some(row) = scroll_row {
            self.active_terminal_mut().scroll_to_row = Some(row);
            // Must dirty: UI only re-renders when needs_render(), and
            // scroll_to_row is applied inside render().
            self.mark_viewport_dirty();
            self.last_stats_at = None;
        }
    }

    pub(crate) fn push_lines(
        &mut self,
        lines: impl IntoIterator<Item = String>,
        mark_dirty: bool,
    ) {
        let tracking_window = self.has_active_terminal()
            && (self.active_terminal().file_load.is_some()
                || self.active_terminal().file_backed.is_some());
        let mut shifted = false;
        {
            let terminal = self.active_terminal_mut();
            if terminal.buffer.last_is_overwrite_single_line() {
                terminal.buffer.set_last_overwrite(false);
            }
            for line in lines {
                let records = terminal.parser.push_line(line);
                for record in records {
                    let shifted_lines = terminal.buffer.add(record);
                    if tracking_window && shifted_lines > 0 {
                        terminal.buffer_line_start += shifted_lines as u64;
                        shifted = true;
                    }
                }
            }
            terminal.last_line_at = Some(Instant::now());
        }
        if mark_dirty || shifted {
            self.mark_all_views_dirty();
        }
        // Only paint when callers ask (`mark_dirty`) or the window shifted.
        // File load uses mark_dirty=false and paints explicitly at first/last batch.
        if mark_dirty || shifted {
            self.mark_viewport_dirty();
        }
    }

    pub(crate) fn flush_idle_pending(&mut self) {
        if !self.has_active_terminal() {
            return;
        }
        let should_flush = self
            .active_terminal()
            .last_line_at
            .is_some_and(|at| at.elapsed() >= PENDING_IDLE_FLUSH);
        if !should_flush {
            return;
        }
        let flushed = {
            let terminal = self.active_terminal_mut();
            terminal.ingest.idle_flush(&mut terminal.buffer, &mut terminal.parser)
        };
        if flushed {
            self.mark_all_views_dirty();
        }
        self.active_terminal_mut().last_line_at = None;
    }

    pub(crate) fn add_filter(&mut self, filter_type: FilterType, pattern: &str, use_regex: bool) {
        if pattern.is_empty() || self.active_terminal().active_view == 0 {
            return;
        }
        let kind = match filter_type {
            FilterType::Include => "include",
            FilterType::Exclude => "exclude",
        };
        let next_id = next_filter_id(self.active_view().filters(), kind);
        self.active_view_mut().filters_mut().push(compile_filter(FilterRule {
            id: next_id,
            name: None,
            filter_type,
            pattern: pattern.to_string(),
            enabled: true,
            use_regex,
            regex: None,
        }));
    }

    pub(crate) fn filter_toggle(&mut self, id: &str, enabled: bool) {
        if self.active_terminal().active_view == 0 {
            return;
        }
        if let Some(filter) = self
            .active_view_mut()
            .filters_mut()
            .iter_mut()
            .find(|f| f.id == id)
        {
            filter.enabled = enabled;
        }
    }

    pub(crate) fn filter_remove(&mut self, id: &str) {
        if self.active_terminal().active_view == 0 {
            return;
        }
        self.active_view_mut().filters_mut().retain(|f| f.id != id);
    }

    pub(crate) fn filter_update(&mut self, id: &str, pattern: &str) {
        if pattern.is_empty() || self.active_terminal().active_view == 0 {
            return;
        }
        let Some(existing) = self.active_view().filters().iter().find(|f| f.id == id) else {
            return;
        };
        if existing.pattern == pattern {
            return;
        }
        let mut rule = existing.clone();
        rule.pattern = pattern.to_string();
        rule.regex = None;
        if let Some(slot) = self
            .active_view_mut()
            .filters_mut()
            .iter_mut()
            .find(|f| f.id == id)
        {
            *slot = compile_filter(rule);
        }
    }

    pub(crate) fn restart(&mut self) {
        let (has_command, log_file) = {
            let launch = &self.active_terminal().launch;
            (launch.command.is_some(), launch.log_file.clone())
        };
        let format = self.current_format();
        let term = self.viewport_pty_size();
        {
            let terminal = self.active_terminal_mut();
            terminal.buffer.clear();
            terminal.file_backed = None;
            terminal.pending_file_window = None;
            terminal.buffer_line_start = 0;
            terminal.buffer_line_end = 0;
            terminal.parser = RecordParser::new(format);
            terminal.ingest
                .reset_with_size(term.cols as usize, term.rows as usize);
            for view in &mut terminal.views {
                view.clear_flat_lines();
            }
            terminal.scroll_offset_y = 0.0;
        }
        let id = self.active_terminal().id.clone();
        if let Some(mut pty) = self.ptys.remove(&id) {
            pty.stop();
        }
        if has_command {
            self.start_launch_process();
        } else if let Some(path) = log_file {
            self.active_terminal_mut().running = false;
            self.start_log_file_load(&path);
        } else {
            // Interactive shell: clear log and spawn a fresh shell.
            self.start_interactive_shell();
        }
    }

    pub(crate) fn set_format(&mut self, id: &str) {
        if !self.formats.contains_key(id) || !self.has_active_terminal() {
            return;
        }
        if self.format_id == id {
            return;
        }
        self.format_id = id.to_string();
        let format = self.current_format();
        {
            let terminal = self.active_terminal_mut();
            if let Some(rec) = terminal.parser.flush_pending() {
                terminal.buffer.add(rec);
            }
            let lines = terminal.buffer.raw_lines().to_vec();
            let records = reparse_lines(&lines, format.clone());
            terminal.buffer.replace_all(records);
            terminal.parser = RecordParser::new(format);
            terminal.selection = None;
        }
        self.mark_all_views_dirty();
        self.rebuild_if_needed();
        self.mark_viewport_dirty();
        self.last_stats_at = None;
        self.status_message = format!("Format: {id}");
        self.push_event(json!({"type":"status","message": self.status_message}));
    }

    pub(crate) fn preset_apply(&mut self, name: &str) {
        if !self.config.presets.contains_key(name) {
            self.status_message = format!("Preset not found: {name}");
            self.push_event(json!({"type":"status","message": self.status_message}));
            return;
        }
        let filters = load_preset(&self.config, name);
        self.preset_name = name.to_string();
        let on_terminal_tab = self.active_terminal().active_view == 0;
        if on_terminal_tab {
            // The Terminal tab keeps an unfiltered stream; presets do nothing there.
            self.active_view_mut().clear_filters();
        } else {
            self.active_view_mut().set_filters(filters);
        }
        self.rebuild_if_needed();
        self.status_message = format!("Applied preset: {name}");
        self.push_event(json!({"type":"status","message": self.status_message}));
    }

    pub(crate) fn preset_get(&mut self, name: &str) {
        match self.config.presets.get(name) {
            Some(preset) => {
                let filters: Vec<serde_json::Value> = preset
                    .filters
                    .iter()
                    .map(|f| {
                        json!({
                            "id": f.id,
                            "type": f.filter_type,
                            "pattern": f.pattern,
                            "enabled": f.enabled,
                            "use_regex": f.use_regex,
                        })
                    })
                    .collect();
                self.push_event(json!({
                    "type": "preset",
                    "name": name,
                    "filters": filters,
                }));
            }
            None => {
                self.push_event(json!({
                    "type": "preset",
                    "name": name,
                    "filters": [],
                    "error": "not found",
                }));
            }
        }
    }

    pub(crate) fn set_settings(&mut self, max_scrollback_lines: usize) {
        let capped = clamp_max_scrollback_lines(max_scrollback_lines);
        self.config.max_scrollback_lines = capped;
        for terminal in &mut self.terminals {
            let shifted = terminal.buffer.set_max_records(capped);
            if shifted > 0 {
                terminal.buffer_line_start += shifted as u64;
            }
            for view in &mut terminal.views {
                view.mark_flat_lines_dirty();
            }
        }
        match save_user_config(&self.config) {
            Ok(()) => {
                self.status_message = format!("Settings saved (max scrollback: {capped})");
            }
            Err(err) => {
                self.status_message = format!("Settings save failed: {err}");
            }
        }
        self.push_event(json!({"type":"status","message": self.status_message}));
        self.mark_viewport_dirty();
    }

    pub(crate) fn set_sidebar_expanded(&mut self, terminals: bool, files: bool) {
        self.config.terminals_section_expanded = terminals;
        self.config.files_section_expanded = files;
        if let Err(err) = save_user_config(&self.config) {
            self.status_message = format!("Sidebar state save failed: {err}");
            self.push_event(json!({"type":"status","message": self.status_message}));
        }
        self.last_stats_at = None;
    }

    pub(crate) fn set_viewport_font_size(&mut self, size: f32) {
        let capped = clamp_viewport_font_size(size);
        self.config.viewport_font_size = capped;
        self.renderer.set_font_size(capped);
        self.sync_terminal_geometry();
        // Wrap/scroll layout depends on cell metrics.
        if self.has_active_terminal() {
            for view in &mut self.active_terminal_mut().views {
                view.mark_flat_lines_dirty();
            }
        }
        match save_user_config(&self.config) {
            Ok(()) => {
                self.status_message = format!("Viewport font size: {capped:.0} pt");
            }
            Err(err) => {
                self.status_message = format!("Viewport font size save failed: {err}");
            }
        }
        self.push_event(json!({"type":"status","message": self.status_message}));
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn preset_save(&mut self, name: &str, filters: Vec<FilterRule>) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let filters: Vec<FilterRule> = filters.into_iter().map(compile_filter).collect();
        self.config.presets.insert(
            name.to_string(),
            PresetConfig { filters },
        );
        match save_user_config(&self.config) {
            Ok(()) => self.status_message = format!("Preset saved: {name}"),
            Err(err) => self.status_message = format!("Save failed: {err}"),
        }
        self.push_event(json!({"type":"status","message": self.status_message}));
    }

    pub(crate) fn preset_delete(&mut self, name: &str) {
        if !self.config.presets.contains_key(name) {
            return;
        }
        self.config.presets.remove(name);
        if self.preset_name == name {
            self.preset_name = self.config.default_preset.clone();
        }
        match save_user_config(&self.config) {
            Ok(()) => self.status_message = format!("Preset deleted: {name}"),
            Err(err) => self.status_message = format!("Save failed: {err}"),
        }
        self.push_event(json!({"type":"status","message": self.status_message}));
    }

    pub(crate) fn preset_create_from_tab(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let filters = self.active_view().filters().to_vec();
        self.preset_save(name, filters);
        self.preset_name = name.to_string();
    }
}
