use super::*;

impl Engine {
    pub(crate) fn scroll_by_lines(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let row_stride = self.renderer.metrics().row_stride;
        let max_scroll = if self.active_terminal().is_file_session()
            && self.active_terminal().file_backed.is_some()
        {
            self.local_window_max_scroll()
        } else {
            self.max_scroll_offset()
        };
        let terminal = self.active_terminal_mut();
        terminal.scroll_offset_y =
            (terminal.scroll_offset_y + delta as f32 * row_stride).clamp(0.0, max_scroll);
        self.sync_follow_from_scroll();
        self.maybe_prefetch_file_window();
        if self.active_terminal().is_file_session() && self.active_view().uses_match_index() {
            self.apply_match_window();
        }
        self.mark_viewport_dirty();
    }

    pub(crate) fn scroll_page(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        let page = self.viewport_height as f32 * 0.9;
        let max_scroll = if self.active_terminal().is_file_session()
            && self.active_terminal().file_backed.is_some()
        {
            self.local_window_max_scroll()
        } else {
            self.max_scroll_offset()
        };
        let terminal = self.active_terminal_mut();
        terminal.scroll_offset_y =
            (terminal.scroll_offset_y + direction.signum() as f32 * page).clamp(0.0, max_scroll);
        self.sync_follow_from_scroll();
        self.maybe_prefetch_file_window();
        if self.active_terminal().is_file_session() && self.active_view().uses_match_index() {
            self.apply_match_window();
        }
        self.mark_viewport_dirty();
    }

    pub(crate) fn scroll_to_start(&mut self) {
        self.active_view_mut().auto_follow = false;
        self.last_stats_at = None;
        if self.active_terminal().file_backed.is_some() {
            self.request_file_window_at(0, 0.0);
        } else {
            self.active_terminal_mut().scroll_offset_y = 0.0;
            self.materialize_live_terminal_tab();
        }
        self.mark_viewport_dirty();
    }

    pub(crate) fn scroll_to_end(&mut self) {
        if self.active_terminal().file_backed.is_some() {
            let max = self.max_scroll_offset();
            self.scroll_file_to_global_offset(max);
            self.last_stats_at = None;
            self.mark_viewport_dirty();
            return;
        }
        if !self.active_terminal().is_file_session() {
            self.active_view_mut().auto_follow = true;
        }
        self.active_terminal_mut().scroll_offset_y = self.max_scroll_offset();
        self.last_stats_at = None;
        self.mark_viewport_dirty();
    }

    pub(crate) fn scroll_to_row_index(&mut self, row: usize) {
        self.active_view_mut().auto_follow = false;
        let metrics = self.renderer.metrics();
        let row_top = row as f32 * metrics.row_stride;
        let row_bottom = row_top + metrics.row_height;
        let viewport_height = self.viewport_height as f32;
        let terminal = self.active_terminal_mut();
        let visible_top = terminal.scroll_offset_y;
        let visible_bottom = terminal.scroll_offset_y + viewport_height;
        if row_top < visible_top {
            terminal.scroll_offset_y = row_top.max(0.0);
        } else if row_bottom > visible_bottom {
            terminal.scroll_offset_y = (row_bottom - viewport_height).max(0.0);
        }
    }

    /// Stick Follow when the viewport is at (or past) the bottom; clear it when scrolled away.
    pub(crate) fn sync_follow_from_scroll(&mut self) {
        if self.active_terminal().is_file_session() {
            if self.active_view().auto_follow {
                self.active_view_mut().auto_follow = false;
                self.last_stats_at = None;
            }
            return;
        }
        let max_scroll = self.max_scroll_offset();
        let scroll_y = self.active_terminal().scroll_offset_y;
        let at_bottom = max_scroll <= 0.5 || scroll_y >= max_scroll - 1.0;
        let was_follow = self.active_view().auto_follow;
        let view = self.active_view_mut();
        if view.auto_follow == at_bottom {
            return;
        }
        view.auto_follow = at_bottom;
        self.last_stats_at = None;
        if was_follow && !at_bottom {
            self.materialize_live_terminal_tab();
        }
    }

