use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::core::formats::builtin_format_presets;
use crate::core::types::{
    clamp_max_scrollback_lines, clamp_viewport_font_size, compile_filter, AppConfig, FilterRule,
    ProjectConfig, ProjectsStore, TabConfig, WorkspaceConfig, DEFAULT_MAX_SCROLLBACK_LINES,
    DEFAULT_VIEWPORT_FONT_SIZE,
};

const BUNDLED_PRESET_YAML: &str = include_str!("../../../../presets/defaults.yaml");

pub fn load_bundled_config() -> AppConfig {
    merge_config_sources(&[parse_yaml_config(BUNDLED_PRESET_YAML)])
}

pub fn load_config_from_yaml(yaml_text: &str) -> AppConfig {
    let mut sources = vec![load_bundled_config()];
    sources.push(parse_yaml_config(yaml_text));
    merge_config_sources(&sources)
}

pub fn load_user_config() -> Option<AppConfig> {
    let path = user_config_path().ok()?;
    migrate_legacy_config_if_needed(&path);
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path)
        .ok()
        .map(|text| load_config_from_yaml(&text))
}

pub fn save_user_config(config: &AppConfig) -> Result<(), String> {
    let path = user_config_path()?;
    migrate_legacy_config_if_needed(&path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let yaml = serde_yaml::to_string(config).map_err(|e| e.to_string())?;
    write_private_file(&path, yaml.as_bytes())
}

pub fn user_config_path() -> Result<PathBuf, String> {
    config_dir()
        .map(|dir| dir.join("config.yaml"))
        .ok_or_else(|| "no home dir".to_string())
}

pub fn projects_path() -> Result<PathBuf, String> {
    config_dir()
        .map(|dir| dir.join("projects.yaml"))
        .ok_or_else(|| "no home dir".to_string())
}

pub fn load_projects_store() -> ProjectsStore {
    let path = match projects_path() {
        Ok(p) => p,
        Err(_) => return ProjectsStore::default(),
    };
    if !path.exists() {
        return ProjectsStore::default();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_yaml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_projects_store(store: &ProjectsStore) -> Result<(), String> {
    let path = projects_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let yaml = serde_yaml::to_string(store).map_err(|e| e.to_string())?;
    write_private_file(&path, yaml.as_bytes())
}

/// Write `data` to `path`, using mode `0o600` on Unix (owner read/write only).
fn write_private_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts.open(path).map_err(|e| e.to_string())?;
    file.write_all(data).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    // mode() only applies on create; tighten perms if the file already existed.
    #[cfg(unix)]
    {
        let mut perms = file.metadata().map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn project_config_to_store(projects: &[ProjectConfig], active_project: usize) -> ProjectsStore {
    ProjectsStore {
        projects: projects.to_vec(),
        active_project,
    }
}

pub fn program_workspace_snapshot(
    tabs: &[TabConfig],
    active_tab: usize,
) -> WorkspaceConfig {
    views_to_workspace(tabs, active_tab)
}

fn config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("noviewlog"))
}

/// Expand a leading `~` or `~/` to the user's home directory.
pub fn expand_path(path: &str) -> String {
    if path == "~" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir()
            .map(|h| h.join(rest).to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
    }
    path.to_string()
}

pub fn expand_path_opt(path: Option<String>) -> Option<String> {
    path.map(|p| expand_path(&p))
}

fn legacy_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("nolog").join("config.yaml"))
}

fn migrate_legacy_config_if_needed(new_path: &PathBuf) {
    if new_path.exists() {
        return;
    }
    let Some(legacy_path) = legacy_config_path() else {
        return;
    };
    if !legacy_path.exists() {
        return;
    }
    if let Some(parent) = new_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::copy(&legacy_path, new_path);
}

fn parse_yaml_config(yaml_text: &str) -> AppConfig {
    serde_yaml::from_str(yaml_text).unwrap_or_default()
}

