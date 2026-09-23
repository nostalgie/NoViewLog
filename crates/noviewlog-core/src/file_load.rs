use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::file_index::{decode_lossy_line, FileBackedLog, LineIndex, TempFileGuard, INDEX_BYTES_PER_TICK};

/// Files above this size show a small tail window immediately while the line
/// index is built in the background.
pub const FILE_LARGE_BYTES: u64 = 8 * 1024 * 1024;

/// Seek near EOF by this many bytes when picking the initial window for large files.
/// (Actual ingested lines are capped by [`FILE_INITIAL_WINDOW_LINES`].)
pub const FILE_INITIAL_TAIL_BYTES: u64 = 2 * 1024 * 1024;

/// Max lines materialized into the buffer on first open of a large file.
/// Keeps first paint cheap even when the tail has very long lines.
pub const FILE_INITIAL_WINDOW_LINES: usize = 800;

/// Soft cap for file sliding windows (independent of live PTY scrollback setting).
/// Long-line logs (access logs, URLs) explode memory/CPU under wrap if this is 10–30k.
pub const FILE_VIEW_WINDOW_LINES: usize = 2_000;

/// Lines ingested per engine tick while file content is loading.
pub const FILE_LOAD_LINES_PER_TICK: usize = 2_000;

/// Refuse to transcode UTF-16 files larger than this (temp UTF-8 copy would
/// double disk usage; PowerShell `Out-File` logs are far below this).
const UTF16_TRANSCODE_MAX_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq)]
enum FileEncoding {
    Plain,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

fn sniff_encoding(prefix: &[u8]) -> Result<FileEncoding, String> {
    if prefix.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Ok(FileEncoding::Utf8Bom)
    } else if prefix.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        // UTF-32LE BOM would transcode as NUL-riddled UTF-16 (issue #111).
        Err("UTF-32LE (BOM FF FE 00 00) log files are not supported; re-encode to UTF-8".into())
    } else if prefix.starts_with(&[0xFF, 0xFE]) {
        Ok(FileEncoding::Utf16Le)
    } else if prefix.starts_with(&[0xFE, 0xFF]) {
        Ok(FileEncoding::Utf16Be)
    } else {
        Ok(FileEncoding::Plain)
    }
}

