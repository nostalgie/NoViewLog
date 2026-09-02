#[test]
fn selection_copy_returns_plain_text() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine
        .send_command_json(r#"{"cmd":"resize","width":400,"height":200}"#)
        .expect("resize");
    engine
        .send_command_json(r#"{"cmd":"selection_at","x":16.0,"y":8.0,"extend":false}"#)
        .expect("selection_at");
    engine
        .send_command_json(r#"{"cmd":"selection_at","x":80.0,"y":8.0,"extend":true}"#)
        .expect("selection_extend");
    // Empty buffer — selection may be empty; command path must not error.
    let _ = engine.selection_text_for_test();
}


#[test]
fn terminal_add_switch_keeps_other_running() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    let first_id = engine.active_terminal_id_for_test();
    engine
        .send_command_json(r#"{"cmd":"terminal_start"}"#)
        .expect("terminal_start");
    assert_eq!(engine.terminal_running_for_test(&first_id), Some(true));

    engine
        .send_command_json(r#"{"cmd":"terminal_add"}"#)
        .expect("terminal_add");
    let second_id = engine.active_terminal_id_for_test();
    assert_ne!(first_id, second_id);
    assert_eq!(engine.terminals_for_test().len(), 2);
    // First terminal should still be marked running (multi-PTY; switch does not stop).
    assert_eq!(engine.terminal_running_for_test(&first_id), Some(true));
    assert_eq!(engine.terminal_running_for_test(&second_id), Some(true));

    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_switch","terminal_id":"{first_id}"}}"#))
        .expect("terminal_switch");
    assert_eq!(engine.active_terminal_id_for_test(), first_id);
    assert_eq!(engine.terminal_running_for_test(&first_id), Some(true));
    assert_eq!(engine.terminal_running_for_test(&second_id), Some(true));
}

#[test]
fn terminal_close_refuses_last_live() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    let first_id = engine.active_terminal_id_for_test();
    engine
        .send_command_json(r#"{"cmd":"terminal_add"}"#)
        .expect("terminal_add");
    assert_eq!(engine.terminals_for_test().len(), 2);
    let second_id = engine.active_terminal_id_for_test();

    // Can close a non-last live terminal (including the first by index).
    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_close","terminal_id":"{first_id}"}}"#))
        .expect("terminal_close first");
    assert_eq!(engine.terminals_for_test().len(), 1);
    assert_eq!(engine.active_terminal_id_for_test(), second_id);

    // Cannot close the last live terminal.
    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_close","terminal_id":"{second_id}"}}"#))
        .expect("terminal_close last refused");
    assert_eq!(engine.terminals_for_test().len(), 1);
}

#[test]
fn terminal_move_reorders_and_tracks_active() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    let id0 = engine.active_terminal_id_for_test();
    engine.send_command_json(r#"{"cmd":"terminal_add"}"#).expect("add1");
    let id1 = engine.active_terminal_id_for_test();
    engine.send_command_json(r#"{"cmd":"terminal_add"}"#).expect("add2");
    let id2 = engine.active_terminal_id_for_test();
    assert_eq!(engine.terminals_for_test().len(), 3);

    // Active is id2 (newest). Move id0 to end.
    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_move","terminal_id":"{id0}","to_index":2}}"#))
        .expect("move");
    let ids: Vec<String> = engine.terminals_for_test().into_iter().map(|(id, _, _)| id).collect();
    assert_eq!(ids, vec![id1.clone(), id2.clone(), id0.clone()]);
    // active was id2 at index 2, after moving id0 from 0 to 2: id2 should still be active
    assert_eq!(engine.active_terminal_id_for_test(), id2);

    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_switch","terminal_id":"{id1}"}}"#))
        .expect("switch");
    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_move","terminal_id":"{id1}","to_index":2}}"#))
        .expect("move active");
    assert_eq!(engine.active_terminal_id_for_test(), id1);
    let ids: Vec<String> = engine.terminals_for_test().into_iter().map(|(id, _, _)| id).collect();
    assert_eq!(ids.last().unwrap(), &id1);
}

#[test]
fn terminal_rename_sets_label_and_ignores_empty_unknown() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    let id = engine.active_terminal_id_for_test();
    let auto_label = engine.terminals_for_test()[0].1.clone();

    engine
        .send_command_json(&format!(
            r#"{{"cmd":"terminal_rename","terminal_id":"{id}","name":"api"}}"#
        ))
        .expect("rename");
    assert_eq!(engine.terminals_for_test()[0].1, "api");

    // Empty / whitespace must not clear the custom title.
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"terminal_rename","terminal_id":"{id}","name":"   "}}"#
        ))
        .expect("empty rename");
    assert_eq!(engine.terminals_for_test()[0].1, "api");

    engine
        .send_command_json(
            r#"{"cmd":"terminal_rename","terminal_id":"missing-id","name":"other"}"#,
        )
        .expect("unknown id");
    assert_eq!(engine.terminals_for_test()[0].1, "api");

    // Custom title survives cwd changes (OSC 7 / auto label path).
    engine.active_terminal_mut().cwd = format!("{auto_label}-changed-cwd-path/other");
    assert_eq!(engine.terminals_for_test()[0].1, "api");
    assert_ne!(crate::terminal_state::cwd_label(&engine.active_terminal().cwd), "api");
}

