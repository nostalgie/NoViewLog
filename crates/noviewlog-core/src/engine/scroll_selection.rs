use super::*;

impl Engine {
    pub(crate) fn scroll_by_lines(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let row_stride = self.renderer.metrics().row_stride;
        let max_scroll = self.max_scroll_offset();
        let terminal = self.active_terminal_mut();
        terminal.scroll_offset_y =
            (terminal.scroll_offset_y + delta as f32 * row_stride).clamp(0.0, max_scroll);
        self.sync_follow_from_scroll();
        self.maybe_prefetch_file_window();
        self.mark_viewport_dirty();
    }

    pub(crate) fn scroll_page(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        let page = self.viewport_height as f32 * 0.9;
        let max_scroll = self.max_scroll_offset();
        let terminal = self.active_terminal_mut();
        terminal.scroll_offset_y =
            (terminal.scroll_offset_y + direction.signum() as f32 * page).clamp(0.0, max_scroll);
        self.sync_follow_from_scroll();
        self.maybe_prefetch_file_window();
        self.mark_viewport_dirty();
    }

    pub(crate) fn scroll_to_start(&mut self) {
        self.active_view_mut().auto_follow = false;
        self.last_stats_at = None;
        if self.active_terminal().file_backed.is_some() {
            self.request_file_window_at(0, 0.0);
        } else {
            self.active_terminal_mut().scroll_offset_y = 0.0;
        }
        self.mark_viewport_dirty();
    }

    pub(crate) fn scroll_to_end(&mut self) {
        self.active_terminal_mut().scroll_offset_y = self.max_scroll_offset();
        self.active_view_mut().auto_follow = true;
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
        let max_scroll = self.max_scroll_offset();
        let scroll_y = self.active_terminal().scroll_offset_y;
        let at_bottom = max_scroll <= 0.5 || scroll_y >= max_scroll - 1.0;
        let view = self.active_view_mut();
        if view.auto_follow == at_bottom {
            return;
        }
        view.auto_follow = at_bottom;
        self.last_stats_at = None;
    }

    pub(crate) fn max_scroll_offset(&self) -> f32 {
        let metrics = self.renderer.metrics();
        let view = self.active_view();
        let visual = build_visual_lines(
            &view.flat_lines,
            view.wrap_lines,
            self.viewport_width,
            metrics.cell_width,
        );
        let mut content_h = visual.len() as f32 * metrics.row_stride;

        if self.has_active_terminal() {
            let terminal = self.active_terminal();
            if let Some(backed) = &terminal.file_backed {
                let lines_after = backed
                    .index
                    .total_lines()
                    .saturating_sub(terminal.buffer_line_end);
                content_h += lines_after as f32 * metrics.row_stride;
            }
        }

        (content_h - self.viewport_height as f32).max(0.0)
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
        self.active_view_mut().wrap_lines = wrap;
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
