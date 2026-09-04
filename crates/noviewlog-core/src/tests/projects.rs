//! Project / Program restore and Run/Stop lifecycle.

use crate::core::types::{
    FilterRule, FilterType, LaunchConfig, ProgramConfig, ProjectConfig, ProjectsStore, TabConfig,
    WorkspaceConfig,
};
use crate::engine::Engine;

fn engine_isolated() -> Engine {
    let mut engine = Engine::new();
    engine.skip_projects_persist = true;
    engine.projects = ProjectsStore::default();
    engine.active_project = None;
    engine
}

#[test]
fn project_open_selects_terminal_tab_and_auto_starts() {
    let mut engine = engine_isolated();
    engine.projects = ProjectsStore {
        projects: vec![ProjectConfig {
            id: "project-1".into(),
            name: "Demo".into(),
            default_cwd: None,
            path_hint: None,
            programs: vec![
                ProgramConfig {
                    id: "program-a".into(),
                    name: "API".into(),
                    launch: LaunchConfig {
                        command: Some("echo".into()),
                        args: vec!["hello".into()],
                        cwd: Some("/tmp".into()),
                        ..LaunchConfig::default()
                    },
                    workspace: WorkspaceConfig {
                        tabs: vec![
                            TabConfig {
                                name: "Terminal".into(),
                                filters: vec![],
                                search_query: String::new(),
                                search_regex: false,
                                search_case_sensitive: false,
                                search_whole_word: false,
                                auto_follow: true,
                                wrap_lines: true,
                            },
                            TabConfig {
                                name: "Errors".into(),
                                filters: vec![FilterRule {
                                    id: "f1".into(),
                                    name: None,
                                    filter_type: FilterType::Include,
                                    pattern: "error".into(),
                                    enabled: true,
                                    use_regex: false,
                                    regex: None,
                                }],
                                search_query: String::new(),
                                search_regex: false,
                                search_case_sensitive: false,
                                search_whole_word: false,
                                auto_follow: true,
                                wrap_lines: true,
                            },
                        ],
                        active_tab: 1,
                    },
                },
                ProgramConfig {
                    id: "program-b".into(),
                    name: "Worker".into(),
                    launch: LaunchConfig {
                        command: Some("sleep".into()),
                        args: vec!["1".into()],
                        ..LaunchConfig::default()
                    },
                    workspace: WorkspaceConfig::default(),
                },
            ],
            active_program: 0,
        }],
        active_project: 0,
    };

    engine
        .send_command_json(r#"{"cmd":"project_open","project_id":"project-1"}"#)
        .expect("open");

    let live = engine.terminals_for_test();
    assert_eq!(live.len(), 2);
    // echo/sleep may already have exited; auto-start is tracked via process_started.
    assert_eq!(live[0].1, "API");
    assert_eq!(live[1].1, "Worker");
    assert_eq!(engine.tab_configs_for_test().len(), 2);
    // Live TERMINALS restore always lands on Terminal (view 0), even when the
    // saved workspace had a filter tab active.
    assert_eq!(engine.active_tab_index_for_test(), 0);
    assert_eq!(engine.active_view_name_for_test(), "Terminal");
    assert!(
        engine.process_started_for_test(),
        "project open auto-starts the active process session"
    );
    assert!(engine.active_project.is_some());
}

#[test]
fn stopped_empty_viewport_messages_are_ascii_without_play_glyph() {
    use crate::engine::{EMPTY_FILTER_TAB_STOPPED, EMPTY_TERMINAL_TAB_STOPPED};
    for msg in [EMPTY_TERMINAL_TAB_STOPPED, EMPTY_FILTER_TAB_STOPPED] {
        assert!(
            !msg.contains('▶') && !msg.contains("Press ▶"),
            "empty hint must not use play glyph: {msg}"
        );
    }
    assert!(
        EMPTY_FILTER_TAB_STOPPED.contains("TERMINALS"),
        "filter-tab hint must point at the TERMINALS Start control"
    );
}

#[test]
fn project_create_starts_empty_and_does_not_copy_previous() {
    let mut engine = engine_isolated();
    engine
        .send_command_json(
            r#"{"cmd":"program_set_launch","command":"echo","args":["a"],"cwd":"/tmp"}"#,
        )
        .expect("launch");
    let tid = engine.active_terminal_id_for_test();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"terminal_rename","terminal_id":"{tid}","name":"One"}}"#
        ))
        .expect("rename");
    engine.terminal_add_blank_for_test();

    engine
        .send_command_json(r#"{"cmd":"project_create","name":"MyProj"}"#)
        .expect("create");
    assert_eq!(engine.projects.projects.len(), 1);
    assert!(
        engine.projects.projects[0].programs.is_empty(),
        "new Project must not snapshot live TERMINALS"
    );
    assert_eq!(engine.active_project, Some(0));

    let live = engine.terminals_for_test();
    assert_eq!(live.len(), 1, "empty Project opens as one Terminal");
    assert_ne!(live[0].1, "One");
    // Blank live terminal auto-starts an interactive shell.
    assert!(
        live[0].2 || engine.has_pty_for_test(&live[0].0),
        "empty Project Terminal should be running without Start"
    );
    assert!(
        engine
            .status_message_for_test()
            .contains("Created project: MyProj")
    );
}

