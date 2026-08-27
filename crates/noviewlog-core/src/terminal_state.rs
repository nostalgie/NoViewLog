//! Per-terminal session state (`TerminalState`).
//!
//! A **terminal** is an independent session (PTY shell, process, or read-only
//! log file). Inside it, `views` holds the Terminal tab plus filter tabs (`LogView`).
//! JSON/UI still call those tabs via `tab_*` commands.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Instant;

use crate::core::buffer::RecordBuffer;
use crate::core::config::RuntimeConfig;
use crate::core::parser::RecordParser;
use crate::core::terminal::TerminalIngest;
use crate::core::types::{LaunchConfig, LogFormat, TabConfig};
use crate::file_index::FileBackedLog;
use crate::file_load::FileLoadState;
use crate::log_view::{LogView, TERMINAL_TAB_NAME};
use crate::viewport_layout::TextSelection;

pub const MAX_CLOSED_TABS: usize = 15;

/// Chunked swap of the in-memory file window while scrolling giant logs.
pub struct PendingFileWindow {
    pub new_start: u64,
    pub scroll_y: f32,
    pub next_line: u64,
    pub end_line: u64,
    pub lines: Vec<String>,
}

/// Runtime state for one terminal session (shell/process + Terminal/filter tabs).
pub struct TerminalState {
    pub id: String,
    /// Live working directory (spawn cwd, updated via OSC 7 when available).
    pub cwd: String,
    /// Optional sidebar title override; when `Some(non-empty)`, preferred by [`Self::label`].
    pub custom_title: Option<String>,
    pub launch: LaunchConfig,
    pub views: Vec<LogView>,
    pub active_view: usize,
    pub closed_tabs: VecDeque<TabConfig>,
    pub buffer: RecordBuffer,
    pub parser: RecordParser,
    pub ingest: TerminalIngest,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub process_started: bool,
    /// Monotonic PTY session token for this terminal. Compared to [`crate::pty::PtyEvent::Exit`].
    pub pty_generation: u64,
    pub last_line_at: Option<Instant>,
    pub scroll_offset_y: f32,
    pub scroll_x: f32,
    pub selection: Option<TextSelection>,
    pub scroll_to_row: Option<usize>,
    pub file_load: Option<FileLoadState>,
    /// On-demand reads after a file load completes.
    pub file_backed: Option<FileBackedLog>,
    /// First raw file line number currently held in `buffer`.
    pub buffer_line_start: u64,
    /// One past the last raw file line in `buffer`.
    pub buffer_line_end: u64,
    pub pending_file_window: Option<PendingFileWindow>,
}

impl TerminalState {
    pub fn new(
        id: String,
        launch: LaunchConfig,
        runtime: &RuntimeConfig,
        format: &LogFormat,
        max_records: usize,
    ) -> Self {
        let cwd = resolve_initial_cwd(&launch);
        let _ = runtime;
        Self {
            id,
            cwd,
            custom_title: None,
            launch,
            views: vec![LogView::from_runtime(TERMINAL_TAB_NAME, Vec::new())],
            active_view: 0,
            closed_tabs: VecDeque::new(),
            buffer: RecordBuffer::new(max_records),
            parser: RecordParser::new(format.clone()),
            ingest: TerminalIngest::new(),
            running: false,
            exit_code: None,
            process_started: false,
            pty_generation: 0,
            last_line_at: None,
            scroll_offset_y: 0.0,
            scroll_x: 0.0,
            selection: None,
            scroll_to_row: None,
            file_load: None,
            file_backed: None,
            buffer_line_start: 0,
            buffer_line_end: 0,
            pending_file_window: None,
        }
    }

    /// Sidebar / tab label: custom title when set; else file basename or cwd segment.
    pub fn label(&self) -> String {
        if let Some(title) = self.custom_title.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            return title.to_string();
        }
        if let Some(path) = self.file_session_path() {
            return Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(path)
                .to_string();
        }
        cwd_label(&self.cwd)
    }

    /// Path for an open / loading log file, if this terminal is a file session.
    pub fn file_session_path(&self) -> Option<&str> {
        self.file_backed
            .as_ref()
            .map(|b| b.path.as_str())
            .or(self.launch.log_file.as_deref())
            .or_else(|| self.file_load.as_ref().map(|f| f.path.as_str()))
    }

    /// View-only log file terminal (no shell / no stdin).
    pub fn is_file_session(&self) -> bool {
        self.file_load.is_some()
            || self.file_backed.is_some()
            || self.launch.log_file.is_some()
    }

    /// Basename for the pinned primary tab when this is a file session.
    pub fn primary_tab_name(&self) -> String {
        if let Some(path) = self.file_session_path() {
            return Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(path)
                .to_string();
        }
        TERMINAL_TAB_NAME.to_string()
    }

    /// Keep index-0 tab named for the session kind; clear filters on that tab.
    pub fn sync_primary_tab_identity(&mut self) {
        let name = self.primary_tab_name();
        let is_file = self.is_file_session();
        if let Some(tab) = self.views.first_mut() {
            if tab.name != name {
                tab.name = name;
            }
            if !tab.filters().is_empty() {
                tab.clear_filters();
            }
            if is_file {
                tab.auto_follow = false;
            }
        }
    }

    /// Disable Follow on every view (file sessions).
    pub fn disable_follow_all_views(&mut self) {
        for view in &mut self.views {
            view.auto_follow = false;
        }
    }

    pub fn active_view(&self) -> &LogView {
        let idx = self.active_view.min(self.views.len().saturating_sub(1));
        &self.views[idx]
    }

    pub fn active_view_mut(&mut self) -> &mut LogView {
        let idx = self.active_view.min(self.views.len().saturating_sub(1));
        &mut self.views[idx]
    }

    pub fn reset_viewport(&mut self) {
        self.scroll_offset_y = 0.0;
        self.scroll_x = 0.0;
        self.selection = None;
        self.scroll_to_row = None;
        self.pending_file_window = None;
    }

    pub fn ensure_terminal_tab_view(&mut self, _runtime: &RuntimeConfig) {
        if self.views.is_empty() {
            let name = self.primary_tab_name();
            self.views = vec![LogView::from_runtime(&name, Vec::new())];
            self.active_view = 0;
            if self.is_file_session() {
                self.disable_follow_all_views();
            }
            return;
        }
        self.sync_primary_tab_identity();
        self.active_view = self.active_view.min(self.views.len() - 1);
    }
}

pub fn next_terminal_id(existing: &[TerminalState]) -> String {
    format!(
        "terminal-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(existing.len() as u128)
    )
}

pub fn resolve_initial_cwd(launch: &LaunchConfig) -> String {
    if let Some(cwd) = launch.cwd.as_deref().filter(|s| !s.is_empty()) {
        return cwd.to_string();
    }
    std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_else(|| ".".to_string())
}

pub fn cwd_label(cwd: &str) -> String {
    let trimmed = cwd.trim();
    if trimmed.is_empty() || trimmed == "." {
        return ".".to_string();
    }
    if let Ok(home) = std::env::var("HOME") {
        if trimmed == home || trimmed == format!("{home}/") {
            return "~".to_string();
        }
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}
