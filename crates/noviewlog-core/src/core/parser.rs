use chrono::Utc;

use crate::core::ansi::strip_ansi;
use crate::core::types::{detect_level, LogFormat, LogRecord};

pub struct RecordParser {
    format: LogFormat,
    pending_lines: Vec<String>,
    next_id: u64,
}

impl RecordParser {
    pub fn new(format: LogFormat) -> Self {
        Self {
            format,
            pending_lines: Vec::new(),
            next_id: 1,
        }
    }

    pub fn set_format(&mut self, format: LogFormat) {
        self.format = format;
    }

    pub fn push_line(&mut self, line: String) -> Vec<LogRecord> {
        let mut records = Vec::new();
        let plain = strip_ansi(&line);

        if self.is_start_line(&plain) {
            if !self.pending_lines.is_empty() {
                records.push(self.flush());
            }
            self.pending_lines = vec![line];
            return records;
        }

        if !self.pending_lines.is_empty() && self.is_continuation(&plain) {
            self.pending_lines.push(line);
            return records;
        }

        if !self.pending_lines.is_empty() {
            records.push(self.flush());
        }

        self.pending_lines = vec![line];
        records
    }

    pub fn flush_pending(&mut self) -> Option<LogRecord> {
        if self.pending_lines.is_empty() {
            None
        } else {
            Some(self.flush())
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.pending_lines.is_empty()
    }

    /// Replace the in-progress pending line (spinner frame) without flushing to the buffer.
    pub fn replace_last_pending_line(&mut self, line: String) {
        if let Some(last) = self.pending_lines.last_mut() {
            *last = line;
        } else {
            self.pending_lines.push(line);
        }
    }

    fn flush(&mut self) -> LogRecord {
        let lines = std::mem::take(&mut self.pending_lines);
        let text = lines
            .iter()
            .map(|l| strip_ansi(l))
            .collect::<Vec<_>>()
            .join("\n");
        let level = detect_level(&text);
        let record = LogRecord {
            id: self.next_id,
            lines,
            text,
            received_at: Utc::now(),
            level,
            overwrite: false,
        };
        self.next_id += 1;
        record
    }

    fn is_start_line(&self, plain: &str) -> bool {
        if self.format.id == "raw" {
            return true;
        }
        self.format
            .start_regex
            .as_ref()
            .is_some_and(|re| re.is_match(plain))
    }

    fn is_continuation(&self, plain: &str) -> bool {
        if self.format.continuation_regexes.is_empty() {
            return false;
        }
        self.format
            .continuation_regexes
            .iter()
            .any(|re| re.is_match(plain))
    }
}

pub fn reparse_lines(lines: &[String], format: LogFormat) -> Vec<LogRecord> {
    let mut parser = RecordParser::new(format);
    let mut records = Vec::new();
    for line in lines {
        records.extend(parser.push_line(line.clone()));
    }
    if let Some(last) = parser.flush_pending() {
        records.push(last);
    }
    records
}