    pub(crate) fn max_scroll_offset(&self) -> f32 {
        let metrics = self.renderer.metrics();
        let stride = metrics.row_stride;

        if self.has_active_terminal() {
            let terminal = self.active_terminal();
            let view = terminal.active_view();
            if terminal.is_file_session()
                && view.uses_match_index()
                && view.match_scan_pos.is_none()
            {
                let total = view.match_offsets.len();
                let content_h = total as f32 * stride;
                return (content_h - self.viewport_height as f32).max(0.0);
            }
            if let Some(backed) = &terminal.file_backed {
                // Whole-file scrollbar range (1 file line ≈ 1 visual row for unread spans).
                // When the last window is resident, raise the range to the real visual
                // height so Wrap ON can still scroll to the true bottom.
                let total = backed.index.total_lines();
                let window = self.file_view_window_lines() as u64;
                let max_start = total.saturating_sub(window);
                let mut content_h = total as f32 * stride;
                let (start, local_rows_h) = if let Some(pending) = &terminal.pending_file_window {
                    if pending.new_start >= max_start {
                        let rows = view.cached_visual_rows(
                            self.viewport_width,
                            metrics.cell_width,
                            count_visual_rows,
                        );
                        // Pending EOF: estimate at least raw window height; after load
                        // the resident branch below will refine.
                        (
                            pending.new_start,
                            (window as f32 * stride).max(rows as f32 * stride),
                        )
                    } else {
                        (pending.new_start, 0.0)
                    }
                } else if terminal.buffer_line_start >= max_start {
                    let rows = view.cached_visual_rows(
                        self.viewport_width,
                        metrics.cell_width,
                        count_visual_rows,
                    );
                    (terminal.buffer_line_start, rows as f32 * stride)
                } else {
                    (0, 0.0)
                };
                if local_rows_h > 0.0 {
                    content_h = content_h.max(start as f32 * stride + local_rows_h);
                }
                return (content_h - self.viewport_height as f32).max(0.0);
            }
        }

        if self.paints_live_vt_grid() {
            // Paint is live-screen only; scrollbar range is retained ring + screen
            // so the thumb stays small (~viewport / (cap+rows)). Screen-only max
            // made the thumb ~half the track under WRAP while the line counter
            // correctly showed ever-seen hundreds of thousands.
            let wrap = self.active_view().wrap_lines;
            let committed = self.active_terminal().buffer.records_len();
            let grid_visual = if wrap {
                let lines = self.active_terminal().ingest.grid_flat_lines();
                count_visual_rows(
                    &lines,
                    true,
                    self.viewport_width,
                    metrics.cell_width,
                )
            } else {
                self.active_terminal().ingest.size().1
            };
            let content_h = (committed + grid_visual) as f32 * stride;
            return (content_h - self.viewport_height as f32).max(0.0);
        }

        let view = self.active_view();
        let rows = view.cached_visual_rows(
            self.viewport_width,
            metrics.cell_width,
            count_visual_rows,
        );
        let content_h = rows as f32 * stride;
        (content_h - self.viewport_height as f32).max(0.0)
    }

    /// Max local scroll within the currently loaded file/match window.
    pub(crate) fn local_window_max_scroll(&self) -> f32 {
        let metrics = self.renderer.metrics();
        let view = self.active_view();
        let rows = view.cached_visual_rows(
            self.viewport_width,
            metrics.cell_width,
            count_visual_rows,
        );
        (rows as f32 * metrics.row_stride - self.viewport_height as f32).max(0.0)
    }

    /// Scrollbar / stats Y for the active session (global for FILES).
    pub(crate) fn stats_scroll_y(&self) -> f32 {
        if !self.has_active_terminal() {
            return 0.0;
        }
        let terminal = self.active_terminal();
        let stride = self.renderer.metrics().row_stride;
        let max_y = self.max_scroll_offset();
        // While a window jump is in flight, report the target so the thumb does not spring back.
        if terminal.is_file_session() {
            if let Some(pending) = &terminal.pending_file_window {
                let raw = pending.new_start as f32 * stride + pending.scroll_y;
                return raw.clamp(0.0, max_y);
            }
        }
        let local = terminal.scroll_offset_y;
        if !terminal.is_file_session() {
            return local.clamp(0.0, max_y);
        }
        let view = terminal.active_view();
        let y = if view.uses_match_index() && view.match_scan_pos.is_none() {
            view.match_window_start as f32 * stride + local
        } else if terminal.file_backed.is_some() {
            terminal.buffer_line_start as f32 * stride + local
        } else {
            local
        };
        y.clamp(0.0, max_y)
    }

    /// 1-based line at the **bottom** of the viewport and total lines for the status bar.
    ///
    /// Using the top line left EOF looking short by roughly one screen (`363177 / 363194`).
    pub(crate) fn viewport_line_position(&self) -> (u64, u64) {
        if !self.has_active_terminal() {
            return (0, 0);
        }
        let metrics = self.renderer.metrics();
        let stride = metrics.row_stride.max(0.001);
        let terminal = self.active_terminal();
        let view = terminal.active_view();
        let viewport_h = self.viewport_height as f32;

        if terminal.is_file_session() {
            if let Some(backed) = &terminal.file_backed {
                let total = backed.index.total_lines();
                if total == 0 {
                    return (0, 0);
                }
                let (base, local_y, pin_end) = if let Some(pending) = &terminal.pending_file_window
                {
                    (
                        pending.new_start,
                        pending.scroll_y,
                        pending.scroll_y >= 1.0e20,
                    )
                } else {
                    (terminal.buffer_line_start, terminal.scroll_offset_y, false)
                };
                if pin_end {
                    return (total, total);
                }
                let index = view.ensure_visual_row_index(self.viewport_width, metrics.cell_width);
                let bottom_visual = ((local_y + viewport_h - 0.01) / stride)
                    .floor()
                    .max(0.0) as usize;
                let bottom_visual = bottom_visual.min(index.total_rows().saturating_sub(1));
                let flat = index
                    .flat_at_visual_row(bottom_visual)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let cur = (base + flat as u64 + 1).min(total.max(1));
                // At (or past) max scroll, snap to last file line so EOF reads `N / N`.
                let at_eof = self.stats_scroll_y() + 1.0 >= self.max_scroll_offset();
                let cur = if at_eof { total } else { cur };
                return (cur, total);
            }
            return (0, 0);
        }

        if self.paints_live_vt_grid() {
            // Monotonic lines-ever-seen (dropped + retained + live rows), not the
            // capped ring size — otherwise Follow shows ~1000/1000 forever and
            // looks like paging through 1000-line chunks.
            let ever = terminal.buffer.dropped_count() as u64
                + terminal.buffer.records_len() as u64
                + terminal.ingest.size().1 as u64;
            if ever == 0 {
                return (0, 0);
            }
            return (ever, ever);
        }

        let total = view.flat_lines.len() as u64;
        if total == 0 {
            return (0, 0);
        }
        let local_y = terminal.scroll_offset_y;
        let index = view.ensure_visual_row_index(self.viewport_width, metrics.cell_width);
        let bottom_visual = ((local_y + viewport_h - 0.01) / stride)
            .floor()
            .max(0.0) as usize;
        let bottom_visual = bottom_visual.min(index.total_rows().saturating_sub(1));
        let flat = index
            .flat_at_visual_row(bottom_visual)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let cur = (flat as u64 + 1).min(total);
        let at_eof = local_y + 1.0 >= self.local_window_max_scroll() || total <= 1;
        let cur = if at_eof { total } else { cur };
        (cur, total)
    }

