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
fn project_open_restores_stopped_terminals_and_tabs() {
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
    assert!(!live[0].2);
    assert!(!live[1].2);
    assert_eq!(live[0].1, "API");
    assert_eq!(live[1].1, "Worker");
    assert_eq!(engine.tab_configs_for_test().len(), 2);
    assert_eq!(engine.active_tab_index_for_test(), 1);
    assert_eq!(engine.active_view_name_for_test(), "Errors");
    assert!(engine.active_project.is_some());
}

#[test]
fn project_create_snapshots_and_reopen_restores() {
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
    engine.send_command_json(r#"{"cmd":"tab_add"}"#).expect("tab");
    engine
        .send_command_json(
            r#"{"cmd":"filter_add","type":"include","pattern":"warn","regex":false}"#,
        )
        .expect("filter");

    engine.terminal_add_blank_for_test();
    let tid2 = engine.active_terminal_id_for_test();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"program_set_launch","terminal_id":"{tid2}","command":"echo","args":["b"]}}"#
        ))
        .expect("launch2");
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"terminal_rename","terminal_id":"{tid2}","name":"Two"}}"#
        ))
        .expect("rename2");

    engine
        .send_command_json(r#"{"cmd":"project_create","name":"MyProj"}"#)
        .expect("create");
    assert_eq!(engine.projects.projects.len(), 1);
    assert_eq!(engine.projects.projects[0].programs.len(), 2);
    let pid = engine.projects.projects[0].id.clone();

    engine.terminal_add_blank_for_test();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"project_open","project_id":"{pid}"}}"#
        ))
        .expect("reopen");

    let live = engine.terminals_for_test();
    assert_eq!(live.len(), 2);
    assert!(!live[0].2 && !live[1].2);
    assert_eq!(live[0].1, "One");
    assert_eq!(live[1].1, "Two");
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"terminal_switch","terminal_id":"{}"}}"#,
            live[0].0
        ))
        .expect("switch");
    assert!(engine.tab_configs_for_test().len() >= 2);
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
fn startup_restores_last_project_stopped() {
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

    let live = engine.terminals_for_test();
    assert_eq!(live.len(), 2);
    assert!(!live[0].2 && !live[1].2);
    assert_eq!(live[0].1, "API");
    assert_eq!(live[1].1, "Worker");
    assert!(
        !engine.has_pty_for_test(&live[0].0) && !engine.has_pty_for_test(&live[1].0),
        "restored programs must stay stopped"
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