pub fn merge_config_sources(sources: &[AppConfig]) -> AppConfig {
    let mut merged = AppConfig {
        default_format: "node-default".to_string(),
        default_preset: "node-dev".to_string(),
        max_scrollback_lines: DEFAULT_MAX_SCROLLBACK_LINES,
        viewport_font_size: DEFAULT_VIEWPORT_FONT_SIZE,
        terminals_section_expanded: true,
        files_section_expanded: true,
        formats: HashMap::new(),
        presets: HashMap::new(),
        workspaces: HashMap::new(),
    };

    for source in sources {
        if source.default_format.is_empty() {
            continue;
        }
        merged.default_format = source.default_format.clone();
        merged.default_preset = source.default_preset.clone();
        merged.max_scrollback_lines = clamp_max_scrollback_lines(source.max_scrollback_lines);
        merged.viewport_font_size = clamp_viewport_font_size(source.viewport_font_size);
        merged.terminals_section_expanded = source.terminals_section_expanded;
        merged.files_section_expanded = source.files_section_expanded;
        merged.formats.extend(source.formats.clone());
        merged.presets.extend(source.presets.clone());
        merged.workspaces.extend(source.workspaces.clone());
    }

    merged.max_scrollback_lines = clamp_max_scrollback_lines(merged.max_scrollback_lines);
    merged.viewport_font_size = clamp_viewport_font_size(merged.viewport_font_size);
    merged
}

pub fn workspace_key(cwd: Option<&str>) -> String {
    cwd.and_then(|p| fs::canonicalize(p).ok())
        .or_else(|| cwd.map(PathBuf::from))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "_no_cwd".to_string())
}

pub fn compile_tab_config(tab: TabConfig) -> TabConfig {
    TabConfig {
        name: tab.name,
        filters: tab.filters.into_iter().map(compile_filter).collect(),
        search_query: tab.search_query,
        search_regex: tab.search_regex,
        search_case_sensitive: tab.search_case_sensitive,
        search_whole_word: tab.search_whole_word,
        auto_follow: tab.auto_follow,
        wrap_lines: tab.wrap_lines,
    }
}

pub fn tab_config_from_runtime(name: &str, filters: Vec<FilterRule>) -> TabConfig {
    TabConfig {
        name: name.to_string(),
        filters,
        search_query: String::new(),
        search_regex: false,
        search_case_sensitive: false,
        search_whole_word: false,
        auto_follow: true,
        wrap_lines: true,
    }
}

pub fn workspace_to_tab_configs(workspace: &WorkspaceConfig) -> Vec<TabConfig> {
    workspace
        .tabs
        .iter()
        .cloned()
        .map(compile_tab_config)
        .collect()
}

pub fn views_to_workspace(tabs: &[TabConfig], active_tab: usize) -> WorkspaceConfig {
    WorkspaceConfig {
        tabs: tabs
            .iter()
            .map(|t| TabConfig {
                name: t.name.clone(),
                filters: t.filters.clone(),
                search_query: t.search_query.clone(),
                search_regex: t.search_regex,
                search_case_sensitive: t.search_case_sensitive,
                search_whole_word: t.search_whole_word,
                auto_follow: t.auto_follow,
                wrap_lines: t.wrap_lines,
            })
            .collect(),
        active_tab,
    }
}

pub fn load_preset(config: &AppConfig, preset_name: &str) -> Vec<FilterRule> {
    let preset = config.presets.get(preset_name);
    match preset {
        Some(p) => compile_preset_filters(&p.filters),
        None => Vec::new(),
    }
}

pub fn compile_preset_filters(filters: &[FilterRule]) -> Vec<FilterRule> {
    filters.iter().cloned().map(compile_filter).collect()
}

pub struct RuntimeConfig {
    pub format_id: String,
    pub filters: Vec<FilterRule>,
}

pub fn build_runtime_config(config: &AppConfig, preset_name: Option<&str>) -> RuntimeConfig {
    let name = preset_name.unwrap_or(&config.default_preset);
    let filters = load_preset(config, name);
    RuntimeConfig {
        format_id: config.default_format.clone(),
        filters,
    }
}

pub fn all_format_presets(config: &AppConfig) -> HashMap<String, crate::core::types::FormatPreset> {
    let mut presets = builtin_format_presets();
    presets.extend(config.formats.clone());
    presets
}

impl Default for AppConfig {
    fn default() -> Self {
        load_bundled_config()
    }
}
