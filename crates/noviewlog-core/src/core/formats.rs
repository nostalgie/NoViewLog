use std::collections::HashMap;

use crate::core::types::{compile_regex, FormatPreset, LogFormat};

pub fn builtin_format_presets() -> HashMap<String, FormatPreset> {
    HashMap::from([
        (
            "node-default".to_string(),
            FormatPreset {
                start: r#"^(\d{4}-|\[|\{|"level"|Error:|\[Nest\]|\[Express\]|\s*>\s)"#.to_string(),
                continuation: vec![
                    r"^\s+at ".to_string(),
                    r"^Caused by:".to_string(),
                    r"^\s+\^".to_string(),
                    r"^\s+~".to_string(),
                    r"^\s*$".to_string(),
                    r"^\s+\.{3}".to_string(),
                ],
            },
        ),
        (
            "pino".to_string(),
            FormatPreset {
                start: r"^\{".to_string(),
                continuation: vec![],
            },
        ),
        (
            "raw".to_string(),
            FormatPreset {
                start: r".*".to_string(),
                continuation: vec![],
            },
        ),
    ])
}

pub fn create_log_format(id: &str, preset: &FormatPreset) -> LogFormat {
    LogFormat {
        id: id.to_string(),
        name: id.to_string(),
        start: preset.start.clone(),
        continuation: preset.continuation.clone(),
        start_regex: Some(compile_regex(&preset.start)),
        continuation_regexes: preset
            .continuation
            .iter()
            .map(|p| compile_regex(p))
            .collect(),
    }
}

pub fn get_builtin_format(id: &str) -> LogFormat {
    let presets = builtin_format_presets();
    let preset = presets
        .get(id)
        .or_else(|| presets.get("node-default"))
        .expect("node-default format exists");
    create_log_format(id, preset)
}

pub fn merge_formats(
    builtin: &HashMap<String, FormatPreset>,
    custom: &HashMap<String, FormatPreset>,
) -> HashMap<String, LogFormat> {
    let mut merged = builtin.clone();
    merged.extend(custom.clone());
    merged
        .into_iter()
        .map(|(id, preset)| {
            let format = create_log_format(&id, &preset);
            (id, format)
        })
        .collect()
}
