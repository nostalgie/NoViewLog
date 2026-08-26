use super::*;
use super::events::{StatsSnapshot, StatsTab, StatsTerminal};

impl Engine {
    pub(crate) fn emit_stats(&mut self) {
        let now = Instant::now();
        let send = self
            .last_stats_at
            .is_none_or(|t| t.elapsed() > Duration::from_millis(250));
        if !send {
            return;
        }
        self.last_stats_at = Some(now);
        self.ensure_valid_state();

        let format_ids: Vec<String> = self.formats.keys().cloned().collect();
        let preset_names: Vec<String> = self.config.presets.keys().cloned().collect();
        let terminals: Vec<StatsTerminal> = self
            .terminals
            .iter()
            .enumerate()
            .map(|(i, t)| StatsTerminal {
                index: i,
                id: t.id.clone(),
                label: t.label(),
                running: t.running,
                cwd: t.cwd.clone(),
            })
            .collect();
        let active_terminal_idx = self.active_terminal;
        let (
            tabs,
            terminal_id,
            terminal_label,
            has_launch,
            lines,
            running,
            exit_code,
            active_tab,
            tab_count,
            dropped,
            can_restore,
            scroll_x,
            scroll_y,
            has_selection,
            auto_follow,
            tab_name,
            filters,
            search_query,
            search_regex,
            search_case_sensitive,
            search_whole_word,
            search_counter,
            search_error,
            search_has_matches,
            wrap_lines,
            file_total_lines,
            file_index_progress,
            file_window_start,
            file_lines_before,
            file_loading,
        ) = if let Some(terminal) = self.terminals.get(active_terminal_idx) {
            let view = terminal.active_view();
            let tabs: Vec<StatsTab> = terminal
                .views
                .iter()
                .enumerate()
                .map(|(i, v)| StatsTab {
                    index: i,
                    name: v.name.clone(),
                    is_console: i == 0,
                })
                .collect();
            (
                tabs,
                terminal.id.clone(),
                terminal.label(),
                terminal.launch.has_process_launch(),
                view.flat_lines.len(),
                terminal.running,
                terminal.exit_code,
                terminal.active_view,
                terminal.views.len(),
                terminal.buffer.dropped_count(),
                !terminal.closed_tabs.is_empty(),
                terminal.scroll_x,
                terminal.scroll_offset_y,
                terminal.selection.is_some_and(|s| !s.is_empty()),
                view.auto_follow,
                view.name.clone(),
                view.filters().to_vec(),
                view.search_query.clone(),
                view.search_regex,
                view.search_case_sensitive,
                view.search_whole_word,
                view.search_counter_label(),
                view.search_error.clone(),
                !view.search_matches.is_empty(),
                view.wrap_lines,
                terminal
                    .file_backed
                    .as_ref()
                    .map(|b| b.index.total_lines())
                    .unwrap_or(0),
                terminal
                    .file_load
                    .as_ref()
                    .map(|l| l.index_progress())
                    .unwrap_or(if terminal.file_backed.is_some() {
                        1.0
                    } else {
                        0.0
                    }),
                terminal.buffer_line_start,
                terminal.buffer_line_start,
                terminal.file_load.is_some(),
            )
        } else {
            (
                vec![],
                String::new(),
                String::new(),
                false,
                0usize,
                false,
                None,
                0usize,
                0usize,
                0usize,
                false,
                0.0f32,
                0.0f32,
                false,
                true,
                String::new(),
                vec![],
                String::new(),
                false,
                false,
                false,
                String::new(),
                None,
                false,
                true,
                0u64,
                0.0f32,
                0u64,
                0u64,
                false,
            )
        };
        let max_scroll_x = if self.has_active_terminal() {
            self.current_max_scroll_x()
        } else {
            0.0
        };
        let max_scroll_y = if self.has_active_terminal() {
            self.max_scroll_offset()
        } else {
            0.0
        };
        let severity_filter = if let Some(terminal) = self.terminals.get(active_terminal_idx) {
            terminal.active_view().severity_filter.as_str().to_string()
        } else {
            "all".to_string()
        };

        let snapshot = StatsSnapshot {
            event_type: "stats".to_string(),
            lines,
            running,
            status: self.status_message.clone(),
            exit_code,
            format_id: self.format_id.clone(),
            preset_name: self.preset_name.clone(),
            auto_follow,
            tab_name,
            active_tab,
            tab_count,
            console_tab: 0,
            is_console_tab: active_tab == 0,
            tabs,
            dropped,
            formats: format_ids,
            presets: preset_names,
            filters,
            search_query,
            search_regex,
            search_case_sensitive,
            search_whole_word,
            search_counter,
            search_error,
            search_has_matches,
            can_restore_closed_tab: can_restore,
            wrap_lines,
            scroll_x,
            max_scroll_x,
            scroll_y,
            max_scroll_y,
            has_selection,
            terminals,
            active_terminal: active_terminal_idx,
            terminal_id,
            terminal_label,
            has_launch,
            has_active_terminal: self.has_active_terminal(),
            file_total_lines,
            file_index_progress,
            file_window_start,
            file_lines_before,
            file_loading,
            max_scrollback_lines: self.config.max_scrollback_lines,
            viewport_font_size: self.renderer.font_size(),
            severity_filter,
        };
        match serde_json::to_value(&snapshot) {
            Ok(value) => self.push_event(value),
            Err(err) => self.push_event(json!({
                "type": "status",
                "message": format!("stats serialize: {err}"),
            })),
        }
    }
}
