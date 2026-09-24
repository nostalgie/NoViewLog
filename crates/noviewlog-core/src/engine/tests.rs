//! Engine unit tests (moved verbatim from mod.rs).
use super::*;

fn drain_events(engine: &mut Engine) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(event) = engine.poll_event_json() {
        out.push(event);
    }
    out
}

#[test]
fn command_needs_active_terminal_classifies_commands() {
    let terminal_free = [
        Command::Resize {
            width: 10,
            height: 10,
        },
        Command::TerminalAdd,
        Command::TerminalClose { terminal_id: None },
        Command::LoadFile {
            path: "x.log".into(),
        },
        Command::SetSettings {
            max_scrollback_lines: 1_000,
        },
        Command::SetViewportFontSize { size: 14.0 },
        Command::FilterDraftSet {
            pattern: "x".into(),
            use_regex: false,
        },
    ];
    for cmd in &terminal_free {
        assert!(
            !Engine::command_needs_active_terminal(cmd),
            "must not need an active terminal: {cmd:?}"
        );
    }
    let needs_terminal = [
        Command::TabAdd,
        Command::ScrollLines { delta: 1 },
        Command::SearchSet {
            query: "x".into(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
        },
        Command::SetFollow { follow: true },
    ];
    for cmd in &needs_terminal {
        assert!(
            Engine::command_needs_active_terminal(cmd),
            "must need an active terminal: {cmd:?}"
        );
    }
}

#[test]
fn send_command_json_rejects_malformed_and_unknown_commands() {
    let mut engine = Engine::new();
    assert!(engine.send_command_json("{not json").is_err());
    assert!(engine
        .send_command_json(r#"{"cmd":"definitely_not_a_command"}"#)
        .is_err());
    // The rejected commands must not dirty the viewport.
    assert!(engine.viewport_dirty);
}

#[test]
fn severity_set_unknown_mode_is_dispatch_error() {
    let mut engine = Engine::new();
    let err = engine
        .send_command(Command::SeveritySet {
            mode: "bogus".into(),
        })
        .expect_err("unknown severity mode must fail");
    assert!(err.contains("unknown severity mode: bogus"), "err={err}");
}

#[test]
fn load_preset_alias_reports_missing_preset_via_status() {
    let mut engine = Engine::new();
    drain_events(&mut engine);
    engine
        .send_command_json(r#"{"cmd":"load_preset","name":"__noviewlog_test_missing_preset__"}"#)
        .expect("alias command itself must succeed");
    let events = drain_events(&mut engine);
    assert!(
        events
            .iter()
            .any(|e| e.contains("Preset not found: __noviewlog_test_missing_preset__")),
        "events={events:?}"
    );
}

#[test]
fn render_error_keeps_dirty_and_success_clears_it() {
    let mut engine = Engine::new();
    assert!(engine.needs_render());
    // Undersized buffer: the host-facing error must keep the dirty flag so
    // the host retries instead of showing a stale frame forever.
    let mut small = [0u8; 64];
    assert!(engine.render(64, 48, &mut small).is_err());
    assert!(engine.needs_render());

    let mut out = vec![0u8; 64 * 48 * 4];
    engine.render(64, 48, &mut out).expect("full-size render");
    assert!(!engine.needs_render());
    // Stopped Terminal tab paints the centered hint over a background.
    assert!(out.iter().any(|&b| b != 0));
}

#[test]
fn idle_tick_emits_stats_only_and_stays_clean() {
    let mut engine = Engine::new();
    // Settle: the first tick builds the initial (empty) view state and
    // may dirty once. Steady state must stay quiet.
    engine.tick();
    assert!(matches!(
        crate::engine::parse_engine_event(&drain_events(&mut engine)[0]),
        Some(crate::engine::EngineEvent::Stats(_))
    ));

    let mut out = vec![0u8; 800 * 600 * 4];
    engine.render(800, 600, &mut out).expect("render");
    assert!(!engine.needs_render());

    // Cross the stats throttle window (250 ms) so the next tick is
    // stats-eligible, mirroring the caret test's timing approach.
    std::thread::sleep(std::time::Duration::from_millis(300));
    engine.tick();
    assert!(
        !engine.needs_render(),
        "idle tick must not dirty the bitmap"
    );
    assert!(!engine.host_work_pending());
    let events = drain_events(&mut engine);
    assert_eq!(events.len(), 1, "exactly the stats snapshot: {events:?}");
    assert!(matches!(
        crate::engine::parse_engine_event(&events[0]),
        Some(crate::engine::EngineEvent::Stats(_))
    ));
    assert!(drain_events(&mut engine).is_empty());
}

#[test]
fn tick_auto_start_launch_loads_log_file_once() {
    let path = std::env::temp_dir().join(format!(
        "noviewlog-tick-autostart-{}.log",
        std::process::id()
    ));
    std::fs::write(&path, "hello\nworld\nwarn: boom\n").expect("write fixture");
    let mut engine = Engine::new();
    engine.auto_start_launch = true;
    engine.terminals[0].launch.log_file = Some(path.to_string_lossy().into_owned());

    assert!(!engine.file_load_pending_for_test());
    engine.tick();
    assert!(engine.process_started_for_test());
    // Small fixtures finish within the same tick (`advance_file_load` runs
    // at the end of `tick`); only assert the pending state afterwards.
    engine.finish_file_load_for_test();
    let records = engine.buffer_record_count_for_test();
    assert!(records >= 3);

    // A later tick must not restart the already-started launch.
    engine.tick();
    engine.finish_file_load_for_test();
    assert!(!engine.file_load_pending_for_test());
    assert_eq!(
        engine.buffer_record_count_for_test(),
        records,
        "second tick must not reload or duplicate records"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn poll_event_json_is_fifo() {
    let mut engine = Engine::new();
    engine.enqueue_event_for_test(r#"{"type":"status","message":"first"}"#.to_string());
    engine.enqueue_event_for_test(r#"{"type":"status","message":"second"}"#.to_string());
    let first = engine.poll_event_json().expect("first event");
    assert!(first.contains("first"));
    let second = engine.poll_event_json().expect("second event");
    assert!(second.contains("second"));
    assert!(engine.poll_event_json().is_none());
}

#[test]
fn stdin_command_without_pty_surfaces_status() {
    let mut engine = Engine::new();
    engine.mark_running_for_test();
    drain_events(&mut engine);
    engine
        .send_command(Command::Stdin {
            text: String::new(),
            bytes: Some(b"x".to_vec()),
        })
        .expect("stdin dispatch");
    let events = drain_events(&mut engine);
    assert!(
        events
            .iter()
            .any(|e| e.contains("stdin: no pty for terminal")),
        "events={events:?}"
    );
}

#[test]
fn stdin_on_stopped_filter_tab_is_dropped() {
    let mut engine = Engine::new();
    engine.send_command(Command::TabAdd).expect("tab add");
    assert_eq!(engine.active_tab_index_for_test(), 1);
    drain_events(&mut engine);
    // Not running + non-Terminal tab: keystrokes are ignored, no error spam.
    engine.handle_key(b"x");
    assert!(drain_events(&mut engine).is_empty());
}

#[test]
fn selection_clear_command_clears_selection_and_dirties() {
    let mut engine = Engine::new();
    let mut out = vec![0u8; 64 * 48 * 4];
    engine.render(64, 48, &mut out).expect("render");
    engine.active_terminal_mut().selection = Some(crate::viewport_layout::TextSelection::default());
    engine
        .send_command(Command::SelectionClear)
        .expect("selection clear");
    assert!(engine.active_terminal().selection.is_none());
    assert!(engine.needs_render());
}

#[test]
fn pty_ingest_dirty_throttle_cadence() {
    let mut engine = Engine::new();
    engine.viewport_dirty = false;

    // Never painted: flood frames must dirty immediately.
    engine.mark_viewport_dirty_after_pty_ingest(true);
    assert!(engine.viewport_dirty);

    // Just painted: throttled while more flood is pending.
    engine.note_viewport_painted();
    engine.viewport_dirty = false;
    engine.mark_viewport_dirty_after_pty_ingest(true);
    assert!(
        !engine.viewport_dirty,
        "throttle must skip mid-flood frames"
    );

    // Tail of the flood: always dirty so the tail becomes visible.
    engine.mark_viewport_dirty_after_pty_ingest(false);
    assert!(engine.viewport_dirty);
}

#[test]
fn pty_work_pending_tracks_hold_and_drain() {
    let mut engine = Engine::new();
    assert!(!engine.pty_work_pending());
    assert!(!engine.take_pty_drain_pending());

    engine.pty_drain_pending = true;
    assert!(engine.pty_work_pending());
    assert!(engine.take_pty_drain_pending());
    assert!(!engine.pty_work_pending());

    engine.pty_hold = Some(PtyEvent::Bytes {
        id: "t".into(),
        data: vec![b'x'],
        generation: 1,
    });
    assert!(engine.pty_work_pending());
    engine.last_pty_poll_at = Some(Instant::now());
    assert!(
        engine.defer_pty_reader_wake(),
        "recent poll with pending flood must defer the reader wake"
    );
}

#[test]
fn flush_persist_test_mode_clears_dirty_without_writes() {
    let mut engine = Engine::new();
    // mark_config_dirty is a no-op while persistence is skipped in tests.
    engine.mark_config_dirty();
    assert!(!engine.config_dirty);

    engine.config_dirty = true;
    engine.projects_dirty = true;
    engine.persist_changed_at = Some(Instant::now());
    engine.flush_persist();
    assert!(!engine.config_dirty);
    assert!(!engine.projects_dirty);
    assert!(engine.persist_changed_at.is_none());
}
