//! Typed engine → host events (stats / status / exit).
//!
//! Wire format stays flat JSON (`{"type":"stats", ...}`). Prefer
//! [`parse_engine_event`] over deserializing [`EngineEvent`] directly — stats
//! is not nested under a `stats` key.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::types::FilterRule;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsTab {
    pub index: usize,
    pub name: String,
    #[serde(default)]
    pub is_console: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsTerminal {
    pub index: usize,
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub cwd: String,
}

/// Flat `{"type":"stats", ...}` snapshot emitted by [`super::Engine::emit_stats`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSnapshot {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub lines: usize,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub format_id: String,
    #[serde(default)]
    pub preset_name: String,
    #[serde(default = "default_true")]
    pub auto_follow: bool,
    #[serde(default)]
    pub tab_name: String,
    #[serde(default)]
    pub active_tab: usize,
    #[serde(default)]
    pub tab_count: usize,
    #[serde(default)]
    pub console_tab: usize,
    #[serde(default)]
    pub is_console_tab: bool,
    #[serde(default)]
    pub tabs: Vec<StatsTab>,
    #[serde(default)]
    pub dropped: usize,
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub presets: Vec<String>,
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    #[serde(default)]
    pub search_query: String,
    #[serde(default)]
    pub search_regex: bool,
    #[serde(default)]
    pub search_case_sensitive: bool,
    #[serde(default)]
    pub search_whole_word: bool,
    #[serde(default)]
    pub search_counter: String,
    #[serde(default)]
    pub search_error: Option<String>,
    #[serde(default)]
    pub search_has_matches: bool,
    #[serde(default)]
    pub can_restore_closed_tab: bool,
    #[serde(default = "default_true")]
    pub wrap_lines: bool,
    #[serde(default)]
    pub scroll_x: f32,
    #[serde(default)]
    pub max_scroll_x: f32,
    #[serde(default)]
    pub scroll_y: f32,
    #[serde(default)]
    pub max_scroll_y: f32,
    #[serde(default)]
    pub has_selection: bool,
    #[serde(default)]
    pub terminals: Vec<StatsTerminal>,
    #[serde(default)]
    pub active_terminal: usize,
    #[serde(default)]
    pub terminal_id: String,
    #[serde(default)]
    pub terminal_label: String,
    #[serde(default)]
    pub has_launch: bool,
    #[serde(default)]
    pub has_active_terminal: bool,
    #[serde(default)]
    pub file_total_lines: u64,
    #[serde(default)]
    pub file_index_progress: f32,
    #[serde(default)]
    pub file_window_start: u64,
    #[serde(default)]
    pub file_lines_before: u64,
    #[serde(default)]
    pub file_loading: bool,
    #[serde(default)]
    pub max_scrollback_lines: usize,
    #[serde(default)]
    pub viewport_font_size: f32,
}

fn default_true() -> bool {
    true
}

/// Engine → host event after parsing wire JSON.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Stats(StatsSnapshot),
    Status { message: String },
    Exit { code: i32, message: String },
    Unknown,
}

