use crate::core::types::LogRecord;

pub struct RecordBuffer {
    records: Vec<LogRecord>,
    raw_lines: Vec<String>,
    dropped: usize,
    max_records: usize,
}

impl RecordBuffer {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Vec::new(),
            raw_lines: Vec::new(),
            dropped: 0,
            max_records,
        }
    }

    /// Drop oldest records (and their raw lines) when over `max_records`.
    /// Returns the number of raw lines removed from the front.
    fn trim_overflow(&mut self) -> usize {
        if self.records.len() <= self.max_records {
            return 0;
        }
        let overflow = self.records.len() - self.max_records;
        let shifted_lines: usize = self.records[..overflow]
            .iter()
            .map(|r| r.lines.len())
            .sum();
        self.records.drain(0..overflow);
        self.raw_lines.drain(0..shifted_lines);
        self.dropped += overflow;
        shifted_lines
    }

    /// Returns the number of raw lines dropped from the front when the cap is exceeded.
    pub fn add(&mut self, record: LogRecord) -> usize {
        self.raw_lines.extend(record.lines.iter().cloned());
        self.records.push(record);
        self.trim_overflow()
    }

    pub fn records(&self) -> &[LogRecord] {
        &self.records
    }

    pub fn raw_lines(&self) -> &[String] {
        &self.raw_lines
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped
    }

    pub fn max_records(&self) -> usize {
        self.max_records
    }

    /// Update the retention cap and drop oldest records if over the new limit.
    /// Returns the number of raw lines removed from the front.
    pub fn set_max_records(&mut self, max_records: usize) -> usize {
        self.max_records = max_records.max(1);
        self.trim_overflow()
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.raw_lines.clear();
        self.dropped = 0;
    }

    /// Replace all records (file window swap). Resets dropped count.
    pub fn replace_all(&mut self, records: Vec<LogRecord>) {
        self.records.clear();
        self.raw_lines.clear();
        self.dropped = 0;
        for record in records {
            self.raw_lines.extend(record.lines.iter().cloned());
            self.records.push(record);
        }
        self.trim_overflow();
    }

    /// Remove the last `n` records (and their raw lines). Used to strip the
    /// volatile terminal tail before re-committing / re-rendering it.
    pub fn pop_last(&mut self, n: usize) {
        for _ in 0..n {
            let Some(rec) = self.records.pop() else {
                break;
            };
            let line_count = rec.lines.len();
            let start = self.raw_lines.len().saturating_sub(line_count);
            self.raw_lines.truncate(start);
        }
    }

    pub fn last_is_overwrite_single_line(&self) -> bool {
        self.records
            .last()
            .is_some_and(|r| r.lines.len() == 1 && r.overwrite)
    }

    pub fn set_last_overwrite(&mut self, overwrite: bool) {
        if let Some(last) = self.records.last_mut() {
            last.overwrite = overwrite;
        }
    }

    /// Replace the last single-line overwrite record (spinner / progress).
    /// Returns true if a record was replaced.
    pub fn replace_last_single_line(&mut self, record: LogRecord) -> bool {
        if self.last_is_overwrite_single_line() {
            let removed = self.records.pop().expect("last exists");
            let line_count = removed.lines.len();
            let start = self.raw_lines.len().saturating_sub(line_count);
            self.raw_lines.truncate(start);
            self.add(record);
            return true;
        }
        self.add(record);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::LogLevel;
    use chrono::Utc;

    fn rec(id: u64, text: &str) -> LogRecord {
        LogRecord {
            id,
            lines: vec![text.to_string()],
            text: text.to_string(),
            received_at: Utc::now(),
            level: None::<LogLevel>,
            overwrite: false,
        }
    }

    #[test]
    fn add_past_max_records_drops_oldest() {
        let mut buf = RecordBuffer::new(3);
        assert_eq!(buf.add(rec(1, "a")), 0);
        assert_eq!(buf.add(rec(2, "b")), 0);
        assert_eq!(buf.add(rec(3, "c")), 0);
        assert_eq!(buf.records().len(), 3);
        assert_eq!(buf.dropped_count(), 0);

        let shifted = buf.add(rec(4, "d"));
        assert_eq!(shifted, 1);
        assert_eq!(buf.records().len(), 3);
        assert_eq!(buf.dropped_count(), 1);
        assert_eq!(
            buf.records()
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(buf.raw_lines(), &["b".to_string(), "c".to_string(), "d".to_string()]);
    }
}
