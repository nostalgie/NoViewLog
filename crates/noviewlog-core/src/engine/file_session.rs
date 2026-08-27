use super::*;

impl Engine {
    pub(crate) fn open_log_file_command(&mut self, path: &str) {
        self.ensure_valid_state();
        let path = path.to_string();

        // Re-open: switch to an existing terminal for the same file and reload.
        if let Some(idx) = self.terminals.iter().position(|t| {
            t.launch.log_file.as_deref() == Some(path.as_str())
                || t.file_session_path() == Some(path.as_str())
        }) {
            self.active_terminal = idx;
            self.configure_active_as_file_session(&path);
            self.start_log_file_load(&path);
            self.mark_viewport_dirty();
            self.last_stats_at = None;
            return;
        }

        // Never convert a live PTY session into a file row — always open a
        // dedicated file session (appears under FILES in the sidebar).
        self.terminal_add_blank();
        self.configure_active_as_file_session(&path);
        self.start_log_file_load(&path);
    }

    pub(crate) fn configure_active_as_file_session(&mut self, path: &str) {
        let parent = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty());
        let id = self.active_terminal().id.clone();
        {
            let terminal = self.active_terminal_mut();
            terminal.launch.log_file = Some(path.to_string());
            terminal.launch.command = None;
            terminal.launch.args.clear();
            terminal.launch.wsl = false;
            terminal.launch.wsl_distro = None;
            terminal.process_started = true;
            terminal.running = false;
            if let Some(cwd) = parent {
                terminal.cwd = cwd;
            }
            terminal.sync_primary_tab_identity();
            terminal.disable_follow_all_views();
        }
        // Stop any PTY that might still be attached (should not happen on a
        // blank terminal, but keeps reopen/reload safe).
        if let Some(mut pty) = self.ptys.remove(&id) {
            pty.stop();
        }
    }

    pub(crate) fn start_log_file_load(&mut self, path: &str) {
        let load = match FileLoadState::open(path) {
            Ok(state) => state,
            Err(message) => {
                self.status_message = message.clone();
                self.push_event(json!({"type":"status","message": message}));
                return;
            }
        };
        let display_path = load.path.clone();
        let term = self.viewport_pty_size();

        {
            let terminal = self.active_terminal_mut();
            terminal.file_load = None;
            terminal.file_backed = None;
            terminal.pending_file_window = None;
            terminal.buffer_line_start = 0;
            terminal.buffer_line_end = 0;
            terminal.buffer.clear();
            terminal.ingest
                .reset_with_size(term.cols as usize, term.rows as usize);
            for view in &mut terminal.views {
                view.clear_flat_lines();
            }
            terminal.file_load = Some(load);
            terminal.running = false;
        }

        self.status_message = format!("Loading: {display_path}…");
        self.push_event(json!({"type":"status","message": self.status_message}));
    }

    pub(crate) fn advance_file_load(&mut self) {
        if !self.has_active_terminal() {
            return;
        }
        let Some(mut load) = self.active_terminal_mut().file_load.take() else {
            return;
        };

        let was_empty = self.active_terminal().buffer.raw_lines().is_empty();
        let (lines, content_done, index_done) = match load.tick() {
            Ok(result) => result,
            Err(message) => {
                self.active_terminal_mut().file_load = None;
                self.status_message = message.clone();
                self.push_event(json!({"type":"status","message": message}));
                return;
            }
        };

        let first_batch = was_empty && !lines.is_empty();
        let got_lines = !lines.is_empty();
        if got_lines {
            // Quiet ingest during load — paint only on first/last content batch.
            self.push_lines(lines, false);
            let terminal = self.active_terminal_mut();
            if load.content_start_byte == 0 {
                terminal.buffer_line_start = 0;
            } else if load.index_finished {
                terminal.buffer_line_start = load.index.line_at_offset(load.content_start_byte);
            }
            terminal.buffer_line_end = terminal.buffer_line_start + load.content_lines_read;
        }

        if first_batch || (content_done && got_lines) {
            self.mark_all_views_dirty();
            self.mark_viewport_dirty();
        }

        if load.is_finished() {
            {
                let terminal = self.active_terminal_mut();
                if let Some(last) = terminal.parser.flush_pending() {
                    let shifted = terminal.buffer.add(last);
                    terminal.buffer_line_start += shifted as u64;
                }
                if load.content_start_byte > 0 {
                    terminal.buffer_line_start = load.index.line_at_offset(load.content_start_byte);
                }
                terminal.buffer_line_end = terminal.buffer_line_start + load.content_lines_read;
                match load.into_backed() {
                    Ok(backed) => terminal.file_backed = Some(backed),
                    Err(message) => {
                        self.status_message = message.clone();
                        self.push_event(json!({"type":"status","message": message}));
                        return;
                    }
                }
                terminal.file_load = None;
            }
            self.mark_all_views_dirty();
            self.mark_viewport_dirty();
            let path = self
                .active_terminal()
                .file_backed
                .as_ref()
                .map(|b| b.path.clone())
                .unwrap_or_default();
            let total = self
                .active_terminal()
                .file_backed
                .as_ref()
                .map(|b| b.index.total_lines())
                .unwrap_or(0);
            self.status_message = format!("Opened: {path} ({total} lines, scroll for full file)");
            self.push_event(json!({"type":"status","message": self.status_message}));
        } else {
            let path = load.path.clone();
            let lines_read = load.content_lines_read;
            let index_pct = (load.index_progress() * 100.0) as u32;
            self.active_terminal_mut().file_load = Some(load);
            // Throttle status churn: update only every ~5% while indexing.
            if content_done && !index_done {
                if index_pct % 5 == 0 || index_pct >= 99 {
                    self.status_message =
                        format!("Indexing: {path}… ({index_pct}%, {lines_read} lines visible)");
                }
            } else if !content_done {
                self.status_message =
                    format!("Loading: {path}… ({lines_read} lines)");
            }
        }
    }

    pub(crate) fn maybe_prefetch_file_window(&mut self) {
        if !self.has_active_terminal() || self.active_terminal().file_load.is_some() {
            return;
        }
        if self.active_terminal().pending_file_window.is_some() {
            return;
        }

        let metrics = self.renderer.metrics();
        let row_stride = metrics.row_stride;
        let (need_up, need_down, scroll_y, _content_h, window_start, window_end, total_lines) = {
            let terminal = self.active_terminal();
            let Some(backed) = &terminal.file_backed else {
                return;
            };
            let view = terminal.active_view();
            // Cached row count — never allocate a full wrap layout on every tick.
            let rows = view.cached_visual_rows(
                self.viewport_width,
                metrics.cell_width,
                count_visual_rows,
            );
            let content_h = rows as f32 * row_stride;
            let near_top = terminal.scroll_offset_y <= PREFETCH_SCROLL_PX;
            let near_bottom =
                terminal.scroll_offset_y + self.viewport_height as f32 >= content_h - PREFETCH_SCROLL_PX;
            (
                near_top && terminal.buffer_line_start > 0,
                near_bottom && terminal.buffer_line_end < backed.index.total_lines(),
                terminal.scroll_offset_y,
                content_h,
                terminal.buffer_line_start,
                terminal.buffer_line_end,
                backed.index.total_lines(),
            )
        };

        if need_up {
            let new_start = window_start.saturating_sub(PREFETCH_RAW_LINES as u64);
            let scroll_adjust = (window_start - new_start) as f32 * row_stride;
            self.request_file_window_at(new_start, scroll_y + scroll_adjust);
        } else if need_down {
            let window = self.file_view_window_lines() as u64;
            let new_start = window_end.min(total_lines.saturating_sub(window));
            self.request_file_window_at(new_start, 0.0);
        }
    }

    pub(crate) fn file_view_window_lines(&self) -> usize {
        self.config
            .max_scrollback_lines
            .min(WINDOW_RAW_LINES)
            .min(FILE_VIEW_WINDOW_LINES)
    }

    pub(crate) fn request_file_window_at(&mut self, new_start: u64, scroll_y: f32) {
        let window = self.file_view_window_lines() as u64;
        let end_line = {
            let terminal = self.active_terminal();
            let Some(backed) = &terminal.file_backed else {
                return;
            };
            if terminal
                .pending_file_window
                .as_ref()
                .is_some_and(|p| p.new_start == new_start)
            {
                return;
            }
            (new_start + window).min(backed.index.total_lines())
        };
        if end_line <= new_start {
            return;
        }
        self.active_terminal_mut().pending_file_window = Some(PendingFileWindow {
            new_start,
            scroll_y,
            next_line: new_start,
            end_line,
            lines: Vec::new(),
        });
        self.mark_viewport_dirty();
    }

    pub(crate) fn advance_pending_file_window(&mut self) {
        if !self.has_active_terminal() {
            return;
        }

        let read_chunk = {
            let terminal = self.active_terminal_mut();
            let Some(pending) = terminal.pending_file_window.as_mut() else {
                return;
            };
            let Some(backed) = terminal.file_backed.as_mut() else {
                terminal.pending_file_window = None;
                return;
            };
            let remaining = pending.end_line.saturating_sub(pending.next_line) as usize;
            if remaining == 0 {
                Ok(None)
            } else {
                let count = remaining.min(FILE_WINDOW_LINES_PER_TICK);
                let next_line = pending.next_line;
                backed
                    .read_lines(next_line, count)
                    .map(Some)
                    .map_err(|message| message)
            }
        };

        match read_chunk {
            Err(message) => {
                self.active_terminal_mut().pending_file_window = None;
                self.status_message = message.clone();
                self.push_event(json!({"type":"status","message": message}));
            }
            Ok(None) => {
                let pending = self.active_terminal_mut().pending_file_window.take().unwrap();
                self.finish_file_window(pending);
            }
            Ok(Some(lines)) if lines.is_empty() => {
                let pending = self.active_terminal_mut().pending_file_window.take().unwrap();
                self.finish_file_window(pending);
            }
            Ok(Some(lines)) => {
                let finished = {
                    let terminal = self.active_terminal_mut();
                    let pending = terminal.pending_file_window.as_mut().unwrap();
                    let read = lines.len() as u64;
                    pending.lines.extend(lines);
                    pending.next_line += read;
                    pending.next_line >= pending.end_line
                };
                if finished {
                    let pending = self.active_terminal_mut().pending_file_window.take().unwrap();
                    self.finish_file_window(pending);
                } else {
                    self.mark_viewport_dirty();
                }
            }
        }
    }

    pub(crate) fn finish_file_window(&mut self, pending: PendingFileWindow) {
        let format = self.current_format();
        let raw_count = pending.lines.len() as u64;
        {
            let terminal = self.active_terminal_mut();
            terminal.parser = RecordParser::new(format);
            let mut records = Vec::new();
            for line in pending.lines {
                records.extend(terminal.parser.push_line(line));
            }
            if let Some(last) = terminal.parser.flush_pending() {
                records.push(last);
            }
            terminal.buffer.replace_all(records);
            terminal.buffer_line_start = pending.new_start;
            terminal.buffer_line_end = pending.new_start + raw_count;
            terminal.scroll_offset_y = pending.scroll_y;
            terminal.selection = None;
            terminal.pending_file_window = None;
        }
        self.mark_all_views_dirty();
        self.mark_viewport_dirty();
    }

    /// Advance whole-file match scan for the active file filter tab.
    pub(crate) fn advance_file_match_scan(&mut self) {
        if !self.has_active_terminal() || !self.active_terminal().is_file_session() {
            return;
        }
        if self.active_terminal().file_backed.is_none() {
            return;
        }

        let needs = {
            let view = self.active_view();
            view.uses_match_index()
        };
        if !needs {
            let view = self.active_view_mut();
            if view.match_scan_pos.is_some() || !view.match_offsets.is_empty() {
                view.clear_match_index();
                view.mark_flat_lines_dirty();
            }
            return;
        }

        // Start scan if filters require an index but none is running/complete.
        {
            let view = self.active_view_mut();
            if view.match_scan_pos.is_none()
                && view.match_offsets.is_empty()
                && view.is_flat_lines_dirty()
            {
                view.invalidate_match_index();
            }
        }

        let Some(from) = self.active_view().match_scan_pos else {
            return;
        };

        let file_size = self
            .active_terminal()
            .file_backed
            .as_ref()
            .map(|b| b.index.file_size())
            .unwrap_or(0);

        let (filters, severity) = {
            let view = self.active_view();
            (view.filters().to_vec(), view.severity_filter)
        };
        let filter_engine = crate::core::filter::FilterEngine::new(filters);

        let result = {
            let mut offsets = self.active_view().match_offsets.clone();
            let terminal = self.active_terminal_mut();
            let backed = terminal.file_backed.as_mut().unwrap();
            let scan = crate::file_match::scan_match_chunk(
                &mut backed.file,
                file_size,
                from,
                crate::file_match::MATCH_SCAN_BYTES_PER_TICK,
                &filter_engine,
                severity,
                &mut offsets,
            );
            (scan, offsets)
        };

        match result {
            (Ok((next, done)), offsets) => {
                let view = self.active_view_mut();
                view.match_offsets = offsets;
                if done {
                    view.match_scan_pos = None;
                    view.request_match_rebuild();
                    self.status_message = format!(
                        "Filter scan complete ({} matches)",
                        view.match_offsets.len()
                    );
                    self.push_event(json!({"type":"status","message": self.status_message}));
                    self.apply_match_window();
                } else {
                    view.match_scan_pos = Some(next);
                    let pct = if file_size == 0 {
                        100
                    } else {
                        ((next as f32 / file_size as f32) * 100.0) as u32
                    };
                    self.status_message = format!(
                        "Scanning filters… {pct}% ({} matches)",
                        view.match_offsets.len()
                    );
                }
                self.mark_viewport_dirty();
                self.last_stats_at = None;
            }
            (Err(message), _) => {
                self.active_view_mut().match_scan_pos = None;
                self.status_message = message.clone();
                self.push_event(json!({"type":"status","message": message}));
            }
        }
    }

    /// Materialize a window of match lines into the active view's flat_lines.
    pub(crate) fn apply_match_window(&mut self) {
        if !self.has_active_terminal() || self.active_terminal().file_backed.is_none() {
            return;
        }
        if !self.active_view().uses_match_index() {
            return;
        }
        if self.active_view().match_scan_pos.is_some() {
            return;
        }

        let metrics = self.renderer.metrics();
        let scroll_y = self.active_terminal().scroll_offset_y;
        let first_match = ((scroll_y / metrics.row_stride).floor() as usize)
            .saturating_sub(crate::file_match::MATCH_WINDOW_LINES / 4);

        let (lines, start) = {
            let offsets = self.active_view().match_offsets.clone();
            let start = first_match.min(offsets.len());
            let terminal = self.active_terminal_mut();
            let backed = terminal.file_backed.as_mut().unwrap();
            let lines = match crate::file_match::read_match_window(
                &mut backed.file,
                &offsets,
                start,
                crate::file_match::MATCH_WINDOW_LINES,
            ) {
                Ok(lines) => lines,
                Err(err) => {
                    self.status_message = err;
                    return;
                }
            };
            (lines, start)
        };
        self.active_view_mut().match_window_start = start;

        let flat = crate::core::visible::flat_lines_from_raw_lines(&lines, start as u64);
        self.active_view_mut().set_match_flat_lines(flat);
        self.mark_viewport_dirty();
    }
}