/// Parse a wire event JSON string into a typed [`EngineEvent`].
///
/// Stats objects are flat (`type` + fields), so this dispatches on `type`
/// rather than relying on a nested tagged-enum shape.
pub fn parse_engine_event(json: &str) -> Option<EngineEvent> {
    let v: Value = serde_json::from_str(json).ok()?;
    match v.get("type")?.as_str()? {
        "stats" => serde_json::from_value(v).ok().map(EngineEvent::Stats),
        "status" => {
            let message = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            Some(EngineEvent::Status { message })
        }
        "exit" => {
            let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;
            let message = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            Some(EngineEvent::Exit { code, message })
        }
        _ => Some(EngineEvent::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::FilterType;

    const GOLDEN_STATS: &str = r#"{
        "type": "stats",
        "lines": 42,
        "running": true,
        "status": "ok",
        "exit_code": null,
        "format_id": "raw",
        "preset_name": "default",
        "auto_follow": true,
        "tab_name": "Errors",
        "active_tab": 1,
        "tab_count": 2,
        "console_tab": 0,
        "is_console_tab": false,
        "tabs": [
            {"index": 0, "name": "Console", "is_console": true},
            {"index": 1, "name": "Errors", "is_console": false}
        ],
        "dropped": 0,
        "formats": ["raw"],
        "presets": ["default"],
        "filters": [
            {
                "id": "f1",
                "type": "include",
                "pattern": "error",
                "enabled": true,
                "use_regex": false
            }
        ],
        "search_query": "boom",
        "search_regex": false,
        "search_case_sensitive": false,
        "search_whole_word": false,
        "search_counter": "1/3",
        "search_error": null,
        "search_has_matches": true,
        "can_restore_closed_tab": false,
        "wrap_lines": true,
        "scroll_x": 0.0,
        "max_scroll_x": 0.0,
        "scroll_y": 120.5,
        "max_scroll_y": 500.0,
        "has_selection": false,
        "terminals": [
            {"index": 0, "id": "t0", "label": ".", "running": true, "cwd": "/tmp"}
        ],
        "active_terminal": 0,
        "terminal_id": "t0",
        "terminal_label": ".",
        "has_launch": false,
        "has_active_terminal": true,
        "file_total_lines": 0,
        "file_index_progress": 0.0,
        "file_window_start": 0,
        "file_lines_before": 0,
        "file_loading": false,
        "max_scrollback_lines": 10000,
        "viewport_font_size": 13.0
    }"#;

    #[test]
    fn parse_stats_golden_fields() {
        let event = parse_engine_event(GOLDEN_STATS).expect("parse");
        let EngineEvent::Stats(s) = event else {
            panic!("expected Stats");
        };
        assert_eq!(s.event_type, "stats");
        assert_eq!(s.lines, 42);
        assert!(s.auto_follow);
        assert!(s.wrap_lines);
        assert_eq!(s.active_tab, 1);
        assert!((s.scroll_y - 120.5).abs() < f32::EPSILON);
        assert_eq!(s.search_counter, "1/3");
        assert_eq!(s.max_scrollback_lines, 10_000);
        assert_eq!(s.tabs.len(), 2);
        assert_eq!(s.tabs[0].name, "Console");
        assert!(s.tabs[0].is_console);
        assert_eq!(s.terminals.len(), 1);
        assert_eq!(s.terminals[0].id, "t0");
        assert_eq!(s.filters.len(), 1);
        assert_eq!(s.filters[0].pattern, "error");
        assert_eq!(s.filters[0].filter_type, FilterType::Include);
        assert!(!s.filters[0].use_regex);
    }

    #[test]
    fn stats_snapshot_roundtrip() {
        let snap: StatsSnapshot = serde_json::from_str(GOLDEN_STATS).expect("from_str");
        let encoded = serde_json::to_string(&snap).expect("to_string");
        let again: StatsSnapshot = serde_json::from_str(&encoded).expect("re-parse");
        assert_eq!(again.tabs.len(), snap.tabs.len());
        assert_eq!(again.terminals.len(), snap.terminals.len());
        assert_eq!(again.filters.len(), snap.filters.len());
        assert_eq!(again.scroll_y, snap.scroll_y);
        assert_eq!(again.auto_follow, snap.auto_follow);
        assert_eq!(again.wrap_lines, snap.wrap_lines);
        assert_eq!(again.active_tab, snap.active_tab);
        assert_eq!(again.search_counter, snap.search_counter);
        assert_eq!(again.max_scrollback_lines, snap.max_scrollback_lines);

        let event = parse_engine_event(&encoded).expect("parse encoded");
        assert!(matches!(event, EngineEvent::Stats(_)));
    }

    #[test]
    fn parse_status_and_exit() {
        let status = parse_engine_event(r#"{"type":"status","message":"hi"}"#).unwrap();
        match status {
            EngineEvent::Status { message } => assert_eq!(message, "hi"),
            _ => panic!("expected Status"),
        }
        let exit = parse_engine_event(r#"{"type":"exit","code":7,"message":"done"}"#).unwrap();
        match exit {
            EngineEvent::Exit { code, message } => {
                assert_eq!(code, 7);
                assert_eq!(message, "done");
            }
            _ => panic!("expected Exit"),
        }
    }
}
