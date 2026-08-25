use crate::core::filter::FilterEngine;
use crate::core::formats::get_builtin_format;
use crate::core::parser::{reparse_lines, RecordParser};
use crate::core::types::{compile_filter, FilterRule, FilterType, LogLevel, LogRecord};
use super::sample_records;

#[test]
fn groups_stack_trace_into_single_record() {
    let mut parser = RecordParser::new(get_builtin_format("node-default"));
    assert!(parser.push_line("Error: something failed".to_string()).is_empty());
    assert!(parser
        .push_line("    at Object.<anonymous> (/app/index.js:10:5)".to_string())
        .is_empty());
    assert!(parser
        .push_line("    at Module._compile (node:internal/modules/cjs/loader:1376:14)".to_string())
        .is_empty());

    let last = parser.flush_pending().expect("pending record");
    assert_eq!(last.lines.len(), 3);
    assert!(last.text.contains("Error: something failed"));
    assert_eq!(last.level, Some(LogLevel::Error));
}

#[test]
fn starts_new_record_on_timestamp_line() {
    let mut parser = RecordParser::new(get_builtin_format("node-default"));
    parser.push_line("2024-01-01T10:00:00.000Z info: first".to_string());
    let records = parser.push_line("2024-01-01T10:00:01.000Z info: second".to_string());
    assert_eq!(records.len(), 1);
    assert!(records[0].lines[0].contains("first"));
}

#[test]
fn parses_pino_json_one_line_per_record() {
    let mut parser = RecordParser::new(get_builtin_format("pino"));
    assert!(parser.push_line(r#"{"level":30,"msg":"hello"}"#.to_string()).is_empty());
    let records = parser.push_line(r#"{"level":50,"msg":"error"}"#.to_string());
    assert_eq!(records.len(), 1);
    assert!(records[0].text.contains("hello"));
}

#[test]
fn reparse_lines_rebuilds_buffer() {
    let lines = vec![
        "Error: fail".to_string(),
        "    at foo.js:1:1".to_string(),
        "[Nest] INFO started".to_string(),
    ];
    let records = reparse_lines(&lines, get_builtin_format("node-default"));
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].lines.len(), 2);
    assert!(records[1].lines[0].contains("[Nest]"));
}

#[test]
fn excludes_matching_records() {
    let engine = FilterEngine::new(vec![compile_filter(FilterRule {
        id: "x".to_string(),
        name: None,
        filter_type: FilterType::Exclude,
        pattern: "deprecated".to_string(),
        enabled: true,
        use_regex: true,
        regex: None,
    })]);
    let records = sample_records();
    let visible: Vec<u64> = engine
        .filter_records(&records)
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(visible, vec![2, 3]);
}

#[test]
fn includes_only_matching_when_include_active() {
    let engine = FilterEngine::new(vec![compile_filter(FilterRule {
        id: "x".to_string(),
        name: None,
        filter_type: FilterType::Include,
        pattern: "Error".to_string(),
        enabled: true,
        use_regex: true,
        regex: None,
    })]);
    let records = sample_records();
    let visible: Vec<u64> = engine
        .filter_records(&records)
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(visible, vec![2]);
}

#[test]
fn exclude_wins_over_include() {
    let engine = FilterEngine::new(vec![
        compile_filter(FilterRule {
            id: "i".to_string(),
            name: None,
            filter_type: FilterType::Include,
            pattern: "Error|warn".to_string(),
            enabled: true,
            use_regex: true,
            regex: None,
        }),
        compile_filter(FilterRule {
            id: "e".to_string(),
            name: None,
            filter_type: FilterType::Exclude,
            pattern: "deprecated".to_string(),
            enabled: true,
            use_regex: true,
            regex: None,
        }),
    ]);
    let records = sample_records();
    let visible: Vec<u64> = engine
        .filter_records(&records)
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(visible, vec![2]);
}

#[test]
fn filter_literal_mode_does_not_treat_dot_as_wildcard() {
    let literal = FilterEngine::new(vec![compile_filter(FilterRule {
        id: "lit".to_string(),
        name: None,
        filter_type: FilterType::Include,
        pattern: "foo.bar".to_string(),
        enabled: true,
        use_regex: false,
        regex: None,
    })]);
    let regex = FilterEngine::new(vec![compile_filter(FilterRule {
        id: "re".to_string(),
        name: None,
        filter_type: FilterType::Include,
        pattern: "foo.bar".to_string(),
        enabled: true,
        use_regex: true,
        regex: None,
    })]);
    let records = vec![
        LogRecord {
            id: 1,
            lines: vec!["foo.bar".into()],
            text: "foo.bar".into(),
            received_at: chrono::Utc::now(),
            level: None,
            overwrite: false,
        },
        LogRecord {
            id: 2,
            lines: vec!["fooxbar".into()],
            text: "fooxbar".into(),
            received_at: chrono::Utc::now(),
            level: None,
            overwrite: false,
        },
    ];
    let lit_ids: Vec<u64> = literal
        .filter_records(&records)
        .into_iter()
        .map(|r| r.id)
        .collect();
    let re_ids: Vec<u64> = regex
        .filter_records(&records)
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(lit_ids, vec![1]);
    assert_eq!(re_ids, vec![1, 2]);
}

#[test]
fn filter_add_regex_false_and_omitted_default() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine
        .send_command_json(r#"{"cmd":"tab_add"}"#)
        .expect("tab_add");
    engine
        .send_command_json(
            r#"{"cmd":"filter_add","type":"include","pattern":"foo.bar","regex":false}"#,
        )
        .expect("filter_add literal");
    engine
        .send_command_json(r#"{"cmd":"filter_add","type":"exclude","pattern":"warn"}"#)
        .expect("filter_add default regex");

    let cfg = engine.active_tab_config_for_test();
    assert_eq!(cfg.filters.len(), 2);
    assert!(!cfg.filters[0].use_regex);
    assert_eq!(cfg.filters[0].pattern, "foo.bar");
    assert!(cfg.filters[1].use_regex);

    // Legacy YAML without use_regex → true
    let yaml = r#"
id: legacy
name: null
type: include
pattern: Error
enabled: true
"#;
    let parsed: FilterRule = serde_yaml::from_str(yaml).unwrap();
    assert!(parsed.use_regex);
}

#[test]
fn filter_update_keeps_id_type_regex() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine
        .send_command_json(r#"{"cmd":"tab_add"}"#)
        .expect("tab_add");
    engine
        .send_command_json(
            r#"{"cmd":"filter_add","type":"include","pattern":"foo.bar","regex":false}"#,
        )
        .expect("filter_add");

    let id = engine.active_tab_config_for_test().filters[0].id.clone();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"filter_update","id":"{id}","pattern":"baz.qux"}}"#
        ))
        .expect("filter_update");

    let cfg = engine.active_tab_config_for_test();
    assert_eq!(cfg.filters.len(), 1);
    assert_eq!(cfg.filters[0].id, id);
    assert_eq!(cfg.filters[0].pattern, "baz.qux");
    assert!(!cfg.filters[0].use_regex);
    assert_eq!(cfg.filters[0].filter_type, FilterType::Include);
    assert!(cfg.filters[0].enabled);
}