#[test]
fn exit_with_launch_command_does_not_respawn_shell() {
    let mut engine = engine_isolated();
    engine
        .send_command_json(r#"{"cmd":"program_set_launch","command":"true"}"#)
        .expect("launch");
    let id = engine.active_terminal_id_for_test();
    // Pretend a PTY was running with generation 1.
    engine.set_pty_generation_for_test(1);
    engine.mark_running_for_test();
    engine.inject_pty_exit_for_test(&id, 0, 1);
    engine.poll_pty_for_test();

    assert!(!engine.active_terminal_running_for_test());
    assert!(
        !engine.has_pty_for_test(&id),
        "must not respawn interactive shell when launch.command is set"
    );
}

#[test]
fn typing_after_program_exit_does_not_spawn_shell() {
    let mut engine = engine_isolated();
    engine
        .send_command_json(r#"{"cmd":"program_set_launch","command":"uname","args":["-a"]}"#)
        .expect("launch");
    engine.push_lines_for_test([
        "Linux Dima-PC uname-a-output".into(),
        "trailing-line".into(),
    ]);
    let id = engine.active_terminal_id_for_test();
    engine.set_pty_generation_for_test(1);
    engine.mark_running_for_test();
    engine.inject_pty_exit_for_test(&id, 0, 1);
    engine.poll_pty_for_test();
    assert!(!engine.active_terminal_running_for_test());

    engine.handle_key(b"\r");
    engine.poll_pty_for_test();

    assert!(
        !engine.active_terminal_running_for_test(),
        "Enter after a Program exits must not start a shell"
    );
    assert!(
        !engine.has_pty_for_test(&id),
        "Enter after a Program exits must not create a PTY"
    );
    let texts = engine.flat_line_texts_for_test();
    let uname_hits = texts
        .iter()
        .filter(|l| l.contains("uname-a-output"))
        .count();
    assert_eq!(
        uname_hits, 1,
        "finished Program output must not be duplicated, got {texts:?}"
    );
}

#[test]
fn program_start_clears_previous_scrollback() {
    let mut engine = engine_isolated();
    #[cfg(windows)]
    engine
        .send_command_json(
            r#"{"cmd":"program_set_launch","command":"cmd","args":["/c","echo","ok"]}"#,
        )
        .expect("launch");
    #[cfg(not(windows))]
    engine
        .send_command_json(r#"{"cmd":"program_set_launch","command":"true"}"#)
        .expect("launch");
    engine.push_lines_for_test(["OLD-BANNER-LINE".into(), "second-line".into()]);
    assert!(
        engine.buffer_record_count_for_test() > 0,
        "precondition: leftover scrollback in the record buffer"
    );
    engine
        .send_command_json(r#"{"cmd":"terminal_start"}"#)
        .expect("start");
    let leftover = engine
        .active_terminal()
        .buffer
        .raw_lines()
        .iter()
        .any(|l| l.contains("OLD-BANNER-LINE"));
    assert!(
        !leftover,
        "Start must drop the previous session, got {:?}",
        engine.active_terminal().buffer.raw_lines()
    );
}

#[test]
fn projects_store_yaml_round_trip() {
    let store = ProjectsStore {
        projects: vec![ProjectConfig {
            id: "project-rt".into(),
            name: "Round".into(),
            default_cwd: None,
            path_hint: None,
            programs: vec![ProgramConfig {
                id: "program-rt".into(),
                name: "Main".into(),
                launch: LaunchConfig {
                    command: Some("npm".into()),
                    args: vec!["run".into(), "dev".into()],
                    cwd: Some("/home/me/app".into()),
                    ..LaunchConfig::default()
                },
                workspace: WorkspaceConfig {
                    tabs: vec![TabConfig {
                        name: "Terminal".into(),
                        filters: vec![],
                        search_query: String::new(),
                        search_regex: false,
                        search_case_sensitive: false,
                        search_whole_word: false,
                        auto_follow: true,
                        wrap_lines: true,
                    }],
                    active_tab: 0,
                },
            }],
            active_program: 0,
        }],
        active_project: 0,
    };
    let yaml = serde_yaml::to_string(&store).expect("to yaml");
    let parsed: ProjectsStore = serde_yaml::from_str(&yaml).expect("from yaml");
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(
        parsed.projects[0].programs[0].launch.command.as_deref(),
        Some("npm")
    );
    assert_eq!(
        parsed.projects[0].programs[0].launch.args,
        vec!["run", "dev"]
    );
}

fn sample_project(id: &str, name: &str) -> ProjectConfig {
    ProjectConfig {
        id: id.into(),
        name: name.into(),
        default_cwd: None,
        path_hint: None,
        programs: vec![
            ProgramConfig {
                id: "program-a".into(),
                name: "API".into(),
                launch: LaunchConfig {
                    command: Some("echo".into()),
                    args: vec!["hello".into()],
                    ..LaunchConfig::default()
                },
                workspace: WorkspaceConfig::default(),
            },
            ProgramConfig {
                id: "program-b".into(),
                name: "Worker".into(),
                launch: LaunchConfig {
                    command: Some("sleep".into()),
                    args: vec!["1".into()],
                    ..LaunchConfig::default()
                },
                workspace: WorkspaceConfig::default(),
            },
        ],
        active_program: 0,
    }
}

#[test]
fn startup_restores_last_project_and_auto_starts() {
    let mut engine = engine_isolated();
    engine.projects = ProjectsStore {
        projects: vec![
            sample_project("project-1", "First"),
            sample_project("project-2", "Second"),
        ],
        active_project: 1,
    };

    let restored = engine.finish_startup(LaunchConfig::default());
    assert!(restored);
    assert_eq!(engine.active_project, Some(1));
    assert_eq!(engine.active_terminal_index_for_test(), 0);
    assert_eq!(engine.active_tab_index_for_test(), 0);

    let live = engine.terminals_for_test();
    assert_eq!(live.len(), 2);
    assert_eq!(live[0].1, "API");
    assert_eq!(live[1].1, "Worker");
    assert!(
        engine.process_started_for_test(),
        "restored programs auto-start without pressing Start"
    );
    assert!(
        engine
            .status_message_for_test()
            .contains("Opened project: Second")
    );
}

#[test]
fn startup_cli_launch_skips_project_restore() {
    let mut engine = engine_isolated();
    engine.projects = ProjectsStore {
        projects: vec![sample_project("project-1", "Demo")],
        active_project: 0,
    };

    let restored = engine.finish_startup(LaunchConfig {
        log_file: Some("/tmp/access.log".into()),
        ..LaunchConfig::default()
    });
    assert!(!restored);
    assert!(engine.active_project.is_none());
    assert!(engine.active_is_file_session_for_test());
    assert_eq!(engine.terminals_for_test().len(), 1);
}

#[test]
fn startup_no_projects_keeps_boot_terminal() {
    let mut engine = engine_isolated();
    engine.projects = ProjectsStore::default();

    let restored = engine.finish_startup(LaunchConfig::default());
    assert!(!restored);
    assert!(engine.active_project.is_none());
    assert_eq!(engine.terminals_for_test().len(), 1);
    assert!(!engine.active_terminal_running_for_test());
}

#[test]
fn active_project_saves_and_restores_file_sessions() {
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "noviewlog-proj-file-{}.log",
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "nginx line").unwrap();
    }
    let mut engine = engine_isolated();
    engine
        .send_command_json(r#"{"cmd":"project_create","name":"Logs"}"#)
        .expect("create");
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_str}"}}"#))
        .expect("load_file");
    engine.finish_file_load_for_test();

    let programs = &engine.projects.projects[0].programs;
    assert!(
        programs.iter().any(|p| p.launch.log_file.as_deref() == Some(path.to_str().unwrap())
            || p.launch.log_file.as_ref().is_some_and(|s| s.replace('\\', "/") == path.to_string_lossy().replace('\\', "/"))),
        "open file must snapshot log_file onto the active Project: {:?}",
        programs.iter().map(|p| p.launch.log_file.clone()).collect::<Vec<_>>()
    );

    let project_id = engine.projects.projects[0].id.clone();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"project_open","project_id":"{project_id}"}}"#
        ))
        .expect("reopen");

    let files = engine.file_session_ids_for_test();
    assert_eq!(files.len(), 1);
    assert_eq!(engine.file_session_paths_for_test().len(), 1);
    assert!(!engine.active_is_file_session_for_test());

    engine
        .send_command_json(&format!(
            r#"{{"cmd":"terminal_switch","terminal_id":"{}"}}"#,
            files[0]
        ))
        .expect("switch");
    engine.finish_file_load_for_test();
    assert!(engine.active_is_file_session_for_test());
    assert!(engine.file_backed_for_test());
    assert!(engine.buffer_record_count_for_test() >= 1);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn project_open_replaces_leftover_files() {
    use std::io::Write;

    let path_a = std::env::temp_dir().join(format!(
        "noviewlog-proj-a-{}.log",
        std::process::id()
    ));
    let path_b = std::env::temp_dir().join(format!(
        "noviewlog-proj-b-{}.log",
        std::process::id()
    ));
    for p in [&path_a, &path_b] {
        let mut f = std::fs::File::create(p).unwrap();
        writeln!(f, "x").unwrap();
    }
    let mut engine = engine_isolated();
    engine
        .send_command_json(r#"{"cmd":"project_create","name":"A"}"#)
        .expect("create A");
    let id_a = engine.projects.projects[0].id.clone();
    let path_a_str = path_a.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_a_str}"}}"#))
        .expect("load A");
    engine.finish_file_load_for_test();

    engine
        .send_command_json(r#"{"cmd":"project_create","name":"B"}"#)
        .expect("create B");
    let path_b_str = path_b.to_string_lossy().replace('\\', "\\\\");
    engine
        .send_command_json(&format!(r#"{{"cmd":"load_file","path":"{path_b_str}"}}"#))
        .expect("load B");
    engine.finish_file_load_for_test();
    assert_eq!(engine.file_session_ids_for_test().len(), 1);

    engine
        .send_command_json(&format!(r#"{{"cmd":"project_open","project_id":"{id_a}"}}"#))
        .expect("open A");
    let restored = engine.file_session_paths_for_test();
    assert_eq!(restored.len(), 1);
    let restored_norm = restored[0].replace('\\', "/");
    let expect_a = path_a.to_string_lossy().replace('\\', "/");
    assert_eq!(restored_norm, expect_a);

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

#[test]
fn program_set_launch_keeps_wsl_through_project_open() {
    let mut engine = engine_isolated();
    engine
        .send_command_json(r#"{"cmd":"project_create","name":"WslProj"}"#)
        .expect("create");
    engine
        .send_command_json(
            r#"{"cmd":"program_set_launch","command":"uname","args":["-a"],"cwd":"/home/me","wsl":true,"wsl_distro":"Ubuntu"}"#,
        )
        .expect("launch");

    let launch = &engine.active_terminal().launch;
    assert!(launch.wsl, "Edit Launch must keep wsl");
    assert_eq!(launch.command.as_deref(), Some("uname"));
    assert_eq!(launch.args, vec!["-a".to_string()]);
    assert_eq!(launch.cwd.as_deref(), Some("/home/me"));
    assert_eq!(launch.wsl_distro.as_deref(), Some("Ubuntu"));

    let stored = &engine.projects.projects[0].programs[0].launch;
    assert!(stored.wsl);
    assert_eq!(stored.wsl_distro.as_deref(), Some("Ubuntu"));

    let project_id = engine.projects.projects[0].id.clone();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"project_open","project_id":"{project_id}"}}"#
        ))
        .expect("reopen");
    let restored = &engine.active_terminal().launch;
    assert!(restored.wsl);
    assert_eq!(restored.command.as_deref(), Some("uname"));
    assert_eq!(restored.wsl_distro.as_deref(), Some("Ubuntu"));
    assert_eq!(restored.cwd.as_deref(), Some("/home/me"));
}