/// Transcode a UTF-16 (BOM-prefixed) file to a UTF-8 temp file and return its
/// path + size. Unpaired surrogates become U+FFFD instead of failing the load.
fn transcode_utf16_to_temp(
    file: &mut File,
    big_endian: bool,
) -> Result<(PathBuf, u64), String> {
    static TRANSCODE_SEQ: AtomicU64 = AtomicU64::new(0);
    let out_path = std::env::temp_dir().join(format!(
        "noviewlog-utf16-{}-{}.log",
        std::process::id(),
        TRANSCODE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    // Skip the 2-byte BOM.
    file.seek(SeekFrom::Start(2))
        .map_err(|e| format!("Seek failed: {e}"))?;
    let mut reader = BufReader::new(&mut *file);
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(&out_path).map_err(|e| format!("Temp file create failed: {e}"))?,
    );

    // Stage keeps chunk boundaries pair-aligned; a trailing high surrogate is
    // held back so pairs split across reads still decode.
    let mut stage: Vec<u8> = Vec::with_capacity(65_536 + 1);
    let mut chunk = vec![0u8; 65_536];
    loop {
        let n = reader.read(&mut chunk).map_err(|e| format!("Read error: {e}"))?;
        if n == 0 {
            break;
        }
        stage.extend_from_slice(&chunk[..n]);
        let usable = stage.len() & !1;
        if usable == 0 {
            continue;
        }
        let mut units = Vec::with_capacity(usable / 2);
        for pair in stage[..usable].chunks_exact(2) {
            units.push(if big_endian {
                u16::from_be_bytes([pair[0], pair[1]])
            } else {
                u16::from_le_bytes([pair[0], pair[1]])
            });
        }
        let mut take = units.len();
        if let Some(&last) = units.last() {
            if (0xD800..0xDC00).contains(&last) {
                take -= 1; // maybe a pair continues in the next chunk
            }
        }
        let text = String::from_utf16_lossy(&units[..take]);
        out.write_all(text.as_bytes())
            .map_err(|e| format!("Temp file write failed: {e}"))?;
        stage.drain(..take * 2);
    }
    if !stage.is_empty() {
        // Dangling byte or unpaired surrogate at EOF.
        out.write_all("\u{FFFD}".as_bytes())
            .map_err(|e| format!("Temp file write failed: {e}"))?;
    }
    out.flush()
        .map_err(|e| format!("Temp file write failed: {e}"))?;
    drop(out);

    let size = std::fs::metadata(&out_path)
        .map_err(|e| format!("Temp file stat failed: {e}"))?
        .len();
    Ok((out_path, size))
}

pub struct FileLoadState {
    pub path: String,
    pub file_size: u64,
    /// Content reader (tail-first for large files, start-to-end for small).
    content_reader: Option<BufReader<File>>,
    pub content_lines_read: u64,
    pub content_finished: bool,
    /// Stop content ingest after this many lines (`None` = read until EOF).
    content_line_limit: Option<usize>,
    /// Byte offset where the content reader started (for window placement).
    pub content_start_byte: u64,
    /// Background index construction.
    index_file: Option<File>,
    pub index: LineIndex,
    pub index_bytes_done: u64,
    pub index_finished: bool,
    /// File the IO actually targets (temp UTF-8 copy for UTF-16 sessions).
    source_path: PathBuf,
    /// Deletes the temp copy when this state (or the derived FileBackedLog) drops.
    temp: Option<TempFileGuard>,
}

impl FileLoadState {
    pub fn open(path: &str) -> Result<Self, String> {
        let path = crate::core::config::expand_path(path);
        let mut file = File::open(&path).map_err(|e| format!("Failed to open {path}: {e}"))?;
        let raw_size = file.metadata().map_err(|e| e.to_string())?.len();

        // Sniff a BOM on a cloned handle so `file` stays at position 0.
        let encoding = {
            let mut sniff = file.try_clone().map_err(|e| e.to_string())?;
            let mut prefix = [0u8; 4];
            let mut got = 0usize;
            while got < prefix.len() {
                match sniff.read(&mut prefix[got..]) {
                    Ok(0) => break,
                    Ok(k) => got += k,
                    Err(e) => return Err(format!("Read error in {path}: {e}")),
                }
            }
            sniff_encoding(&prefix[..got])?
        };

        let (source_path, file_size, temp, bom_skip) = match encoding {
            FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
                if raw_size > UTF16_TRANSCODE_MAX_BYTES {
                    return Err(format!(
                        "UTF-16 file too large to transcode ({raw_size} bytes): {path}"
                    ));
                }
                let (tmp, size) =
                    transcode_utf16_to_temp(&mut file, encoding == FileEncoding::Utf16Be)?;
                (tmp.clone(), size, Some(TempFileGuard(tmp)), 0u64)
            }
            FileEncoding::Utf8Bom => (PathBuf::from(&path), raw_size, None, 3u64),
            FileEncoding::Plain => (PathBuf::from(&path), raw_size, None, 0u64),
        };

        let content_file = File::open(&source_path)
            .map_err(|e| format!("Failed to open {path}: {e}"))?;
        let large = file_size > FILE_LARGE_BYTES;
        let (content_reader, content_start_byte) = if large {
            open_tail_reader(content_file, file_size)?
        } else {
            let mut content_file = content_file;
            if bom_skip > 0 {
                content_file
                    .seek(SeekFrom::Start(bom_skip))
                    .map_err(|e| format!("Seek failed: {e}"))?;
            }
            (Some(BufReader::new(content_file)), bom_skip)
        };

        let index_file = File::open(&source_path)
            .map_err(|e| format!("Failed to open {path}: {e}"))?;

        Ok(Self {
            path,
            file_size,
            content_reader,
            content_lines_read: 0,
            content_finished: file_size == 0,
            content_line_limit: if large {
                Some(FILE_INITIAL_WINDOW_LINES)
            } else {
                None
            },
            content_start_byte,
            index_file: Some(index_file),
            index: LineIndex::new(file_size),
            index_bytes_done: bom_skip,
            index_finished: file_size == 0,
            source_path,
            temp,
        })
    }

    /// Advance content load and/or index scan. Returns `(content_lines, content_done, index_done)`.
    pub fn tick(&mut self) -> Result<(Vec<String>, bool, bool), String> {
        let mut lines = Vec::new();

        if let Some(reader) = self.content_reader.as_mut() {
            if !self.content_finished {
                let limit = self.content_line_limit.unwrap_or(usize::MAX);
                let budget = FILE_LOAD_LINES_PER_TICK
                    .min(limit.saturating_sub(self.content_lines_read as usize));
                for _ in 0..budget {
                    let mut raw = Vec::new();
                    match reader.read_until(b'\n', &mut raw) {
                        Ok(0) => {
                            self.content_finished = true;
                            break;
                        }
                        Ok(_) => {
                            lines.push(decode_lossy_line(&raw));
                            self.content_lines_read += 1;
                        }
                        Err(err) => {
                            return Err(format!("Read error in {}: {err}", self.path));
                        }
                    }
                }
                if self
                    .content_line_limit
                    .is_some_and(|lim| self.content_lines_read as usize >= lim)
                {
                    self.content_finished = true;
                }
            }
        } else {
            self.content_finished = true;
        }

        // Index in parallel with content so large files become scrollable sooner.
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

        Ok((lines, self.content_finished, self.index_finished))
    }

    pub fn index_progress(&self) -> f32 {
        self.index.progress(self.index_bytes_done)
    }

    pub fn into_backed(self) -> Result<FileBackedLog, String> {
        let file = File::open(&self.source_path)
            .map_err(|e| format!("Failed to open {}: {e}", self.path))?;
        Ok(FileBackedLog {
            path: self.path,
            file,
            index: self.index,
            temp: self.temp,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.content_finished && self.index_finished
    }

    /// Content window is ready to show (index may still be running).
    pub fn content_ready(&self) -> bool {
        self.content_finished
    }
}

fn open_tail_reader(mut file: File, file_size: u64) -> Result<(Option<BufReader<File>>, u64), String> {
    let seek_pos = file_size.saturating_sub(FILE_INITIAL_TAIL_BYTES);
    file.seek(SeekFrom::Start(seek_pos))
        .map_err(|e| format!("Seek failed: {e}"))?;

    let mut reader = BufReader::new(file);
    if seek_pos > 0 {
        // The seek offset is arbitrary and may land inside a multi-byte UTF-8
        // sequence (Cyrillic / CJK / emoji logs). Discard the partial line as
        // raw bytes: `read_line` would fail UTF-8 validation and abort the
        // whole file open.
        let mut discard = Vec::new();
        if let Err(err) = reader.read_until(b'\n', &mut discard) {
            return Err(format!("Read error after seek: {err}"));
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
        assert!(
            state.content_lines_read as usize <= FILE_INITIAL_WINDOW_LINES,
            "initial window must be capped, got {}",
            state.content_lines_read
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
    fn tail_seek_mid_utf8_char_opens_large_file() {
        // Regression for issue #50: the tail seek offset (file_size - 2 MiB)
        // must land inside a multi-byte UTF-8 character. Layout (byte offsets):
        //   [0..prefix_len)              'a' prefix (no newline)
        //   [prefix_len..prefix_len+3)   "日" (E6 97 A5)
        //   [prefix_len+3..EOF)          209715 lines of "sNNNNNNNN\n"
        // With prefix_len = FILE_LARGE_BYTES - FILE_INITIAL_TAIL_BYTES + 1024,
        // file_size = prefix_len + 3 + 2_097_150 = FILE_LARGE_BYTES + 1025, so
        // the file takes the large path and seek_pos = prefix_len + 1 lands on
        // the 2nd byte of the 3-byte char.
        let path = temp_log_path("utf8-boundary");
        let tail = FILE_INITIAL_TAIL_BYTES as usize;
        let prefix_len = (FILE_LARGE_BYTES - FILE_INITIAL_TAIL_BYTES) as usize + 1024;
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(&vec![b'a'; prefix_len]).unwrap();
            f.write_all("日".as_bytes()).unwrap();
            for i in 0..((tail - 2) / 10) {
                writeln!(f, "s{i:08}").unwrap();
            }
        }
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        assert!(state.file_size > FILE_LARGE_BYTES);
        assert_eq!(state.content_start_byte, prefix_len as u64 + 1);

        // The partial line (rest of the char + first suffix line) is discarded;
        // content starts at the first complete line after it.
        let mut first_line: Option<String> = None;
        while first_line.is_none() && !state.content_finished {
            let (lines, _, _) = state.tick().unwrap();
            first_line = lines.first().cloned();
        }
        assert_eq!(first_line.as_deref(), Some("s00000001"));

        while !state.index_finished {
            state.tick().unwrap();
        }
        assert_eq!(state.index.total_lines(), 209_715);

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

    fn drain_content(state: &mut FileLoadState) -> Vec<String> {
        let mut all = Vec::new();
        while !state.content_finished {
            let (lines, _, _) = state.tick().unwrap();
            all.extend(lines);
        }
        while !state.index_finished {
            state.tick().unwrap();
        }
        all
    }

    #[test]
    fn non_utf8_byte_loads_with_replacement() {
        // Issue #51: a lone cp1251 byte must not abort load/index/match.
        let path = temp_log_path("cp1251");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"log \xED gotcha\nplain\n").unwrap();
        }
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        let lines = drain_content(&mut state);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains('\u{FFFD}'), "got {:?}", lines[0]);
        assert_eq!(lines[1], "plain");
        assert_eq!(state.index.total_lines(), 2);

        let mut backed = state.into_backed().unwrap();
        let read = backed.read_lines(0, 2).unwrap();
        assert!(read[0].contains('\u{FFFD}'));
        assert_eq!(read[1], "plain");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn utf16le_bom_file_loads() {
        // Issue #51: PowerShell `Out-File` default encoding (UTF-16LE + BOM).
        let path = temp_log_path("utf16le");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&path).unwrap();
            let text = "alpha\r\nerror beta\r\n";
            let units: Vec<u16> = text.encode_utf16().collect();
            let bytes: Vec<u8> = units
                .iter()
                .flat_map(|u| u.to_le_bytes())
                .collect();
            f.write_all(&[0xFF, 0xFE]).unwrap();
            f.write_all(&bytes).unwrap();
        }
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        let lines = drain_content(&mut state);
        assert_eq!(lines, vec!["alpha", "error beta"]);
        assert_eq!(state.index.total_lines(), 2);

        let mut backed = state.into_backed().unwrap();
        assert_eq!(backed.read_lines(0, 2).unwrap(), vec!["alpha", "error beta"]);
        drop(backed);
        // Temp copy is removed with the backed log.
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn utf16be_bom_file_loads() {
        let path = temp_log_path("utf16be");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&path).unwrap();
            let text = "one\nдва\n";
            let units: Vec<u16> = text.encode_utf16().collect();
            let bytes: Vec<u8> = units
                .iter()
                .flat_map(|u| u.to_be_bytes())
                .collect();
            f.write_all(&[0xFE, 0xFF]).unwrap();
            f.write_all(&bytes).unwrap();
        }
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        let lines = drain_content(&mut state);
        assert_eq!(lines, vec!["one", "два"]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn utf16_unpaired_surrogate_becomes_replacement() {
        let path = temp_log_path("utf16-surrogate");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(&[0xFF, 0xFE]).unwrap();
            // Valid pair then a lone high surrogate at EOF.
            f.write_all(&0x0041u16.to_le_bytes()).unwrap();
            f.write_all(&0xD83Du16.to_le_bytes()).unwrap();
            f.write_all(&0xDE00u16.to_le_bytes()).unwrap();
            f.write_all(&0xD800u16.to_le_bytes()).unwrap();
        }
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        let lines = drain_content(&mut state);
        assert_eq!(lines, vec!["A\u{1F600}\u{FFFD}"]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn utf8_bom_stripped_from_content_and_index() {
        // Issue #74: the BOM must not ride along line 0 into search/copy.
        let path = temp_log_path("utf8bom");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"\xEF\xBB\xBFalpha\nbeta\n").unwrap();
        }
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        let lines = drain_content(&mut state);
        assert_eq!(lines, vec!["alpha", "beta"]);

        let mut backed = state.into_backed().unwrap();
        assert_eq!(backed.read_lines(0, 2).unwrap(), vec!["alpha", "beta"]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn big_log_opens_quickly_with_generated_fixture() {
        // Issue #72: generated fixture instead of a hardcoded dev-machine path.
        let path = crate::tests::big_log_fixture();
        let start = std::time::Instant::now();
        let mut state = FileLoadState::open(path.to_str().unwrap()).unwrap();
        assert!(state.file_size > FILE_LARGE_BYTES);

        // First content window must finish fast (not the whole file).
        while !state.content_finished {
            state.tick().unwrap();
        }
        let content_ms = start.elapsed().as_millis();
        assert!(
            state.content_lines_read as usize <= FILE_INITIAL_WINDOW_LINES,
            "content lines {}",
            state.content_lines_read
        );
        assert!(
            content_ms < 2_000,
            "initial window took {content_ms}ms (want <2s)"
        );

        // Full sparse index for ~74MB should finish in a few seconds of CPU ticks.
        let index_start = std::time::Instant::now();
        while !state.index_finished {
            state.tick().unwrap();
        }
        let index_ms = index_start.elapsed().as_millis();
        assert!(
            index_ms < 15_000,
            "sparse index took {index_ms}ms (want <15s)"
        );
        assert!(state.index.total_lines() > 100_000);
    }
}
