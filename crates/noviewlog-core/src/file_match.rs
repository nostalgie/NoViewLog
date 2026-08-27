//! Whole-file match index for file-session filter tabs.
//!
//! Scans the source file for include/exclude (+ severity) hits and stores
//! matching line **byte offsets**. The viewport seeks those offsets instead of
//! copying the filtered text into a second log file.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

use crate::core::filter::FilterEngine;
use crate::core::types::{LogRecord, SeverityFilter};

/// Bytes scanned per engine tick while building a match index.
pub const MATCH_SCAN_BYTES_PER_TICK: u64 = 512 * 1024;

/// How many match lines to keep materialized for the active filter tab.
pub const MATCH_WINDOW_LINES: usize = 10_000;

/// Returns true when a file filter tab should use a whole-file match index
/// instead of the shared sliding window buffer.
pub fn view_needs_match_index(filters: &FilterEngine, severity: SeverityFilter) -> bool {
    let has_rules = filters.filters().iter().any(|f| f.enabled);
    has_rules || severity != SeverityFilter::All
}

fn line_record(text: &str) -> LogRecord {
    LogRecord {
        id: 0,
        lines: vec![text.to_string()],
        text: text.to_string(),
        received_at: chrono::Utc::now(),
        level: None,
        overwrite: false,
    }
}

/// Scan up to `max_bytes` from `from_byte`, appending matching line start offsets.
/// Returns the next byte position to continue from (and whether the file is done).
pub fn scan_match_chunk(
    file: &mut File,
    file_size: u64,
    from_byte: u64,
    max_bytes: u64,
    filters: &FilterEngine,
    severity: SeverityFilter,
    offsets: &mut Vec<u64>,
) -> Result<(u64, bool), String> {
    if from_byte >= file_size {
        return Ok((from_byte, true));
    }
    file.seek(SeekFrom::Start(from_byte))
        .map_err(|e| format!("Seek failed: {e}"))?;

    let end_byte = (from_byte + max_bytes).min(file_size);
    let mut reader = BufReader::new(file);
    let mut pos = from_byte;
    while pos < end_byte {
        let line_start = pos;
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(n) => {
                pos += n as u64;
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                let record = line_record(&line);
                if severity.allows(record.level) && filters.is_visible(&record) {
                    offsets.push(line_start);
                }
                if pos >= end_byte {
                    break;
                }
            }
            Err(err) => return Err(format!("Read error: {err}")),
        }
    }
    let done = pos >= file_size;
    Ok((pos, done))
}

/// Read one line starting at `offset` (does not require a full line index).
pub fn read_line_at(file: &mut File, offset: u64) -> Result<String, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Seek failed: {e}"))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => Ok(String::new()),
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Ok(line)
        }
        Err(err) => Err(format!("Read error: {err}")),
    }
}

/// Materialize a window of match lines as plain strings.
pub fn read_match_window(
    file: &mut File,
    offsets: &[u64],
    start: usize,
    count: usize,
) -> Result<Vec<String>, String> {
    if start >= offsets.len() || count == 0 {
        return Ok(Vec::new());
    }
    let end = (start + count).min(offsets.len());
    let mut out = Vec::with_capacity(end - start);
    for &off in &offsets[start..end] {
        out.push(read_line_at(file, off)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{compile_filter, FilterRule, FilterType};
    use std::io::Write;

    #[test]
    fn scan_finds_include_matches() {
        let path = std::env::temp_dir().join(format!(
            "noviewlog-match-{}",
            std::process::id()
        ));
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "alpha").unwrap();
            writeln!(f, "error boom").unwrap();
            writeln!(f, "beta").unwrap();
            writeln!(f, "error again").unwrap();
        }
        let size = std::fs::metadata(&path).unwrap().len();
        let mut file = File::open(&path).unwrap();
        let rule = compile_filter(FilterRule {
            id: "1".into(),
            name: None,
            filter_type: FilterType::Include,
            pattern: "error".into(),
            enabled: true,
            use_regex: false,
            regex: None,
        });
        let engine = FilterEngine::new(vec![rule]);
        let mut offsets = Vec::new();
        let (pos, done) = scan_match_chunk(
            &mut file,
            size,
            0,
            size,
            &engine,
            SeverityFilter::All,
            &mut offsets,
        )
        .unwrap();
        assert!(done);
        assert_eq!(pos, size);
        assert_eq!(offsets.len(), 2);
        let lines = read_match_window(&mut file, &offsets, 0, 10).unwrap();
        assert_eq!(lines, vec!["error boom", "error again"]);
        let _ = std::fs::remove_file(path);
    }
}
