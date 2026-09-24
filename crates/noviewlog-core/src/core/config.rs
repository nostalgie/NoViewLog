use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::core::formats::builtin_format_presets;
use crate::core::types::{
    clamp_max_scrollback_lines, clamp_viewport_font_size, compile_filter, AppConfig, FilterRule,
    ProjectConfig, ProjectsStore, ShellPreference, TabConfig, WorkspaceConfig,
    DEFAULT_MAX_SCROLLBACK_LINES, DEFAULT_VIEWPORT_FONT_SIZE,
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

/// Load the user `config.yaml`. On read/parse failure the corrupt file is
/// preserved as `config.yaml.corrupt` and a status message is returned instead
/// of silently resetting to defaults (issue #49).
pub fn load_user_config() -> (Option<AppConfig>, Option<String>) {
    let path = match user_config_path() {
        Ok(p) => p,
        Err(err) => return (None, Some(err)),
    };
    load_user_config_from(&path)
}

fn load_user_config_from(path: &Path) -> (Option<AppConfig>, Option<String>) {
    migrate_legacy_config_if_needed(path);
    if !path.exists() {
        return (None, None);
    }
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => return (None, quarantine_corrupt_file(path, &err.to_string())),
    };
    match serde_yaml::from_str::<AppConfig>(&text) {
        Ok(user) => (
            Some(merge_config_sources(&[load_bundled_config(), user])),
            None,
        ),
        Err(err) => (None, quarantine_corrupt_file(path, &err.to_string())),
    }
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

/// Load `projects.yaml`. On read/parse failure the corrupt file is preserved
/// as `projects.yaml.corrupt` and a status message is returned; the in-memory
/// store starts empty but the original bytes are never silently discarded
/// (issue #49).
pub fn load_projects_store() -> (ProjectsStore, Option<String>) {
    let path = match projects_path() {
        Ok(p) => p,
        Err(err) => return (ProjectsStore::default(), Some(err)),
    };
    load_projects_store_from(&path)
}

fn load_projects_store_from(path: &Path) -> (ProjectsStore, Option<String>) {
    if !path.exists() {
        return (ProjectsStore::default(), None);
    }
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            return (
                ProjectsStore::default(),
                quarantine_corrupt_file(path, &err.to_string()),
            )
        }
    };
    match serde_yaml::from_str::<ProjectsStore>(&text) {
        Ok(store) => (store, None),
        Err(err) => (
            ProjectsStore::default(),
            quarantine_corrupt_file(path, &err.to_string()),
        ),
    }
}

/// Move a corrupt file aside so no later save can overwrite it without a
/// surviving copy: rename to `<name>.corrupt`, falling back to a copy when
/// rename is not possible. Returns a user-facing status message.
fn quarantine_corrupt_file(path: &Path, reason: &str) -> Option<String> {
    let corrupt = corrupt_backup_path(path);
    let preserved = match fs::rename(path, &corrupt) {
        Ok(()) => true,
        Err(_) => fs::copy(path, &corrupt).is_ok(),
    };
    if preserved {
        Some(format!(
            "{} is corrupted ({}); the original file was kept as {}. Starting with an empty state.",
            path.display(),
            reason,
            corrupt.display()
        ))
    } else {
        Some(format!(
            "{} is corrupted ({}); it could not be backed up automatically and was left in place.",
            path.display(),
            reason
        ))
    }
}

fn corrupt_backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".corrupt");
    PathBuf::from(name)
}

pub fn save_projects_store(store: &ProjectsStore) -> Result<(), String> {
    let path = projects_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let yaml = serde_yaml::to_string(store).map_err(|e| e.to_string())?;
    write_private_file(&path, yaml.as_bytes())
}

/// Write `data` to `path` atomically (issue #49): the payload goes to a temp
/// file in the same directory (mode `0o600` on Unix), the previous version is
/// copied to `<name>.bak`, then the temp file is renamed over `path`. A crash
/// or power loss mid-write can only lose the temp file — `path` always holds
/// either the previous or the new content, never a truncated mix.
fn write_private_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| format!("no parent directory for {}", path.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("no file name for {}", path.display()))?;
    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("{}.{}.tmp", file_name.to_string_lossy(), seq));

    if let Err(err) = write_private_file_data(&tmp, data) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }

    // Back up the previous version before replacing it. Also covers replacing
    // a corrupt file: the .bak keeps the recoverable bytes.
    if path.exists() {
        let bak = dir.join(format!("{}.bak", file_name.to_string_lossy()));
        if let Err(err) = fs::copy(path, &bak) {
            let _ = fs::remove_file(&tmp);
            return Err(format!(
                "failed to back up {} before writing: {}",
                path.display(),
                err
            ));
        }
    }

    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("failed to replace {}: {}", path.display(), err));
    }

    #[cfg(unix)]
    if let Ok(dir_handle) = fs::File::open(dir) {
        let _ = dir_handle.sync_all();
    }
    Ok(())
}

