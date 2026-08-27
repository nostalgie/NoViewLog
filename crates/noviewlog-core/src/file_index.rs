use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

/// Bytes scanned per engine tick while building a line index.
pub const INDEX_BYTES_PER_TICK: u64 = 4 * 1024 * 1024;

/// Raw file lines kept in the in-memory sliding window.
pub const WINDOW_RAW_LINES: usize = 50_000;

/// Raw lines loaded when the user scrolls near a window edge.
pub const PREFETCH_RAW_LINES: usize = 5_000;

/// Store a checkpoint every N lines (plus line 0). Keeps RAM ~O(lines/stride).
pub const LINE_INDEX_STRIDE: u64 = 256;

/// Sparse line index: checkpoints + on-demand walk within a stride.
#[derive(Clone, Debug)]
pub struct LineIndex {
    /// `(line_number, byte_offset)` sorted by line number.
    checkpoints: Vec<(u64, u64)>,
    line_count: u64,
    file_size: u64,
    stride: u64,
}

impl Default for LineIndex {
    fn default() -> Self {
        Self::new(0)
    }
}

impl LineIndex {
    pub fn new(file_size: u64) -> Self {
        Self {
            checkpoints: Vec::new(),
            line_count: 0,
            file_size,
            stride: LINE_INDEX_STRIDE.max(1),
        }
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    pub fn total_lines(&self) -> u64 {
        self.line_count
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    pub fn is_complete(&self, bytes_indexed: u64) -> bool {
        bytes_indexed >= self.file_size
    }

    pub fn progress(&self, bytes_indexed: u64) -> f32 {
        if self.file_size == 0 {
            1.0
        } else {
            (bytes_indexed as f32 / self.file_size as f32).clamp(0.0, 1.0)
        }
    }

    fn push_checkpoint(&mut self, line: u64, offset: u64) {
        if let Some(&(last_line, _)) = self.checkpoints.last() {
            if line <= last_line {
                return;
            }
        }
        self.checkpoints.push((line, offset));
    }

    fn should_checkpoint(&self, line: u64) -> bool {
        line == 0 || line % self.stride == 0
    }

    /// Approximate line whose start is `<= byte` (checkpoint interpolation).
    pub fn line_at_offset(&self, byte: u64) -> u64 {
        if self.line_count == 0 || self.checkpoints.is_empty() {
            return 0;
        }
        let mut lo = 0usize;
        let mut hi = self.checkpoints.len();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.checkpoints[mid].1 <= byte {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (line, off) = self.checkpoints[lo];
        if off >= byte {
            return line.min(self.line_count.saturating_sub(1));
        }
        if lo + 1 < self.checkpoints.len() {
            let (next_line, next_off) = self.checkpoints[lo + 1];
            if next_off <= byte {
                return next_line.min(self.line_count.saturating_sub(1));
            }
            if next_off > off && next_line > line {
                let span = next_off - off;
                let lines = next_line - line;
                let progress = ((byte - off) as f64 / span as f64).clamp(0.0, 1.0);
                return (line + (progress * lines as f64) as u64)
                    .min(self.line_count.saturating_sub(1));
            }
        }
        line.min(self.line_count.saturating_sub(1))
    }

    /// Exact byte offset of `line`, walking from the nearest checkpoint at or before it.
    pub fn offset_of_exact(&self, file: &mut File, line: u64) -> Result<Option<u64>, String> {
        if line >= self.line_count {
            return Ok(None);
        }
        let Some(&(cp_line, cp_off)) = self
            .checkpoints
            .iter()
            .rev()
            .find(|(l, _)| *l <= line)
        else {
            return Ok(None);
        };
        if cp_line == line {
            return Ok(Some(cp_off));
        }
        file.seek(SeekFrom::Start(cp_off))
            .map_err(|e| format!("Seek failed: {e}"))?;
        let mut reader = BufReader::new(file);
        let mut cur = cp_line;
        let mut pos = cp_off;
        while cur < line {
            let mut buf = String::new();
            match reader.read_line(&mut buf) {
                Ok(0) => return Ok(None),
                Ok(n) => {
                    pos += n as u64;
                    cur += 1;
                }
                Err(err) => return Err(format!("Read error: {err}")),
            }
        }
        Ok(Some(pos))
    }

    /// Checkpoint hit only (no file walk). Prefer [`Self::offset_of_exact`] for reads.
    pub fn offset_of(&self, line: u64) -> Option<u64> {
        if line >= self.line_count {
            return None;
        }
        self.checkpoints
            .iter()
            .rev()
            .find(|(l, _)| *l <= line)
            .and_then(|(l, o)| if *l == line { Some(*o) } else { None })
    }

    pub fn scan_chunk(
        &mut self,
        file: &mut File,
        from_byte: u64,
        max_bytes: u64,
    ) -> Result<(u64, bool), String> {
        if from_byte >= self.file_size {
            return Ok((from_byte, true));
        }

        file.seek(SeekFrom::Start(from_byte))
            .map_err(|e| format!("Seek failed: {e}"))?;

        if from_byte == 0 && self.checkpoints.is_empty() {
            self.push_checkpoint(0, 0);
            self.line_count = 1;
        }

        let end_byte = (from_byte + max_bytes).min(self.file_size);
        let mut buf = vec![0u8; (end_byte - from_byte) as usize];
        file.read_exact(&mut buf)
            .map_err(|e| format!("Read failed: {e}"))?;

        for (i, &b) in buf.iter().enumerate() {
            if b != b'\n' {
                continue;
            }
            let next = from_byte + i as u64 + 1;
            if next >= self.file_size {
                continue;
            }
            // New line starts at `next`.
            let new_line = self.line_count;
            self.line_count += 1;
            if self.should_checkpoint(new_line) {
                self.push_checkpoint(new_line, next);
            }
        }

        let done = end_byte >= self.file_size;
        Ok((end_byte, done))
    }

    pub fn read_lines(
        &self,
        file: &mut File,
        start_line: u64,
        count: usize,
    ) -> Result<Vec<String>, String> {
        if count == 0 || start_line >= self.total_lines() {
            return Ok(Vec::new());
        }

        let start = self
            .offset_of_exact(file, start_line)?
            .ok_or_else(|| format!("Line {start_line} not in index"))?;
        file.seek(SeekFrom::Start(start))
            .map_err(|e| format!("Seek failed: {e}"))?;

        let mut reader = BufReader::new(file);
        let mut out = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            let line_index = start_line + out.len() as u64;
            if line_index >= self.total_lines() {
                break;
            }
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }
                    out.push(line);
                }
                Err(err) => return Err(format!("Read error: {err}")),
            }
        }
        Ok(out)
    }
}