    pub(crate) fn current_max_scroll_x(&self) -> f32 {
        let view = self.active_view();
        if view.wrap_lines {
            return 0.0;
        }
        let metrics = self.renderer.metrics();
        max_scroll_x(
            &view.flat_lines,
            self.viewport_width,
            metrics.cell_width,
        )
    }

    pub(crate) fn set_wrap_lines(&mut self, wrap: bool) {
        let view = self.active_view_mut();
        view.wrap_lines = wrap;
        view.invalidate_visual_rows_cache();
        self.active_terminal_mut().scroll_x = 0.0;
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn scroll_horizontal(&mut self, delta: f32) {
        if self.active_view().wrap_lines || delta == 0.0 {
            return;
        }
        let max_x = self.current_max_scroll_x();
        let terminal = self.active_terminal_mut();
        terminal.scroll_x = (terminal.scroll_x + delta).clamp(0.0, max_x);
        self.mark_viewport_dirty();
    }

    pub(crate) fn set_scroll_x(&mut self, offset: f32) {
        if self.active_view().wrap_lines {
            return;
        }
        let max_x = self.current_max_scroll_x();
        self.active_terminal_mut().scroll_x = offset.clamp(0.0, max_x);
        self.mark_viewport_dirty();
    }

    pub(crate) fn selection_at(&mut self, x: f32, y: f32, extend: bool, click_count: u32) {
        let view = self.active_view();
        let metrics = self.renderer.metrics();
        let cell_width = metrics.cell_width;
        let visual = build_visual_lines(
            &view.flat_lines,
            view.wrap_lines,
            self.viewport_width,
            metrics.cell_width,
        );
        if visual.is_empty() {
            return;
        }
        let terminal = self.active_terminal();
        let pos = pos_at_pixel(
            x,
            y,
            terminal.scroll_offset_y,
            terminal.scroll_x,
            view.wrap_lines,
            metrics,
            &visual,
            &view.flat_lines,
        );
        self.active_view_mut().auto_follow = false;

        // Collapse toggle: disclosure gutter, or anywhere on a collapsed preview row.
        if !extend && click_count == 1 {
            if let Some(line) = self.active_view().flat_lines.get(pos.line_index) {
                let disclosure_hit = x < (LEFT_PAD as f32 + cell_width as f32 * 1.5);
                if line.collapsed || (line.collapsible && line.line_index == 0 && disclosure_hit) {
                    let id = line.record_id;
                    self.active_view_mut().toggle_record_collapse(id);
                    self.active_terminal_mut().selection = None;
                    self.mark_viewport_dirty();
                    self.last_stats_at = None;
                    return;
                }
            }
        }

        if !extend && click_count >= 3 {
            if let Some(sel) = record_selection_at(&self.active_view().flat_lines, pos) {
                self.active_terminal_mut().selection = Some(sel);
                return;
            }
        }
        if !extend && click_count >= 2 {
            if let Some(sel) = word_selection_at(&self.active_view().flat_lines, pos) {
                self.active_terminal_mut().selection = Some(sel);
                return;
            }
        }

        let terminal = self.active_terminal_mut();
        if extend {
            if let Some(sel) = terminal.selection.as_mut() {
                sel.caret = pos;
            } else {
                terminal.selection = Some(TextSelection::new(pos, pos));
            }
        } else {
            terminal.selection = Some(TextSelection::new(pos, pos));
        }
    }

    pub fn selection_text(&self) -> Option<String> {
        if !self.has_active_terminal() {
            return None;
        }
        let sel = self.active_terminal().selection.filter(|s| !s.is_empty())?;
        Some(selection_plain_text(&self.active_view().flat_lines, &sel))
    }
}
