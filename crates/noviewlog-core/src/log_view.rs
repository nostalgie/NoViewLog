//! Per-terminal filter **view** (UI/JSON name: **tab**).
//!
//! `Command::Tab*` and stats field `tabs` refer to `LogView` instances.
//! Index 0 is the Console (not filter-editable in the UI).

use std::sync::Arc;

use crate::core::buffer::RecordBuffer;
use crate::core::config::tab_config_from_runtime;
use crate::core::filter::FilterEngine;
use crate::core::types::{FilterRule, FlatLine, SearchMatch, TabConfig};
use crate::core::visible::{
    append_search_matches, collect_search_matches, compile_search_pattern, rebuild_flat_lines,
    rebuild_flat_lines_for_records, SearchPattern,
};

pub struct LogView {
    pub name: String,
    pub search_query: String,
    pub search_regex: bool,
    pub search_case_sensitive: bool,
    pub search_whole_word: bool,
    pub search_match_index: usize,
    pub search_matches: Vec<SearchMatch>,
    pub search_pattern: Option<SearchPattern>,
    pub search_error: Option<String>,
    search_dirty: bool,
    search_scroll_pending: bool,
    search_jump_to_last: bool,
    /// How many `flat_lines` were scanned for the current `search_pattern`.
    search_match_scan_end: usize,
    /// When true, next refresh must rescan from scratch (query/pattern/flat rebuild).
    search_full_rescan: bool,
    pub auto_follow: bool,
    pub wrap_lines: bool,
    pub flat_lines: Arc<Vec<FlatLine>>,
    flat_lines_dirty: bool,
    pub(crate) flat_lines_record_cursor: usize,
    filter_engine: FilterEngine,
}

impl LogView {
    pub fn from_tab_config(tab: TabConfig) -> Self {
        Self {
            name: tab.name,
            search_query: tab.search_query,
            search_regex: tab.search_regex,
            search_case_sensitive: tab.search_case_sensitive,
            search_whole_word: tab.search_whole_word,
            search_match_index: 0,
            search_matches: Vec::new(),
            search_pattern: None,
            search_error: None,
            search_dirty: true,
            search_scroll_pending: false,
            search_jump_to_last: false,
            search_match_scan_end: 0,
            search_full_rescan: true,
            auto_follow: tab.auto_follow,
            wrap_lines: tab.wrap_lines,
            flat_lines: Arc::new(Vec::new()),
            flat_lines_dirty: true,
            flat_lines_record_cursor: 0,
            filter_engine: FilterEngine::new(tab.filters),
        }
    }

    pub fn from_runtime(name: &str, filters: Vec<FilterRule>) -> Self {
        Self::from_tab_config(tab_config_from_runtime(name, filters))
    }

    pub fn filters(&self) -> &[FilterRule] {
        self.filter_engine.filters()
    }

    pub fn filters_mut(&mut self) -> &mut Vec<FilterRule> {
        self.flat_lines_dirty = true;
        self.filter_engine.filters_mut()
    }

    pub fn set_filters(&mut self, filters: Vec<FilterRule>) {
        self.filter_engine.set_filters(filters);
        self.flat_lines_dirty = true;
    }

    pub fn clear_filters(&mut self) {
        self.set_filters(Vec::new());
    }

    pub fn to_tab_config(&self) -> TabConfig {
        TabConfig {
            name: self.name.clone(),
            filters: self.filters().to_vec(),
            search_query: self.search_query.clone(),
            search_regex: self.search_regex,
            search_case_sensitive: self.search_case_sensitive,
            search_whole_word: self.search_whole_word,
            auto_follow: self.auto_follow,
            wrap_lines: self.wrap_lines,
        }
    }

