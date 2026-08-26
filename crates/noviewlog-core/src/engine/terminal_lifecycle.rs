use super::*;

pub(crate) enum StartAction {
    File(String),
    Launch,
}

/// Human-readable exit status. Maps Windows NTSTATUS values that commonly appear
/// when ConPTY spawn goes wrong (e.g. Store App execution aliases).
fn format_process_exit_message(code: i32) -> String {
    const STATUS_DLL_INIT_FAILED: i32 = -1073741502; // 0xC0000142 as i32
    if code == STATUS_DLL_INIT_FAILED {
        return format!(
            "Process exited ({code} / 0xC0000142 STATUS_DLL_INIT_FAILED). \
             Often caused by a Microsoft Store node.exe stub under WindowsApps — \
             set Command to the full path of a real node.exe, or install Node from nodejs.org."
        );
    }
    format!("Process exited ({code})")
}

impl Engine {
    fn bump_pty_generation_for(&mut self, terminal_id: &str) -> u64 {
        let Some(term) = self.terminals.iter_mut().find(|t| t.id == terminal_id) else {
            return 0;
        };
        term.pty_generation = term.pty_generation.wrapping_add(1);
        term.pty_generation
    }

    pub(crate) fn poll_pty(&mut self) {
        let mut changed = false;
        let mut active_changed = false;
        let mut respawn_shell_ids: Vec<String> = Vec::new();
        let active_id = self
            .terminals
            .get(self.active_terminal)
            .map(|t| t.id.clone());
        while let Ok(event) = self.pty_rx.try_recv() {
            match event {
                PtyEvent::Bytes { id, data } => {
                    let Some(term) = self.terminals.iter_mut().find(|t| t.id == id) else {
                        continue;
                    };
                    term.ingest.feed(&data, &mut term.buffer, &mut term.parser);
                    if let Some(cwd) = term.ingest.take_cwd_update() {
                        term.cwd = cwd;
                    }
                    term.last_line_at = Some(Instant::now());
                    changed = true;
                    if active_id.as_ref() == Some(&id) {
                        active_changed = true;
                    }
                }
                PtyEvent::Exit {
                    id,
                    code,
                    generation,
                } => {
                    if self
                        .ptys
                        .get(&id)
                        .is_some_and(|p| p.generation() != generation)
                    {
                        continue;
                    }
                    let Some(term) = self.terminals.iter_mut().find(|t| t.id == id) else {
                        continue;
                    };
                    if term.pty_generation != generation {
                        continue;
                    }
                    term.ingest.finish(&mut term.buffer, &mut term.parser);
                    term.running = false;
                    term.exit_code = Some(code);
                    // File viewers are not interactive sessions — do not spawn a shell.
                    let file_session = term.is_file_session();
                    self.ptys.remove(&id);
                    let message = format_process_exit_message(code);
                    if active_id.as_ref() == Some(&id) {
                        self.status_message = message.clone();
                        active_changed = true;
                        self.push_event(json!({"type":"exit","code": code, "message": message}));
                    }
                    if !file_session {
                        respawn_shell_ids.push(id);
                    }
                    changed = true;
                }
            }
        }
        if changed {
            // Rebuild only the active terminal's views for display; inactive
            // buffers still receive bytes for when the user switches back.
            if active_changed {
                self.mark_all_views_dirty();
                self.mark_viewport_dirty();
            }
            self.last_stats_at = None;
        }
        // Keep a live session: when the PTY exits, respawn the default interactive shell.
        for id in respawn_shell_ids {
            self.start_interactive_shell_for(&id);
        }
        self.flush_idle_pending();
    }