#[test]
fn terminal_switch_marks_viewport_dirty() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    let first = engine.active_terminal_id_for_test();
    engine.send_command_json(r#"{"cmd":"terminal_add"}"#).expect("add");
    let second = engine.active_terminal_id_for_test();
    // Stop PTYs and disable follow so needs_render tracks viewport_dirty only.
    engine.send_command_json(r#"{"cmd":"stop"}"#).expect("stop");
    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_switch","terminal_id":"{first}"}}"#))
        .expect("switch first");
    engine.send_command_json(r#"{"cmd":"stop"}"#).expect("stop first");
    engine
        .send_command_json(r#"{"cmd":"set_follow","follow":false}"#)
        .expect("follow off");
    let _ = engine.render(800, 600, &mut vec![0u8; 800 * 600 * 4]);
    assert!(!engine.needs_render());

    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_switch","terminal_id":"{second}"}}"#))
        .expect("switch");
    assert!(engine.needs_render());

    engine
        .send_command_json(r#"{"cmd":"set_follow","follow":false}"#)
        .expect("follow off");
    let _ = engine.render(800, 600, &mut vec![0u8; 800 * 600 * 4]);
    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_switch","terminal_id":"{first}"}}"#))
        .expect("switch2");
    assert!(engine.needs_render());
}

#[test]
fn idle_running_follow_does_not_need_render() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine
        .send_command_json(r#"{"cmd":"set_follow","follow":true}"#)
        .expect("follow on");
    engine.mark_running_for_test();
    assert!(engine.auto_follow_for_test());

    // Settle initial flat/search rebuild so tick no longer reports a change.
    engine.rebuild_if_needed_for_test();

    let mut rgba = vec![0u8; 800 * 600 * 4];
    engine.render(800, 600, &mut rgba).expect("render clears dirty");
    assert!(!engine.needs_render());

    // No PTY activity, no focus/caret blink — tick must not force paint.
    engine.tick();
    assert!(
        !engine.needs_render(),
        "idle live shell with auto-follow must not perpetual-redraw"
    );
}

#[test]
fn stats_json_includes_terminals() {
    use crate::engine::Engine;
    use serde_json::Value;
    use std::thread;
    use std::time::Duration;

    let mut engine = Engine::new();
    engine.send_command_json(r#"{"cmd":"terminal_add"}"#).expect("add");
    while engine.poll_event_json().is_some() {}
    thread::sleep(Duration::from_millis(260));
    engine.tick();
    let mut stats = None;
    while let Some(ev) = engine.poll_event_json() {
        let v: Value = serde_json::from_str(&ev).unwrap();
        if v["type"] == "stats" {
            stats = Some(v);
        }
    }
    let parsed = stats.expect("stats event");
    assert!(parsed["terminals"].as_array().unwrap().len() >= 2);
    assert!(parsed["active_terminal"].as_u64().is_some());
    assert!(parsed["terminal_id"].as_str().is_some());
    assert!(parsed["has_active_terminal"].as_bool().unwrap());
}

#[test]
fn load_file_on_terminal_sets_log_file() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-test-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "line one").unwrap();
        writeln!(f, "line two").unwrap();
    }
    let mut engine = Engine::new();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    assert!(engine.buffer_record_count_for_test() >= 2);
    // load_file always opens a dedicated file session (live terminal stays).
    assert_eq!(engine.terminals_for_test().len(), 2);
    assert!(engine.active_is_file_session_for_test());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn load_file_creates_separate_terminal_when_session_used() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-file-term-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "alpha").unwrap();
        writeln!(f, "beta").unwrap();
    }
    let mut engine = Engine::new();
    // Simulate an interactive session that already started (must not be hijacked).
    engine.mark_active_process_started_for_test();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    assert_eq!(engine.terminals_for_test().len(), 2);
    assert_eq!(engine.active_terminal_index_for_test(), 1);
    assert!(engine.active_is_file_session_for_test());
    assert!(!engine.terminal_is_file_session_for_test(0));
    assert!(engine.buffer_record_count_for_test() >= 2);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn load_file_reopen_switches_to_existing_file_terminal() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-reopen-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "one").unwrap();
    }
    let mut engine = Engine::new();
    engine.mark_active_process_started_for_test();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    assert_eq!(engine.terminals_for_test().len(), 2);
    let file_id = engine.active_terminal_id_for_test();

    // Switch back to the first (interactive) terminal.
    let first_id = engine.terminals_for_test()[0].0.clone();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"terminal_switch","terminal_id":"{first_id}"}}"#
        ))
        .expect("switch");
    assert_eq!(engine.active_terminal_index_for_test(), 0);

    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("reopen");
    engine.finish_file_load_for_test();
    assert_eq!(engine.terminals_for_test().len(), 2);
    assert_eq!(engine.active_terminal_id_for_test(), file_id);
    assert!(engine.active_is_file_session_for_test());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_session_rejects_stdin_and_start() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-viewonly-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "line").unwrap();
    }
    let mut engine = Engine::new();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    assert!(engine.active_is_file_session_for_test());

    engine.handle_key(b"echo hi\n");
    assert!(!engine.active_terminal_running_for_test());

    engine
        .send_command_json(r#"{"cmd":"start","command":"true","args":[]}"#)
        .expect("start");
    assert!(engine.active_is_file_session_for_test());
    assert!(!engine.active_terminal_running_for_test());
    assert!(engine.status_message_for_test().contains("view-only"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn project_program_config_round_trip_yaml() {
    use crate::core::types::{
        LaunchConfig, ProgramConfig, ProjectConfig, ProjectsStore, WorkspaceConfig,
    };

    let store = ProjectsStore {
        active_project: 0,
        projects: vec![ProjectConfig {
            id: "p1".into(),
            name: "demo".into(),
            default_cwd: Some("/tmp".into()),
            path_hint: None,
            active_program: 0,
            programs: vec![
                ProgramConfig {
                    id: "a".into(),
                    name: "api".into(),
                    launch: LaunchConfig {
                        command: Some("npm".into()),
                        args: vec!["run".into(), "dev".into()],
                        cwd: Some("/tmp/app".into()),
                        ..Default::default()
                    },
                    workspace: WorkspaceConfig::default(),
                },
                ProgramConfig {
                    id: "b".into(),
                    name: "redis".into(),
                    launch: LaunchConfig {
                        command: Some("redis-server".into()),
                        cwd: Some("/usr".into()),
                        ..Default::default()
                    },
                    workspace: WorkspaceConfig::default(),
                },
            ],
        }],
    };
    let yaml = serde_yaml::to_string(&store).unwrap();
    let parsed: ProjectsStore = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.projects[0].programs.len(), 2);
    assert_eq!(parsed.projects[0].programs[0].launch.cwd.as_deref(), Some("/tmp/app"));
    assert_eq!(parsed.projects[0].programs[1].launch.cwd.as_deref(), Some("/usr"));
}

#[test]
fn program_display_name_from_launch() {
    use crate::core::types::{program_display_name, LaunchConfig};
    let launch = LaunchConfig {
        command: Some("npm".into()),
        args: vec!["run".into(), "develop".into()],
        ..Default::default()
    };
    assert_eq!(program_display_name(&launch), "npm run develop");
}

#[test]
fn selection_text_with_no_selection_returns_none() {
    use crate::engine::Engine;
    let engine = Engine::new();
    assert!(engine.selection_text_for_test().is_none());
}

#[test]
fn set_launch_starts_command_immediately() {
    use crate::core::types::LaunchConfig;
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine.set_launch(LaunchConfig {
        command: Some("sleep".into()),
        args: vec!["30".into()],
        ..LaunchConfig::default()
    });
    assert!(engine.process_started_for_test());
    assert!(engine.active_terminal_running_for_test());
    assert!(
        engine.status_message_for_test().starts_with("Running:"),
        "status={}",
        engine.status_message_for_test()
    );
    assert_eq!(engine.terminals_for_test().len(), 1);
    engine.tick();
    assert!(engine.active_terminal_running_for_test());
    assert_eq!(engine.terminals_for_test().len(), 1);
}

#[test]
fn stale_pty_exit_does_not_replace_cli_process() {
    use crate::core::types::LaunchConfig;
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine.set_launch(LaunchConfig {
        command: Some("sleep".into()),
        args: vec!["30".into()],
        ..LaunchConfig::default()
    });
    let id = engine.active_terminal_id_for_test();
    let gen = engine.pty_generation_for_test();
    assert!(gen >= 1);
    engine.inject_pty_exit_for_test(&id, 0, gen.wrapping_sub(1));
    engine.tick();
    assert!(engine.active_terminal_running_for_test());
    assert_eq!(engine.pty_generation_for_test(), gen);
    assert_eq!(engine.terminals_for_test().len(), 1);
    assert!(
        engine.status_message_for_test().starts_with("Running:"),
        "status={}",
        engine.status_message_for_test()
    );
}

#[test]
fn stats_split_terminals_and_files() {
    use crate::engine::Engine;
    use serde_json::Value;
    use std::io::Write;
    use std::thread;
    use std::time::Duration;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-stats-split-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "hello").unwrap();
    }
    let mut engine = Engine::new();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();

    while engine.poll_event_json().is_some() {}
    thread::sleep(Duration::from_millis(260));
    engine.tick();
    let mut stats = None;
    while let Some(ev) = engine.poll_event_json() {
        let v: Value = serde_json::from_str(&ev).unwrap();
        if v["type"] == "stats" {
            stats = Some(v);
        }
    }
    let parsed = stats.expect("stats");
    assert_eq!(parsed["terminals"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["files"].as_array().unwrap().len(), 1);
    assert!(parsed["is_file_session"].as_bool().unwrap());
    assert!(!parsed["auto_follow"].as_bool().unwrap());
    let file_name = path.file_name().unwrap().to_string_lossy();
    assert_eq!(parsed["tabs"][0]["name"].as_str().unwrap(), file_name.as_ref());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_session_ignores_set_follow() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-nofollow-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "line").unwrap();
    }
    let mut engine = Engine::new();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    engine
        .send_command_json(r#"{"cmd":"set_follow","follow":true}"#)
        .expect("set_follow");
    assert!(!engine.auto_follow_for_test());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn can_close_file_while_keeping_last_live() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-close-file-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "line").unwrap();
    }
    let mut engine = Engine::new();
    let live_id = engine.active_terminal_id_for_test();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    let file_id = engine.active_terminal_id_for_test();
    assert_ne!(live_id, file_id);

    engine
        .send_command_json(&format!(r#"{{"cmd":"terminal_close","terminal_id":"{file_id}"}}"#))
        .expect("close file");
    assert_eq!(engine.terminals_for_test().len(), 1);
    assert_eq!(engine.active_terminal_id_for_test(), live_id);
    assert!(!engine.active_is_file_session_for_test());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_scrollbar_mid_jump_loads_window_not_black() {
    use crate::engine::{Command, Engine};
    use std::io::Write;

    // Must be > FILE_LARGE_BYTES so only a sliding window is kept in memory.
    let path = std::env::temp_dir().join(format!(
        "noviewlog-scroll-mid-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        let pad = "x".repeat(100);
        for i in 0..200_000 {
            writeln!(f, "line-{i:06}-{pad}").unwrap();
        }
    }
    let mut engine = Engine::new();
    engine
        .send_command_json(r#"{"cmd":"resize","width":800,"height":400}"#)
        .expect("resize");
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    assert!(engine.active_is_file_session_for_test());

    let max = engine.max_scroll_offset_for_test();
    assert!(
        max > 1000.0,
        "whole-file max_scroll should be large, got {max}"
    );
    let mid = max * 0.5;
    engine
        .send_command(Command::Scroll { offset: mid })
        .expect("scroll mid");
    engine.finish_pending_file_window_for_test();
    engine.rebuild_if_needed_for_test();

    let start = engine.buffer_line_start_for_test();
    let end = engine.buffer_line_end_for_test();
    assert!(
        end > start,
        "window must be non-empty start={start} end={end}"
    );
    // Mid of a large file must not leave the viewport on the initial tail-only window forever.
    let total_ish = (max / engine.viewport_row_stride_for_test()) as u64;
    assert!(
        start < total_ish / 2 + 5_000 && end > total_ish / 2 - 5_000,
        "window should cover mid-file region, start={start} end={end} mid≈{}",
        total_ish / 2
    );
    let local = engine.scroll_offset_y_for_test();
    let local_max = engine.local_window_max_scroll_for_test();
    assert!(
        local <= local_max + 1.0,
        "local scroll must stay in the loaded window (local={local} max={local_max})"
    );

    let mut rgba = vec![0u8; 800 * 400 * 4];
    engine.render(800, 400, &mut rgba).expect("render");
    let lit = rgba.chunks_exact(4).filter(|px| px[0] | px[1] | px[2] > 0x20).count();
    assert!(
        lit > 200,
        "mid-file paint must show glyphs, not an empty/black frame (lit={lit})"
    );

    let (cur, total) = engine.viewport_line_position_for_test();
    assert!(total > 0, "line position total");
    assert!(
        cur > total / 4 && cur < total * 3 / 4,
        "mid scroll should report mid line position cur={cur} total={total}"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_scrollbar_reaches_eof() {
    use crate::engine::{Command, Engine};
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-scroll-eof-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        let pad = "x".repeat(100);
        for i in 0..200_000 {
            writeln!(f, "line-{i:06}-{pad}").unwrap();
        }
    }
    let mut engine = Engine::new();
    engine
        .send_command_json(r#"{"cmd":"resize","width":800,"height":400}"#)
        .expect("resize");
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();

    let total = engine.file_total_lines_for_test();
    let window = engine.file_view_window_lines_for_test() as u64;
    assert!(total > window * 2);

    let max = engine.max_scroll_offset_for_test();
    engine
        .send_command(Command::Scroll { offset: max })
        .expect("scroll eof");
    engine.finish_pending_file_window_for_test();
    engine.rebuild_if_needed_for_test();

    let start = engine.buffer_line_start_for_test();
    let expected_start = total.saturating_sub(window);
    assert_eq!(
        start, expected_start,
        "EOF scroll must pin the last window"
    );
    let local = engine.scroll_offset_y_for_test();
    let local_max = engine.local_window_max_scroll_for_test();
    assert!(
        (local - local_max).abs() < engine.viewport_row_stride_for_test(),
        "EOF local scroll should be at local max (local={local} max={local_max})"
    );
    let max_after = engine.max_scroll_offset_for_test();
    let global = engine.stats_scroll_y_for_test();
    assert!(
        (global - max_after).abs() < engine.viewport_row_stride_for_test() * 2.0,
        "stats scroll_y should stick at max after EOF (global={global} max={max_after})"
    );

    // Second nudge to max while already on last window.
    engine
        .send_command(Command::Scroll {
            offset: engine.max_scroll_offset_for_test(),
        })
        .expect("scroll eof again");
    engine.finish_pending_file_window_for_test();
    assert_eq!(engine.buffer_line_start_for_test(), expected_start);
    let (cur, tot) = engine.viewport_line_position_for_test();
    assert_eq!(tot, total);
    assert_eq!(
        cur, tot,
        "at EOF status must show N / N (bottom of viewport), got {cur} / {tot}"
    );

    let mut rgba = vec![0u8; 800 * 400 * 4];
    engine.render(800, 400, &mut rgba).expect("render");
    let lit = rgba.chunks_exact(4).filter(|px| px[0] | px[1] | px[2] > 0x20).count();
    assert!(lit > 200, "EOF paint must show content lit={lit}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn reload_file_picks_up_appended_lines() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-reload-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "first").unwrap();
    }
    let mut engine = Engine::new();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    assert!(engine.buffer_record_count_for_test() >= 1);

    {
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "second").unwrap();
    }
    engine
        .send_command_json(r#"{"cmd":"reload_file"}"#)
        .expect("reload_file");
    engine.finish_file_load_for_test();
    assert!(
        engine.buffer_record_count_for_test() >= 2,
        "reload must re-read appended lines"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn reload_missing_file_keeps_session() {
    use crate::engine::Engine;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-reload-missing-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "only").unwrap();
    }
    let mut engine = Engine::new();
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();
    let _ = std::fs::remove_file(&path);

    engine
        .send_command_json(r#"{"cmd":"reload_file"}"#)
        .expect("reload_file");
    assert!(engine.active_is_file_session_for_test());
    assert!(
        engine.status_message_for_test().contains("Failed to open"),
        "missing path must report status: {}",
        engine.status_message_for_test()
    );
}

