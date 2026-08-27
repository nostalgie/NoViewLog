use crate::core::types::{LogLevel, LogRecord};
use chrono::Utc;
use std::sync::Mutex;

/// User config.yaml is process-global; serialize tests that write it.
pub(crate) static USER_CONFIG_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn sample_records() -> Vec<LogRecord> {
    vec![
        LogRecord {
            id: 1,
            lines: vec!["warn: deprecated".to_string()],
            text: "warn: deprecated".to_string(),
            received_at: Utc::now(),
            level: Some(LogLevel::Warn),
            overwrite: false,
        },
        LogRecord {
            id: 2,
            lines: vec!["Error: boom".to_string()],
            text: "Error: boom".to_string(),
            received_at: Utc::now(),
            level: Some(LogLevel::Error),
            overwrite: false,
        },
        LogRecord {
            id: 3,
            lines: vec!["info: ok".to_string()],
            text: "info: ok".to_string(),
            received_at: Utc::now(),
            level: Some(LogLevel::Info),
            overwrite: false,
        },
    ]
}

mod parser_filters;
mod tabs_search;
mod terminals_files;
mod viewport_wrap;
mod volatile_patch;
mod terminal_caret;