    pub(crate) fn start_launch_process(&mut self) {
        if self.active_terminal().is_file_session() {
            self.status_message = "File terminal is view-only".to_string();
            self.push_event(json!({"type":"status","message": self.status_message}));
            return;
        }
        let id = self.active_terminal().id.clone();
        let launch = self.active_terminal().launch.clone();
        let (command, args, cwd) = match resolve_process_launch(&launch) {
            Ok(resolved) => resolved,
            Err(err) => {
                self.active_terminal_mut().running = false;
                self.status_message = format!("Failed to start: {err}");
                self.push_event(json!({"type":"status","message": self.status_message}));
                return;
            }
        };
        let cmdline = crate::spawn_resolve::format_spawn_cmdline(&command, &args, cwd.as_deref());
        self.status_message = format!("Running: {cmdline}");
        self.push_event(json!({"type":"status","message": self.status_message}));
        self.active_terminal_mut().exit_code = None;
        self.active_terminal_mut().running = true;
        self.sync_terminal_geometry();
        let size = self.viewport_pty_size();
        let generation = self.bump_pty_generation_for(&id);
        let start_result = {
            let pty = self.ptys.entry(id.clone()).or_default();
            let _ = pty.set_size(size);
            pty.start(
                self.pty_tx.clone(),
                id.clone(),
                command,
                args,
                cwd,
                generation,
            )
        };
        if let Err(err) = start_result {
            self.active_terminal_mut().running = false;
            self.ptys.remove(&id);
            self.status_message = format!("Failed to start: {err} | resolved: {cmdline}");
            self.push_event(json!({"type":"status","message": self.status_message}));
        } else {
            let terminal = self.active_terminal_mut();
            terminal.ingest.ensure_live_screen(&mut terminal.buffer);
            self.mark_all_views_dirty();
            self.mark_viewport_dirty();
        }
    }

    /// Start `$SHELL` / PowerShell / WSL bash for the active terminal.
    pub(crate) fn start_interactive_shell(&mut self) {
        let id = self.active_terminal().id.clone();
        self.start_interactive_shell_for(&id);
    }

    /// Start an interactive shell for a specific terminal (local / WSL / SSH later).
    pub(crate) fn start_interactive_shell_for(&mut self, terminal_id: &str) {
        let Some(term_idx) = self.terminals.iter().position(|t| t.id == terminal_id) else {
            return;
        };
        // Already running — nothing to do (avoids double-spawn races).
        if self.terminals[term_idx].running {
            return;
        }
        if self.terminals[term_idx].is_file_session() {
            return;
        }

        let mut launch = self.terminals[term_idx].launch.clone();
        if launch.cwd.is_none() {
            launch.cwd = Some(self.terminals[term_idx].cwd.clone());
        }
        let (command, args, cwd) = match resolve_interactive_shell(&launch) {
            Ok(resolved) => resolved,
            Err(err) => {
                self.status_message = format!("Shell: {err}");
                self.push_event(json!({"type":"status","message": self.status_message}));
                return;
            }
        };
        let cmdline = crate::spawn_resolve::format_spawn_cmdline(&command, &args, cwd.as_deref());
        let is_active = term_idx == self.active_terminal;
        if is_active {
            self.status_message = format!("Shell: {cmdline}");
            self.push_event(json!({"type":"status","message": self.status_message}));
        }
        {
            let term = &mut self.terminals[term_idx];
            term.exit_code = None;
            term.process_started = true;
            term.running = true;
        }
        if is_active {
            self.sync_terminal_geometry();
        }
        let size = self.viewport_pty_size();
        let id = terminal_id.to_string();
        let generation = self.bump_pty_generation_for(&id);
        let start_result = {
            let pty = self.ptys.entry(id.clone()).or_default();
            let _ = pty.set_size(size);
            pty.start(
                self.pty_tx.clone(),
                id.clone(),
                command,
                args,
                cwd,
                generation,
            )
        };
        if let Err(err) = start_result {
            self.terminals[term_idx].running = false;
            self.ptys.remove(&id);
            self.status_message = format!("Shell failed: {err} | {cmdline}");
            self.push_event(json!({"type":"status","message": self.status_message}));
        } else {
            let term = &mut self.terminals[term_idx];
            term.ingest.ensure_live_screen(&mut term.buffer);
            if is_active {
                self.mark_all_views_dirty();
            }
        }
        if is_active {
            self.mark_viewport_dirty();
        }
        self.last_stats_at = None;
    }