#[test]
fn filter_update_console_and_empty_are_noop() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine
        .send_command_json(r#"{"cmd":"filter_update","id":"missing","pattern":"foo"}"#)
        .expect("filter_update on console");
    assert!(
        engine.active_tab_config_for_test().filters.is_empty(),
        "Console tab must not accept filter_update"
    );

    engine
        .send_command_json(r#"{"cmd":"tab_add"}"#)
        .expect("tab_add");
    engine
        .send_command_json(
            r#"{"cmd":"filter_add","type":"exclude","pattern":"warn","regex":true}"#,
        )
        .expect("filter_add");

    let before = engine.active_tab_config_for_test();
    let id = before.filters[0].id.clone();
    engine
        .send_command_json(&format!(
            r#"{{"cmd":"filter_update","id":"{id}","pattern":""}}"#
        ))
        .expect("filter_update empty");
    engine
        .send_command_json(r#"{"cmd":"filter_update","id":"no-such","pattern":"other"}"#)
        .expect("filter_update unknown id");

    let after = engine.active_tab_config_for_test();
    assert_eq!(after.filters.len(), 1);
    assert_eq!(after.filters[0].id, id);
    assert_eq!(after.filters[0].pattern, "warn");
    assert_eq!(after.filters[0].filter_type, FilterType::Exclude);
    assert!(after.filters[0].use_regex);
}

#[test]
fn set_format_reparses_entire_buffer() {
    use crate::engine::Engine;

    let mut engine = Engine::new();
    engine.push_lines_for_test([
        "Error: fail".into(),
        "    at foo.js:1:1".into(),
        "[Nest] INFO started".into(),
    ]);

    engine
        .send_command_json(r#"{"cmd":"set_format","format_id":"raw"}"#)
        .expect("set_format raw");
    assert_eq!(
        engine.buffer_record_count_for_test(),
        3,
        "raw format must treat each line as its own record"
    );

    engine
        .send_command_json(r#"{"cmd":"set_format","format_id":"node-default"}"#)
        .expect("set_format node-default");
    assert_eq!(
        engine.buffer_record_count_for_test(),
        2,
        "node-default must regroup stack frames into one record"
    );
}