/// Retained after load: path, index, and a file handle for on-demand reads.
pub struct FileBackedLog {
    pub path: String,
    pub file: File,
    pub index: LineIndex,
}

impl FileBackedLog {
    pub fn read_lines(&mut self, start_line: u64, count: usize) -> Result<Vec<String>, String> {
        self.index.read_lines(&mut self.file, start_line, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_lines(path: &std::path::Path, n: usize) {
        let mut f = std::fs::File::create(path).unwrap();
        for i in 0..n {
            writeln!(f, "line {i:06}").unwrap();
        }
    }

    #[test]
    fn index_and_read_range() {
        let path = std::env::temp_dir().join(format!("noviewlog-idx-{}", std::process::id()));
        write_lines(&path, 500);
        let size = std::fs::metadata(&path).unwrap().len();
        let mut file = File::open(&path).unwrap();
        let mut index = LineIndex::new(size);
        let (end, done) = index.scan_chunk(&mut file, 0, size).unwrap();
        assert!(done);
        assert_eq!(end, size);
        assert_eq!(index.total_lines(), 500);
        assert!(index.checkpoint_count() < 500);
        assert!(index.checkpoint_count() >= 2);

        let lines = index.read_lines(&mut file, 10, 3).unwrap();
        assert_eq!(lines, vec!["line 000010", "line 000011", "line 000012"]);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sparse_index_scales_checkpoints() {
        let path = std::env::temp_dir().join(format!("noviewlog-sparse-{}", std::process::id()));
        write_lines(&path, 10_000);
        let size = std::fs::metadata(&path).unwrap().len();
        let mut file = File::open(&path).unwrap();
        let mut index = LineIndex::new(size);
        index.scan_chunk(&mut file, 0, size).unwrap();
        assert_eq!(index.total_lines(), 10_000);
        let expected_cps = 1 + (10_000 - 1) / LINE_INDEX_STRIDE as usize;
        assert!(
            index.checkpoint_count() <= expected_cps + 2,
            "checkpoints={} expected~{}",
            index.checkpoint_count(),
            expected_cps
        );
        let lines = index.read_lines(&mut file, 9_000, 2).unwrap();
        assert_eq!(lines[0], "line 009000");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn line_at_offset_finds_tail_start() {
        let path = std::env::temp_dir().join(format!("noviewlog-idx2-{}", std::process::id()));
        write_lines(&path, 100);
        let size = std::fs::metadata(&path).unwrap().len();
        let tail_byte = size / 2;
        let mut file = File::open(&path).unwrap();
        let mut index = LineIndex::new(size);
        index.scan_chunk(&mut file, 0, size).unwrap();
        let line = index.line_at_offset(tail_byte);
        assert!(line < 100);
        let _ = std::fs::remove_file(path);
    }
}
