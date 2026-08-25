use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

use crate::file_index::{FileBackedLog, LineIndex, INDEX_BYTES_PER_TICK};

/// Files above this size show the tail immediately while the full line index
/// is built in the background.
pub const FILE_LARGE_BYTES: u64 = 8 * 1024 * 1024;

/// Initial tail bytes read for immediate display on large files.
pub const FILE_INITIAL_TAIL_BYTES: u64 = 8 * 1024 * 1024;

/// Lines ingested per engine tick while file content is loading.
pub const FILE_LOAD_LINES_PER_TICK: usize = 800;

pub struct FileLoadState {
    pub path: String,
    pub file_size: u64,
    /// Content reader (tail-first for large files, start-to-end for small).
    content_reader: Option<BufReader<File>>,
    pub content_lines_read: u64,
    pub content_finished: bool,
    /// Byte offset where the content reader started (for window placement).
    pub content_start_byte: u64,
    /// Background index construction.
    index_file: Option<File>,
    pub index: LineIndex,
    pub index_bytes_done: u64,
    pub index_finished: bool,
}

impl FileLoadState {
    pub fn open(path: &str) -> Result<Self, String> {
        let path = crate::core::config::expand_path(path);
        let file = File::open(&path).map_err(|e| format!("Failed to open {path}: {e}"))?;
        let file_size = file.metadata().map_err(|e| e.to_string())?.len();

        let large = file_size > FILE_LARGE_BYTES;
        let (content_reader, content_start_byte) = if large {
            open_tail_reader(file.try_clone().map_err(|e| e.to_string())?, file_size)?
        } else {
            (Some(BufReader::new(file)), 0)
        };

        let index_file = File::open(&path).map_err(|e| format!("Failed to open {path}: {e}"))?;

        Ok(Self {
            path,
            file_size,
            content_reader,
            content_lines_read: 0,
            content_finished: file_size == 0,
            content_start_byte,
            index_file: Some(index_file),
            index: LineIndex::new(file_size),
            index_bytes_done: 0,
            index_finished: file_size == 0,
        })
    }

    /// Advance content load and/or index scan. Returns `(content_lines, content_done, index_done)`.
    pub fn tick(&mut self) -> Result<(Vec<String>, bool, bool), String> {
        let mut lines = Vec::new();

        if let Some(reader) = self.content_reader.as_mut() {
            if !self.content_finished {
                for _ in 0..FILE_LOAD_LINES_PER_TICK {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            self.content_finished = true;
                            break;
                        }
                        Ok(_) => {
                            if line.ends_with('\n') {
                                line.pop();
                                if line.ends_with('\r') {
                                    line.pop();
                                }
                            }
                            lines.push(line);
                            self.content_lines_read += 1;
                        }
                        Err(err) => {
                            return Err(format!("Read error in {}: {err}", self.path));
                        }
                    }
                }
            }
        } else {
            self.content_finished = true;
        }

        // Index only after content ingestion finishes so each tick does one heavy job.
        if self.content_finished {
            if let Some(file) = self.index_file.as_mut() {
                if !self.index_finished {
                    let (next, done) = self
                        .index
                        .scan_chunk(file, self.index_bytes_done, INDEX_BYTES_PER_TICK)?;
                    self.index_bytes_done = next;
                    if done {
                        self.index_finished = true;
                    }
                }
            }
        }

        Ok((lines, self.content_finished, self.index_finished))
    }

    pub fn index_progress(&self) -> f32 {
        self.index.progress(self.index_bytes_done)
    }

    pub fn into_backed(self) -> Result<FileBackedLog, String> {
        let file = File::open(&self.path).map_err(|e| format!("Failed to open {}: {e}", self.path))?;
        Ok(FileBackedLog {
            path: self.path,
            file,
            index: self.index,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.content_finished && self.index_finished
    }
}

fn open_tail_reader(mut file: File, file_size: u64) -> Result<(Option<BufReader<File>>, u64), String> {
    let seek_pos = file_size.saturating_sub(FILE_INITIAL_TAIL_BYTES);
    file.seek(SeekFrom::Start(seek_pos))
        .map_err(|e| format!("Seek failed: {e}"))?;

    let mut reader = BufReader::new(file);
    if seek_pos > 0 {
        let mut discard = String::new();
        match reader.read_line(&mut discard) {
            Ok(0) => {}
            Ok(_) => {}
            Err(err) => return Err(format!("Read error after seek: {err}")),
        }
    }

    Ok((Some(reader), seek_pos))
}

/// Create a temp log for tests (line_count lines, each ~20 bytes).
#[cfg(test)]
pub fn write_test_log(path: &std::path::Path, line_count: usize) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    for i in 0..line_count {
        writeln!(file, "log line {i:08} payload")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_log_path(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noviewlog-{name}-{stamp}.log"))
    }

    #[test]
    fn large_file_starts_at_tail_but_indexes_whole_file() {
        let path = temp_log_path("large");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            for i in 0..90_000 {
                writeln!(f, "line {i:08} {}", "x".repeat(80)).unwrap();
            }
        }
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        assert!(state.file_size > FILE_LARGE_BYTES);
        assert!(state.content_start_byte > 0);

        let mut first_line: Option<String> = None;
        while !state.content_finished {
            let (lines, _, _) = state.tick().unwrap();
            if first_line.is_none() {
                first_line = lines.first().cloned();
            }
        }
        let first = first_line.expect("tail content");
        assert!(
            !first.contains("line 000000"),
            "initial view should be tail, got {first}"
        );

        while !state.index_finished {
            state.tick().unwrap();
        }
        assert_eq!(state.index.total_lines(), 90_000);

        let backed = state.into_backed().unwrap();
        let mut file = backed.file;
        let early = backed.index.read_lines(&mut file, 0, 2).unwrap();
        assert!(early[0].contains("line 000000"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn small_file_reads_from_start() {
        let path = temp_log_path("small");
        write_test_log(&path, 100).unwrap();
        let state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        assert_eq!(state.content_start_byte, 0);
        assert!(!state.index_finished);
        let _ = std::fs::remove_file(path);
    }
}