fn write_private_file_data(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts.open(path).map_err(|e| e.to_string())?;
    file.write_all(data).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    // mode() only applies on create; tighten perms if a stale temp file existed.
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

pub fn program_workspace_snapshot(tabs: &[TabConfig], active_tab: usize) -> WorkspaceConfig {
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

fn migrate_legacy_config_if_needed(new_path: &Path) {
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
    serde_yaml::from_str(yaml_text).unwrap_or_else(|_| load_bundled_config())
}

pub fn merge_config_sources(sources: &[AppConfig]) -> AppConfig {
    let mut merged = AppConfig {
        default_format: "node-default".to_string(),
        default_preset: "node-dev".to_string(),
        max_scrollback_lines: DEFAULT_MAX_SCROLLBACK_LINES,
        viewport_font_size: DEFAULT_VIEWPORT_FONT_SIZE,
        terminals_section_expanded: true,
        files_section_expanded: true,
        shell: ShellPreference::default(),
        formats: HashMap::new(),
        presets: HashMap::new(),
        workspaces: HashMap::new(),
        tui_ssh_profiles: Vec::new(),
    };

    for source in sources {
        // An empty default_format means "no value for this field", not
        // "skip the whole source" — dropping everything else would wipe the
        // user's presets/formats/shell on the next config flush (issue #160).
        if !source.default_format.is_empty() {
            merged.default_format = source.default_format.clone();
        }
        merged.default_preset = source.default_preset.clone();
        merged.max_scrollback_lines = clamp_max_scrollback_lines(source.max_scrollback_lines);
        merged.viewport_font_size = clamp_viewport_font_size(source.viewport_font_size);
        merged.terminals_section_expanded = source.terminals_section_expanded;
        merged.files_section_expanded = source.files_section_expanded;
        if source.shell != ShellPreference::default() {
            merged.shell = source.shell;
        }
        merged.formats.extend(source.formats.clone());
        merged.presets.extend(source.presets.clone());
        merged.workspaces.extend(source.workspaces.clone());
        if !source.tui_ssh_profiles.is_empty() {
            merged.tui_ssh_profiles = source.tui_ssh_profiles.clone();
        }
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
        severity: tab.severity,
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
        severity: Default::default(),
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
                severity: t.severity,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ProgramConfig;

    fn temp_config_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noviewlog-cfg-{name}-{stamp}"))
    }

    fn sample_store_yaml() -> String {
        let store = ProjectsStore {
            projects: vec![ProjectConfig {
                id: "p1".to_string(),
                name: "Alpha".to_string(),
                default_cwd: None,
                path_hint: None,
                programs: vec![ProgramConfig {
                    id: "prog-1".to_string(),
                    name: "run".to_string(),
                    launch: Default::default(),
                    workspace: Default::default(),
                }],
                active_program: 0,
            }],
            active_project: 0,
        };
        serde_yaml::to_string(&store).unwrap()
    }

    fn leftover_tmp_files(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn write_is_atomic_and_keeps_previous_version() {
        let dir = temp_config_dir("atomic");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("projects.yaml");

        write_private_file(&path, b"v1").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"v1");
        assert!(!dir.join("projects.yaml.bak").exists());

        write_private_file(&path, b"second version").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second version");
        assert_eq!(
            fs::read(dir.join("projects.yaml.bak")).unwrap(),
            b"v1",
            ".bak must hold the previous version"
        );
        assert!(
            leftover_tmp_files(&dir).is_empty(),
            "no temp files may survive a successful write"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_projects_yaml_is_preserved_and_reported() {
        let dir = temp_config_dir("quarantine");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("projects.yaml");
        let good = sample_store_yaml();
        write_private_file(&path, good.as_bytes()).unwrap();

        // Truncated YAML: a crash cut the file mid-write. Keep cutting until
        // the prefix no longer parses — some cut points still leave valid
        // YAML, which would not count as corruption.
        let bytes = fs::read(&path).unwrap();
        let mut truncated = bytes[..bytes.len() / 2].to_vec();
        while !truncated.is_empty()
            && serde_yaml::from_str::<ProjectsStore>(&String::from_utf8_lossy(&truncated)).is_ok()
        {
            truncated.pop();
        }
        assert!(
            serde_yaml::from_str::<ProjectsStore>(&String::from_utf8_lossy(&truncated)).is_err(),
            "truncated content must be unparseable"
        );
        fs::write(&path, &truncated).unwrap();

        let (store, msg) = load_projects_store_from(&path);
        assert!(
            store.projects.is_empty(),
            "corrupt store must not parse into data"
        );
        let msg = msg.expect("corruption must surface a status message");
        assert!(msg.contains("corrupted"), "message: {msg}");

        let corrupt = dir.join("projects.yaml.corrupt");
        assert!(corrupt.exists(), "corrupt file must be preserved");
        assert_eq!(
            fs::read(&corrupt).unwrap(),
            truncated,
            "recovery must keep the original bytes"
        );
        assert!(!path.exists(), "corrupt file must be moved aside");

        // A later save starts a fresh store without touching the .corrupt copy.
        write_private_file(&path, sample_store_yaml().as_bytes()).unwrap();
        let (store, msg) = load_projects_store_from(&path);
        assert!(msg.is_none());
        assert_eq!(store.projects.len(), 1);
        assert_eq!(fs::read(&corrupt).unwrap(), truncated);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crash_mid_write_leaves_previous_store_loadable() {
        let dir = temp_config_dir("crash");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("projects.yaml");
        write_private_file(&path, sample_store_yaml().as_bytes()).unwrap();

        // Simulate a crash mid-write: a temp file with partial new content is
        // left behind, the real file is untouched.
        let tmp = dir.join("projects.yaml.99.tmp");
        fs::write(&tmp, b"projects: [").unwrap();

        let (store, msg) = load_projects_store_from(&path);
        assert!(msg.is_none(), "stray temp files must be ignored: {msg:?}");
        assert_eq!(store.projects.len(), 1);
        assert!(tmp.exists());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn never_overwrites_without_backup() {
        let dir = temp_config_dir("no-backup-no-write");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("projects.yaml");
        write_private_file(&path, b"v1").unwrap();

        // Make the backup step impossible: .bak exists as a directory.
        fs::create_dir(dir.join("projects.yaml.bak")).unwrap();

        let err = write_private_file(&path, b"v2").unwrap_err();
        assert!(err.contains("back up"), "error: {err}");
        assert_eq!(
            fs::read(&path).unwrap(),
            b"v1",
            "original file must survive a failed write"
        );
        assert!(
            leftover_tmp_files(&dir).is_empty(),
            "failed write must clean up its temp file"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn valid_projects_yaml_loads_without_warnings() {
        let dir = temp_config_dir("valid");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("projects.yaml");
        write_private_file(&path, sample_store_yaml().as_bytes()).unwrap();

        let (store, msg) = load_projects_store_from(&path);
        assert!(msg.is_none());
        assert_eq!(store.projects.len(), 1);
        assert_eq!(store.projects[0].name, "Alpha");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_user_config_yaml_is_preserved_and_reported() {
        let dir = temp_config_dir("user-cfg");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.yaml");
        let bad = b"default_format: [unclosed\n".to_vec();
        fs::write(&path, &bad).unwrap();

        let (config, msg) = load_user_config_from(&path);
        assert!(config.is_none(), "corrupt user config must not parse");
        let msg = msg.expect("corruption must surface a status message");
        assert!(msg.contains("corrupted"), "message: {msg}");
        assert_eq!(
            fs::read(dir.join("config.yaml.corrupt")).unwrap(),
            bad,
            "corrupt config must be preserved"
        );

        // Valid user config still merges over the bundled defaults.
        fs::write(&path, "max_scrollback_lines: 5000\n").unwrap();
        let (config, msg) = load_user_config_from(&path);
        assert!(msg.is_none());
        let config = config.expect("valid user config must load");
        assert_eq!(config.max_scrollback_lines, 5000);
        assert!(
            !config.presets.is_empty(),
            "bundled presets must survive the merge"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_default_format_does_not_drop_the_source() {
        // A user config with an empty default_format but real settings must
        // still contribute those settings (issue #160).
        let user = parse_yaml_config(
            "default_format: \"\"\nmax_scrollback_lines: 4242\npresets:\n  mine:\n    filters: []\n",
        );
        let merged = merge_config_sources(&[load_bundled_config(), user]);
        assert_eq!(
            merged.default_format, "node-default",
            "empty user default_format falls back to the earlier source"
        );
        assert_eq!(merged.max_scrollback_lines, 4242);
        assert!(
            merged.presets.contains_key("mine"),
            "user presets must survive an empty default_format"
        );
    }
}
