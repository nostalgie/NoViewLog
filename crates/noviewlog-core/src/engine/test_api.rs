use super::*;

impl Engine {
    #[cfg(test)]
    pub fn active_is_file_session_for_test(&self) -> bool {
        self.has_active_terminal() && self.active_terminal().is_file_session()
    }

    #[cfg(test)]
    pub fn active_terminal_running_for_test(&self) -> bool {
        self.has_active_terminal() && self.active_terminal().running
    }

    #[cfg(test)]
    pub fn mark_active_process_started_for_test(&mut self) {
        if self.has_active_terminal() {
            self.active_terminal_mut().process_started = true;
        }
    }

    #[cfg(test)]
    pub fn process_started_for_test(&self) -> bool {
        self.has_active_terminal() && self.active_terminal().process_started
    }

    #[cfg(test)]
    pub fn pty_generation_for_test(&self) -> u64 {
        if !self.has_active_terminal() {
            return 0;
        }
        self.active_terminal().pty_generation
    }

    #[cfg(test)]
    pub fn inject_pty_exit_for_test(&self, id: &str, code: i32, generation: u64) {
        let _ = self.pty_tx.send(crate::pty::PtyEvent::Exit {
            id: id.to_string(),
            code,
            generation,
        });
    }

    #[cfg(test)]
    pub fn status_message_for_test(&self) -> String {
        self.status_message.clone()
    }

    #[cfg(test)]
    pub fn terminal_is_file_session_for_test(&self, index: usize) -> bool {
        self.terminals
            .get(index)
            .is_some_and(|t| t.is_file_session())
    }

    #[cfg(test)]
    pub fn finish_pending_file_window_for_test(&mut self) {
        while self.has_active_terminal() && self.active_terminal().pending_file_window.is_some() {
            self.advance_pending_file_window();
            self.rebuild_if_needed();
        }
    }

    #[cfg(test)]
    pub fn pending_file_window_for_test(&self) -> bool {
        self.has_active_terminal() && self.active_terminal().pending_file_window.is_some()
    }

    #[cfg(test)]
    pub fn rebuild_if_needed_for_test(&mut self) {
        self.rebuild_if_needed();
    }

    #[cfg(test)]
    pub fn mark_running_for_test(&mut self) {
        if self.has_active_terminal() {
            self.active_terminal_mut().running = true;
        }
    }

    /// Append lines like PTY ingest (`mark_dirty = false`) so inactive views keep
    /// their record cursors and catch up incrementally when selected.
    #[cfg(test)]
    pub fn push_streaming_lines_for_test(&mut self, lines: impl IntoIterator<Item = String>) {
        self.push_lines(lines, false);
    }

    #[cfg(test)]
    pub fn view_record_cursor_for_test(&self, index: usize) -> Option<usize> {
        if !self.has_active_terminal() {
            return None;
        }
        self.active_terminal()
            .views
            .get(index)
            .map(|v| v.flat_lines_record_cursor)
    }

    #[cfg(test)]
    pub fn view_flat_line_count_for_test(&self, index: usize) -> Option<usize> {
        if !self.has_active_terminal() {
            return None;
        }
        self.active_terminal()
            .views
            .get(index)
            .map(|v| v.flat_lines.len())
    }

    #[cfg(test)]
    pub fn first_flat_line_for_test(&self) -> Option<String> {
        if !self.has_active_terminal() {
            return None;
        }
        self.active_view()
            .flat_lines
            .first()
            .map(|l| l.raw.clone())
    }

    #[cfg(test)]
    pub fn file_load_pending_for_test(&self) -> bool {
        self.has_active_terminal() && self.active_terminal().file_load.is_some()
    }

    #[cfg(test)]
    pub fn buffer_record_count_for_test(&self) -> usize {
        if !self.has_active_terminal() {
            return 0;
        }
        self.active_terminal().buffer.records().len()
    }

    #[cfg(test)]
    pub fn finish_file_load_for_test(&mut self) {
        while self.file_load_pending_for_test() {
            self.advance_file_load();
        }
    }

