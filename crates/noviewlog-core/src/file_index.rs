use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

/// Bytes scanned per engine tick while building a line index.
pub const INDEX_BYTES_PER_TICK: u64 = 512 * 1024;

/// Raw file lines kept in the in-memory sliding window.
pub const WINDOW_RAW_LINES: usize = 50_000;

/// Raw lines loaded when the user scrolls near a window edge.
pub const PREFETCH_RAW_LINES: usize = 10_000;

/// Byte offsets of each line start (line 0 begins at offsets[0]).
#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    offsets: Vec<u64>,
    file_size: u64,
}

impl LineIndex {
    pub fn new(file_size: u64) -> Self {
        Self {
            offsets: Vec::new(),
            file_size,
        }
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    pub fn total_lines(&self) -> u64 {
        self.offsets.len() as u64
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

    /// Line number whose byte offset is `<= byte` (last line starting at or before `byte`).
    pub fn line_at_offset(&self, byte: u64) -> u64 {
        match self.offsets.binary_search(&byte) {
            Ok(i) => i as u64,
            Err(i) => i.saturating_sub(1) as u64,
        }
    }

    pub fn offset_of(&self, line: u64) -> Option<u64> {
        self.offsets.get(line as usize).copied()
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

        if from_byte == 0 {
            self.offsets.push(0);
        }

        let end_byte = (from_byte + max_bytes).min(self.file_size);
        let mut buf = vec![0u8; (end_byte - from_byte) as usize];
        file.read_exact(&mut buf)
            .map_err(|e| format!("Read failed: {e}"))?;

        let mut pos = from_byte;
        for (i, &b) in buf.iter().enumerate() {
            if b == b'\n' {
                let next = from_byte + i as u64 + 1;
                if next < self.file_size {
                    self.offsets.push(next);
                }
                pos = next;
            }
        }
        let _ = pos;

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
            .offset_of(start_line)
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

        let lines = index.read_lines(&mut file, 10, 3).unwrap();
        assert_eq!(lines, vec!["line 000010", "line 000011", "line 000012"]);

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
        assert!(line > 0 && line < 100);
        let _ = std::fs::remove_file(path);
    }
}