    pub(crate) fn stop(&mut self) {
        let id = self.active_terminal().id.clone();
        if let Some(mut pty) = self.ptys.remove(&id) {
            pty.stop();
        }
        let terminal = self.active_terminal_mut();
        terminal.running = false;
        self.status_message = "Stopped".to_string();
        self.push_event(json!({"type":"status","message": "Stopped"}));
    }

    pub(crate) fn terminal_add(&mut self) {
        self.terminal_add_blank();
        self.start_interactive_shell();
    }

    /// Create and activate a terminal without starting a shell (used for file viewers).
    pub(crate) fn terminal_add_blank(&mut self) {
        let runtime = build_runtime_config(&self.config, Some(&self.preset_name));
        let format = self.current_format();
        let id = next_terminal_id(&self.terminals);
        let terminal = TerminalState::new(
            id,
            LaunchConfig::default(),
            &runtime,
            &format,
            self.config.max_scrollback_lines,
        );
        self.terminals.push(terminal);
        self.active_terminal = self.terminals.len() - 1;
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn terminal_switch(&mut self, terminal_id: &str) {
        let Some(idx) = self.terminals.iter().position(|t| t.id == terminal_id) else {
            return;
        };
        if idx == self.active_terminal {
            return;
        }
        self.active_terminal = idx;
        self.mark_all_views_dirty();
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }

    pub(crate) fn terminal_move(&mut self, terminal_id: &str, to_index: usize) {
        let Some(from) = self.terminals.iter().position(|t| t.id == terminal_id) else {
            return;
        };
        if self.terminals.len() < 2 {
            return;
        }
        let to = to_index.min(self.terminals.len() - 1);
        if from == to {
            return;
        }
        let item = self.terminals.remove(from);
        self.terminals.insert(to, item);
        let active = self.active_terminal;
        self.active_terminal = if active == from {
            to
        } else if from < active && to >= active {
            active - 1
        } else if from > active && to <= active {
            active + 1
        } else {
            active
        };
        self.last_stats_at = None;
    }

    /// Set a custom sidebar title. Empty/whitespace or unknown id → no-op (does not clear).
    pub(crate) fn terminal_rename(&mut self, terminal_id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let Some(term) = self.terminals.iter_mut().find(|t| t.id == terminal_id) else {
            return;
        };
        term.custom_title = Some(name.to_string());
        self.last_stats_at = None;
    }

    pub(crate) fn terminal_start(&mut self, terminal_id: Option<&str>) {
        if let Some(id) = terminal_id {
            self.terminal_switch(id);
        }
        let (has_command, log_file) = {
            let launch = &self.active_terminal().launch;
            (launch.command.is_some(), launch.log_file.clone())
        };
        if has_command {
            self.active_terminal_mut().process_started = true;
            self.start_launch_process();
        } else if let Some(path) = log_file {
            self.active_terminal_mut().process_started = true;
            self.start_log_file_load(&path);
        } else {
            self.start_interactive_shell();
        }
    }

    pub(crate) fn terminal_close(&mut self, terminal_id: Option<&str>) {
        if self.terminals.len() <= 1 {
            return;
        }
        let idx = match terminal_id {
            Some(id) => self.terminals.iter().position(|t| t.id == id),
            None => Some(self.active_terminal),
        };
        let Some(idx) = idx else {
            return;
        };
        // Refuse closing the first terminal.
        if idx == 0 {
            return;
        }
        let id = self.terminals[idx].id.clone();
        if let Some(mut pty) = self.ptys.remove(&id) {
            pty.stop();
        }
        self.terminals.remove(idx);
        if self.active_terminal >= self.terminals.len() {
            self.active_terminal = self.terminals.len() - 1;
        } else if idx < self.active_terminal {
            self.active_terminal -= 1;
        }
        self.mark_viewport_dirty();
        self.last_stats_at = None;
    }
}