    #[cfg(test)]
    pub fn events_len_for_test(&self) -> usize {
        self.events.len()
    }

    #[cfg(test)]
    pub fn enqueue_event_for_test(&mut self, json: String) {
        self.events.push_back(json);
    }

    #[cfg(test)]
    pub fn terminal_running_for_test(&self, terminal_id: &str) -> Option<bool> {
        self.terminals
            .iter()
            .find(|t| t.id == terminal_id)
            .map(|t| t.running)
    }

    #[cfg(test)]
    pub fn tab_configs_for_test(&self) -> Vec<TabConfig> {
        self.active_terminal()
            .views
            .iter()
            .map(|v| v.to_tab_config())
            .collect()
    }

    #[cfg(test)]
    pub fn active_tab_index_for_test(&self) -> usize {
        self.active_terminal().active_view
    }

    #[cfg(test)]
    pub fn selection_text_for_test(&self) -> Option<String> {
        self.selection_text()
    }

    #[cfg(test)]
    pub fn scroll_x_for_test(&self) -> f32 {
        self.active_terminal().scroll_x
    }

    #[cfg(test)]
    pub fn wrap_lines_for_test(&self) -> bool {
        self.active_view().wrap_lines
    }

    #[cfg(test)]
    pub fn viewport_font_size_for_test(&self) -> f32 {
        self.renderer.font_size()
    }

    #[cfg(test)]
    pub fn viewport_row_stride_for_test(&self) -> f32 {
        self.renderer.metrics().row_stride
    }

    #[cfg(test)]
    pub fn viewport_dirty_for_test(&self) -> bool {
        self.viewport_dirty
    }

    #[cfg(test)]
    pub fn pty_cols_for_test(&self) -> u16 {
        self.viewport_pty_size().cols
    }

    #[cfg(test)]
    pub fn auto_follow_for_test(&self) -> bool {
        self.active_view().auto_follow
    }

    #[cfg(test)]
    pub fn can_restore_closed_tab_for_test(&self) -> bool {
        !self.active_terminal().closed_tabs.is_empty()
    }

    #[cfg(test)]
    pub fn terminals_for_test(&self) -> Vec<(String, String, bool)> {
        self.terminals
            .iter()
            .map(|t| (t.id.clone(), t.label(), t.running))
            .collect()
    }

    #[cfg(test)]
    pub fn active_terminal_index_for_test(&self) -> usize {
        self.active_terminal
    }

    #[cfg(test)]
    pub fn active_terminal_id_for_test(&self) -> String {
        self.active_terminal().id.clone()
    }

    #[cfg(test)]
    pub fn presets_for_test(&self) -> &HashMap<String, PresetConfig> {
        &self.config.presets
    }

    #[cfg(test)]
    pub fn active_tab_config_for_test(&self) -> crate::core::types::TabConfig {
        self.active_view().to_tab_config()
    }

    #[cfg(test)]
    pub fn search_match_index_for_test(&self) -> usize {
        self.active_view().search_match_index
    }

    #[cfg(test)]
    pub fn search_match_count_for_test(&self) -> usize {
        self.active_view().search_matches.len()
    }

    #[cfg(test)]
    pub fn search_counter_for_test(&self) -> String {
        self.active_view().search_counter_label()
    }

    #[cfg(test)]
    pub fn active_view_severity_for_test(&self) -> String {
        self.active_view().severity_filter.as_str().to_string()
    }

    #[cfg(test)]
    pub fn active_flat_line_count_for_test(&self) -> usize {
        self.active_view().flat_lines.len()
    }

    #[cfg(test)]
    pub fn rebuild_active_for_test(&mut self) {
        self.rebuild_if_needed();
    }

    #[cfg(test)]
    pub fn push_lines_for_test(&mut self, lines: impl IntoIterator<Item = String>) {
        self.push_lines(lines, true);
        self.rebuild_if_needed();
    }
}