    pub fn refresh_search(&mut self) -> Option<usize> {
        if self.search_query.is_empty() {
            self.search_matches.clear();
            self.search_pattern = None;
            self.search_error = None;
            self.search_match_index = 0;
            self.search_match_scan_end = 0;
            self.search_full_rescan = true;
            return None;
        }

        let pattern = if let Some(p) = self.search_pattern.clone() {
            p
        } else {
            match compile_search_pattern(
                &self.search_query,
                self.search_regex,
                self.search_case_sensitive,
                self.search_whole_word,
            ) {
                Ok(p) => {
                    self.search_error = None;
                    self.search_pattern = Some(p.clone());
                    p
                }
                Err(e) => {
                    self.search_error = Some(e);
                    self.search_matches.clear();
                    self.search_pattern = None;
                    self.search_match_index = 0;
                    self.search_match_scan_end = 0;
                    self.search_full_rescan = true;
                    return None;
                }
            }
        };

        if self.search_full_rescan {
            self.search_matches = collect_search_matches(&self.flat_lines, &pattern);
            self.search_match_scan_end = self.flat_lines.len();
            self.search_full_rescan = false;
            if self.search_jump_to_last {
                self.search_jump_to_last = false;
                self.search_match_index = self.search_matches.len().saturating_sub(1);
            } else if self.search_match_index >= self.search_matches.len() {
                self.search_match_index = self.search_matches.len().saturating_sub(1);
            }
        } else if self.search_match_scan_end < self.flat_lines.len() {
            let offset = self.search_match_scan_end;
            append_search_matches(
                &mut self.search_matches,
                &self.flat_lines[offset..],
                offset,
                &pattern,
            );
            self.search_match_scan_end = self.flat_lines.len();
            if self.search_jump_to_last {
                self.search_jump_to_last = false;
                self.search_match_index = self.search_matches.len().saturating_sub(1);
            }
        } else if self.search_jump_to_last {
            self.search_jump_to_last = false;
            self.search_match_index = self.search_matches.len().saturating_sub(1);
        }

        if self.search_scroll_pending {
            self.search_scroll_pending = false;
            return self
                .search_matches
                .get(self.search_match_index)
                .map(|m| m.line_index);
        }
        None
    }

    pub fn search_counter_label(&self) -> String {
        if self.search_query.is_empty() || self.search_error.is_some() {
            return String::new();
        }
        let total = self.search_matches.len();
        if total == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.search_match_index + 1, total)
        }
    }

    pub fn rebuild(&mut self, buffer: &RecordBuffer) -> Option<usize> {
        if self.flat_lines_dirty {
            self.flat_lines = Arc::new(rebuild_flat_lines(buffer, &self.filter_engine));
            self.flat_lines_record_cursor = buffer.records().len();
            self.flat_lines_dirty = false;
            self.search_dirty = true;
            self.search_full_rescan = true;
            self.search_match_scan_end = 0;
        } else if self.flat_lines_record_cursor < buffer.records().len() {
            let records = &buffer.records()[self.flat_lines_record_cursor..];
            let appended = rebuild_flat_lines_for_records(records, &self.filter_engine);
            if !appended.is_empty() {
                Arc::make_mut(&mut self.flat_lines).extend(appended);
                self.search_dirty = true;
            }
            self.flat_lines_record_cursor = buffer.records().len();
        }
        if self.search_dirty {
            self.search_dirty = false;
            return self.refresh_search();
        }
        None
    }

    pub fn mark_search_changed(&mut self) {
        self.search_jump_to_last = true;
        self.search_dirty = true;
        self.search_scroll_pending = true;
        self.search_full_rescan = true;
        self.search_match_scan_end = 0;
        self.search_pattern = None;
    }

    pub fn is_search_dirty(&self) -> bool {
        self.search_dirty
    }

    pub fn clear_search_dirty(&mut self) {
        self.search_dirty = false;
    }

    pub fn clear_flat_lines(&mut self) {
        self.flat_lines = Arc::new(Vec::new());
        self.flat_lines_record_cursor = 0;
        self.flat_lines_dirty = true;
        self.search_full_rescan = true;
        self.search_match_scan_end = 0;
    }

    pub fn mark_flat_lines_dirty(&mut self) {
        self.flat_lines_dirty = true;
        self.search_dirty = true;
        self.search_full_rescan = true;
        self.search_match_scan_end = 0;
    }

    #[cfg(test)]
    pub fn search_match_scan_end_for_test(&self) -> usize {
        self.search_match_scan_end
    }
}
