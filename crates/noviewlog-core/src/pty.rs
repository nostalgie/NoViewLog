use crate::core::config::expand_path_opt;
use crate::spawn_resolve::prepare_spawn;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

/// Raw PTY output. Terminal emulation (cursor/erase/scrollback) happens in the
/// consumer via [`crate::core::terminal::TerminalIngest`], not here — the read
/// loop only frames raw bytes so nothing is lost to premature line splitting.
#[derive(Debug)]
pub enum PtyEvent {
    /// A raw chunk of PTY output bytes for a specific terminal session.
    Bytes { id: String, data: Vec<u8> },
    /// Child exited. `generation` matches the [`PtyManager::start`] call that
    /// spawned this child — leftover `Exit` from a previous session must be ignored.
    Exit { id: String, code: i32, generation: u64 },
}

pub struct PtyManager {
    running: Arc<AtomicBool>,
    child_killer: Option<Box<dyn portable_pty::ChildKiller + Send>>,
    writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
    /// Must outlive the child on Windows: dropping the ConPTY master calls
    /// `ClosePseudoConsole`, which makes the child exit with `0xC0000142`
    /// (`STATUS_DLL_INIT_FAILED`) if it has not finished console init yet.
    master: Option<Box<dyn MasterPty + Send>>,
    /// Last size applied to the live PTY (or the size used for the next open).
    size: PtySize,
    /// Session token from the last successful [`Self::start`] (0 = never started).
    generation: u64,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            child_killer: None,
            writer: None,
            master: None,
            size: PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            },
            generation: 0,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Remember geometry for the next `start`, and resize a live PTY if any.
    pub fn set_size(&mut self, size: PtySize) -> Result<(), String> {
        let size = PtySize {
            rows: size.rows.max(1),
            cols: size.cols.max(1),
            pixel_width: size.pixel_width,
            pixel_height: size.pixel_height,
        };
        if size == self.size {
            return Ok(());
        }
        self.size = size;
        if let Some(master) = &self.master {
            master.resize(size).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn size(&self) -> PtySize {
        self.size
    }

    /// Start a process in this PTY. Stops any previous child of **this** manager only.
    pub fn start(
        &mut self,
        tx: Sender<PtyEvent>,
        id: String,
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
        generation: u64,
    ) -> Result<(), String> {
        self.stop();
        self.generation = generation;

        // Always set cwd: portable-pty may not inherit the parent process cwd.
        // On Windows, prepare_spawn rewrites UNC / WSL cwd (never pass UNC to CreateProcess).
        let workdir = expand_path_opt(cwd)
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| ".".to_string());

        let prepared = prepare_spawn(&command, args, Some(workdir.as_str()))?;
        let command = prepared.command;
        let args = prepared.args;
        let workdir = prepared.cwd;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(self.size)
            .map_err(|e| e.to_string())?;

        let mut cmd = CommandBuilder::new(&command);
        cmd.args(&args);
        cmd.cwd(&workdir);
        cmd.env("FORCE_COLOR", "1");
        // Advertise a real terminal so ora/listr2/ink use cursor-addressed
        // progress rendering (which the emulator handles) consistently.
        cmd.env("TERM", "xterm-256color");
        // Some tools read COLUMNS/LINES instead of TIOCGWINSZ.
        cmd.env("COLUMNS", self.size.cols.to_string());
        cmd.env("LINES", self.size.rows.to_string());

        let mut child = pair.slave.spawn_command(cmd).map_err(|e| {
            format!(
                "failed to spawn '{command}' (args: {args:?}) in cwd '{workdir}': {e}"
            )
        })?;
        // Release slave handles only — keep master until stop()/Drop.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        self.writer = Some(Arc::new(Mutex::new(writer)));
        self.master = Some(pair.master);

        let killer = child.clone_killer();
        self.child_killer = Some(killer);
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let session_id = id;
        let session_generation = generation;

        thread::spawn(move || {
            let mut chunk = [0u8; 4096];

            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx
                            .send(PtyEvent::Bytes {
                                id: session_id.clone(),
                                data: chunk[..n].to_vec(),
                            })
                            .is_err()
                        {
                            running.store(false, Ordering::SeqCst);
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // portable-pty reports the raw DWORD; cast to i32 so NTSTATUS values
            // like 0xC0000142 surface as the familiar negative -1073741502 in the UI.
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(1);
            let _ = tx.send(PtyEvent::Exit {
                id: session_id,
                code,
                generation: session_generation,
            });
            running.store(false, Ordering::SeqCst);
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.writer = None;
        if let Some(mut killer) = self.child_killer.take() {
            let _ = killer.kill();
        }
        // Drop master last so ConPTY stays valid until the child is signalled.
        self.master = None;
    }

    /// Send raw bytes to the child process stdin (PTY).
    pub fn write_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        let Some(writer) = &self.writer else {
            return Err("process is not running".to_string());
        };
        let mut guard = writer.lock().map_err(|e| e.to_string())?;
        guard.write_all(data).map_err(|e| e.to_string())?;
        guard.flush().map_err(|e| e.to_string())
    }

    /// Send a line to the process (appends `\n`, like pressing Enter in a terminal).
    pub fn write_line(&mut self, line: &str) -> Result<(), String> {
        let mut data = line.as_bytes().to_vec();
        data.push(b'\n');
        self.write_bytes(&data)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Test helper: run raw bytes (in the given chunks) through the terminal
/// emulator and return the final flat log lines (ANSI-stripped).
#[cfg(test)]
pub fn emulate_chunks_to_lines(chunks: &[&[u8]]) -> Vec<String> {
    use crate::core::ansi::strip_ansi;
    use crate::core::buffer::RecordBuffer;
    use crate::core::formats::get_builtin_format;
    use crate::core::parser::RecordParser;
    use crate::core::terminal::TerminalIngest;

    let mut ingest = TerminalIngest::new();
    let mut buffer = RecordBuffer::new(100_000);
    let mut parser = RecordParser::new(get_builtin_format("node-default"));
    for chunk in chunks {
        ingest.feed(chunk, &mut buffer, &mut parser);
    }
    ingest.finish(&mut buffer, &mut parser);

    buffer
        .records()
        .iter()
        .flat_map(|r| r.lines.iter())
        .map(|l| strip_ansi(l))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_glued_spinner(lines: &[String]) -> bool {
        lines.iter().any(|s| {
            let spinner = s.chars().any(|c| {
                matches!(c, '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏')
            });
            spinner && s.contains('✔')
        })
    }

    fn assert_strapi_startup(lines: &[String], ctx: &str) {
        assert!(
            !has_glued_spinner(lines),
            "[{ctx}] glued spinner+check line: {lines:?}"
        );
        for needle in [
            "✔ Cleaning dist dir",
            "✔ Compiling TS",
            "Project information",
            "Actions available",
            "Welcome back!",
            "http://localhost:1337",
        ] {
            assert!(
                lines.iter().any(|l| l.contains(needle)),
                "[{ctx}] missing {needle:?} in {lines:?}"
            );
        }
        let table_rows = lines
            .iter()
            .filter(|l| l.contains('│') || l.contains('╭') || l.contains('╰'))
            .count();
        assert!(table_rows >= 9, "[{ctx}] too few table rows: {table_rows}");
        // Every ✔ step must survive as its own committed line, not collapse away.
        for step in ["Cleaning dist dir", "Compiling TS"] {
            assert_eq!(
                lines.iter().filter(|l| l.contains('✔') && l.contains(step)).count(),
                lines.iter().filter(|l| l.contains(step) && l.contains('✔')).count(),
                "[{ctx}] {step}"
            );
        }
    }

    #[test]
    fn interactive_capture_all_chunk_sizes() {
        let bytes = include_bytes!("../test_data/strapi-portable-pty.bin");
        for size in [1usize, 7, 37, 64, 512, 4096] {
            let chunks: Vec<&[u8]> = bytes.chunks(size).collect();
            let lines = emulate_chunks_to_lines(&chunks);
            assert_strapi_startup(&lines, &format!("interactive chunk={size}"));
        }
    }

    #[test]
    fn noninteractive_capture_preserves_steps() {
        let bytes = include_bytes!("../test_data/strapi-develop-sample.bin");
        let chunks: Vec<&[u8]> = bytes.chunks(64).collect();
        let lines = emulate_chunks_to_lines(&chunks);
        assert!(!has_glued_spinner(&lines));
        assert!(lines.iter().any(|l| l.contains("Loading Strapi")));
        assert!(lines.iter().any(|l| l.contains("Compiling TS")));
    }

    #[test]
    fn utf8_box_drawing_survives_chunk_splits() {
        let border = "╭────┬────╮\r\n";
        let bytes = border.as_bytes();
        for size in 1..=bytes.len() {
            let chunks: Vec<&[u8]> = bytes.chunks(size).collect();
            let lines = emulate_chunks_to_lines(&chunks);
            assert!(
                lines.iter().any(|l| l.contains('╭') && l.contains('╮')),
                "chunk={size} lines={lines:?}"
            );
        }
    }

    #[test]
    fn plain_lines_pass_through() {
        let lines = emulate_chunks_to_lines(&[b"hello\r\n", b"world\r\n"]);
        assert!(lines.iter().any(|l| l.contains("hello")));
        assert!(lines.iter().any(|l| l.contains("world")));
    }

    #[test]
    fn spinner_frames_collapse_to_final_check() {
        // CSI cursor-up + redraw: final ✔ should win, spinner frames discarded.
        let frames: &[&[u8]] = &[
            b"\x1b[?25l\x1b[1G\x1b[0K\x1b[32m\xe2\xa0\x8b\x1b[0m Cleaning\r\n",
            b"\x1b[1A\x1b[1G\x1b[0K\x1b[32m\xe2\xa0\x99\x1b[0m Cleaning\r\n",
            b"\x1b[1A\x1b[1G\x1b[0K\x1b[32m\xe2\x9c\x94\x1b[0m Cleaning\r\n",
        ];
        let lines = emulate_chunks_to_lines(frames);
        assert!(!has_glued_spinner(&lines));
        assert!(lines.iter().any(|l| l.contains('✔') && l.contains("Cleaning")));
    }

    #[test]
    fn separate_completion_lines_are_all_kept() {
        let lines = emulate_chunks_to_lines(&[
            "✔ step one\r\n".as_bytes(),
            "✔ step two\r\n".as_bytes(),
        ]);
        assert_eq!(
            lines.iter().filter(|l| l.contains('✔')).count(),
            2,
            "{lines:?}"
        );
    }
}
