//! Resolve spawn argv before handing off to portable-pty.
//!
//! On Windows, CreateProcessW (used by portable-pty) does **not** apply PATHEXT when
//! `lpApplicationName` is a bare name like `node`. If PATH lookup also misses, the OS
//! error is ERROR_FILE_NOT_FOUND (localized OS message, e.g. "file not found"). Node/npm installers
//! often leave `node.exe` and `npm.cmd` on PATH — we must resolve `.exe` ourselves and
//! wrap `.cmd`/`.bat` via `cmd.exe /d /c`.
//!
//! Also: Microsoft Store "App execution aliases" put 0-byte `node.exe` stubs under
//! `%LOCALAPPDATA%\Microsoft\WindowsApps`. Spawning those under ConPTY exits with
//! `STATUS_DLL_INIT_FAILED` (0xC0000142 / -1073741502). Prefer real installs.
//!
//! UNC working directories (`\\server\share`, `\\wsl$\…`) break CreateProcess/ConPTY and
//! `cmd.exe` ("CMD does not support UNC paths as current directories"). For `wsl.exe` we
//! convert `\\wsl$\Distro\path` into `-d Distro --cd /path` and always use a local
//! Windows cwd for the `wsl` process itself.
//!
//! WSL mode must never CreateProcess Windows `pnpm`/`npm`/`node` against a UNC mount of
//! the distro. A bare `wsl -- pnpm` without a login shell often misses Linux nvm/fnm PATH
//! and falls through to Windows pnpm via WSL interop — which then sees
//! `\\wsl.localhost\…` and writes to Windows directories. We pass `--shell-type login`
//! so Linux tools win, keep Linux cwd only via `--cd`, and pin `wsl.exe` under System32.

use crate::core::types::LaunchConfig;
use std::path::{Path, PathBuf};

/// Resolved spawn plan ready for portable-pty `CommandBuilder`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSpawn {
    pub command: String,
    pub args: Vec<String>,
    /// Working directory for CreateProcess / posix_spawn — never a UNC path on Windows.
    pub cwd: String,
}

/// Expand a saved [`LaunchConfig`] into `(command, args, cwd)` ready for [`prepare_spawn`].
///
/// WSL mode builds `wsl.exe … -- <command> <args>` and forces a **local Windows** cwd
/// (never the Linux path, never UNC). On non-Windows hosts WSL mode returns an error.
pub fn resolve_process_launch(
    launch: &LaunchConfig,
) -> Result<(String, Vec<String>, Option<String>), String> {
    if launch.wsl {
        if !cfg!(windows) {
            return Err("WSL launch mode is Windows-only".to_string());
        }
        let cmd = launch
            .command
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "WSL mode requires a Command (Linux executable, e.g. npm) — not `wsl` itself"
                    .to_string()
            })?;
        let (exe, args) = build_wsl_argv(
            cmd,
            &launch.args,
            launch.cwd.as_deref(),
            launch.wsl_distro.as_deref(),
        )?;
        // CreateProcess cwd must be local; Linux workdir is already in `--cd`.
        return Ok((exe, args, Some(safe_windows_cwd())));
    }

    // Process mode: never silently run Windows tools against a WSL UNC mount.
    if let Some(cwd) = launch.cwd.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if parse_wsl_unc(cwd).is_some() || is_unc_path(cwd) {
            return Err(format!(
                "Working directory looks like a WSL/UNC path ({cwd}). \
                 Use launch mode **WSL** with a Linux path (e.g. /home/…), \
                 not Process mode with \\\\wsl.localhost\\\\… — that runs Windows \
                 node/pnpm against the UNC mount."
            ));
        }
    }

    let command = launch
        .command
        .clone()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "no command configured".to_string())?;
    Ok((command, launch.args.clone(), launch.cwd.clone()))
}

/// Resolve an interactive shell for the Console tab (no configured Start command needed).
///
/// Uses the program's cwd / WSL settings when present. On Unix: `$SHELL` (fallback
/// `/bin/bash`). On Windows: PowerShell, or a login `bash` inside WSL when `wsl: true`.
pub fn resolve_interactive_shell(
    launch: &LaunchConfig,
) -> Result<(String, Vec<String>, Option<String>), String> {
    if launch.wsl {
        if !cfg!(windows) {
            return Err("WSL launch mode is Windows-only".to_string());
        }
        let (exe, args) = build_wsl_argv(
            "bash",
            &["-l".to_string()],
            launch.cwd.as_deref(),
            launch.wsl_distro.as_deref(),
        )?;
        return Ok((exe, args, Some(safe_windows_cwd())));
    }

    if let Some(cwd) = launch.cwd.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if parse_wsl_unc(cwd).is_some() || is_unc_path(cwd) {
            return Err(format!(
                "Working directory looks like a WSL/UNC path ({cwd}). \
                 Use launch mode **WSL** with a Linux path (e.g. /home/…)."
            ));
        }
    }

    Ok((
        default_interactive_shell(),
        Vec::new(),
        launch.cwd.clone(),
    ))
}

fn default_interactive_shell() -> String {
    #[cfg(windows)]
    {
        "powershell.exe".to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

/// Preferred `wsl.exe` path for CreateProcess (System32). Falls back to bare `wsl`
/// when SystemRoot is unset (unit tests / odd environments).
pub fn windows_wsl_exe() -> String {
    if let Ok(root) = std::env::var("SystemRoot").or_else(|_| std::env::var("SYSTEMROOT")) {
        let p = PathBuf::from(root).join("System32").join("wsl.exe");
        return p.to_string_lossy().into_owned();
    }
    // Stable default on real Windows; prepare_spawn still resolves via PATH if missing.
    r"C:\Windows\System32\wsl.exe".to_string()
}

/// Normalize a WSL-mode working directory into a Linux `--cd` path.
/// Accepts `/home/…` or `\\wsl$\Distro\…` / `\\wsl.localhost\Distro\…` (and single-slash variants).
/// Returns `(linux_path, distro_hint_from_unc)`.
pub fn normalize_wsl_linux_cwd(cwd: &str) -> Result<(String, Option<String>), String> {
    let t = cwd.trim();
    if t.is_empty() {
        return Err("WSL working directory is empty".to_string());
    }
    if looks_like_linux_abs(t) {
        // Mistaken `/wsl.localhost/Distro/home/…` (URI-style) → treat as UNC-ish.
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("/wsl.localhost/") || lower.starts_with("/wsl$/") {
            let as_unc = format!(r"\\{}", t.trim_start_matches('/').replace('/', "\\"));
            if let Some((distro, linux)) = parse_wsl_unc(&as_unc) {
                return Ok((linux, Some(distro)));
            }
        }
        return Ok((t.to_string(), None));
    }
    if let Some((distro, linux)) = parse_wsl_unc(t) {
        return Ok((linux, Some(distro)));
    }
    if is_unc_path(t) {
        return Err(format!(
            "UNC working directory is not a Linux path for wsl --cd. \
             Use /home/… or \\\\wsl$\\\\Distro\\\\…. Got: {t}"
        ));
    }
    if looks_like_windows_drive_path(t) {
        return Err(format!(
            "WSL working directory must be a Linux path (e.g. /home/user/proj), \
             not a Windows drive path. Got: {t}"
        ));
    }
    Err(format!(
        "WSL working directory must be a Linux path (e.g. /home/user/proj). Got: {t}"
    ))
}

/// Build `wsl` argv for dedicated WSL launch mode (pure; tested on all platforms).
///
/// Result:
/// `("<wsl.exe>", [--shell-type, login, -d Distro?, --cd /linux/path?, --, command, ...args])`.
/// Callers must use a local Windows cwd (never UNC / never the Linux path).
pub fn build_wsl_argv(
    linux_command: &str,
    linux_args: &[String],
    linux_cwd: Option<&str>,
    distro: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let (exe, mut rest) = normalize_command_args(linux_command, linux_args.to_vec());
    if exe.is_empty() {
        return Err("WSL command is empty".to_string());
    }
    if is_wsl_executable(&exe) {
        return Err(
            "WSL mode Command should be the Linux executable (e.g. npm), not wsl itself"
                .to_string(),
        );
    }

    let mut distro = distro.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let mut cd: Option<String> = None;
    if let Some(raw) = linux_cwd.map(str::trim).filter(|s| !s.is_empty()) {
        let (linux, from_unc) = normalize_wsl_linux_cwd(raw)?;
        if distro.is_none() {
            distro = from_unc;
        }
        cd = Some(linux);
    }

    let mut args = Vec::new();
    // Login shell so Linux nvm/fnm/pnpm PATH wins over Windows interop appendWindowsPath.
    args.push("--shell-type".to_string());
    args.push("login".to_string());
    if let Some(d) = distro {
        args.push("-d".to_string());
        args.push(d);
    }
    if let Some(cd) = cd {
        args.push("--cd".to_string());
        args.push(cd);
    }
    args.push("--".to_string());
    args.push(exe);
    args.append(&mut rest);
    Ok((windows_wsl_exe(), args))
}

/// Split a mistaken "cmd + args in Command" field, then platform-resolve the executable
/// and (on Windows) sanitize cwd for ConPTY / WSL / cmd.exe.
pub fn prepare_spawn(
    command: &str,
    args: Vec<String>,
    cwd: Option<&str>,
) -> Result<PreparedSpawn, String> {
    let (exe, args) = normalize_command_args(command, args);
    if exe.is_empty() {
        return Err("command is empty".to_string());
    }

    let cwd = cwd.unwrap_or(".").to_string();

    #[cfg(windows)]
    {
        resolve_windows(&exe, args, &cwd)
    }
    #[cfg(not(windows))]
    {
        Ok(PreparedSpawn {
            command: exe,
            args,
            cwd,
        })
    }
}

/// Normalize a Windows working directory for ConPTY CreateProcess.
/// Strips the `\\?\` extended prefix (breaks some ConPTY/cwd combos), rewrites
/// `\\?\UNC\server\…` back to `\\server\…`, and unifies separators — does **not**
/// canonicalize (that would re-introduce `\\?\` and can fail on mapped drives).
pub fn normalize_windows_cwd(cwd: &str) -> String {
    let trimmed = cwd.trim();
    let without_ext = if let Some(rest) = trimmed
        .strip_prefix(r"\\?\")
        .or_else(|| trimmed.strip_prefix("//?/"))
    {
        let rest_norm = rest.replace('/', "\\");
        if rest_norm.len() >= 4 && rest_norm[..4].eq_ignore_ascii_case(r"UNC\") {
            format!(r"\\{}", &rest_norm[4..])
        } else {
            rest_norm
        }
    } else {
        trimmed.replace('/', "\\")
    };
    without_ext
}

/// True for UNC paths: `\\server\share`, `//server/share`, `\\?\UNC\…`, `\\wsl$\…`.
pub fn is_unc_path(path: &str) -> bool {
    let n = normalize_windows_cwd(path);
    if n.len() >= 2 && n.as_bytes()[0] == b'\\' && n.as_bytes()[1] == b'\\' {
        return true;
    }
    // Defensive: if normalize somehow left `UNC\server\…` without leading `\\`.
    let lower = n.to_ascii_lowercase();
    lower.starts_with(r"unc\") || lower.starts_with("unc/")
}

/// Parse `\\wsl$\Distro\rest` / `\\wsl.localhost\Distro\rest` into `(distro, /linux/path)`.
/// Also accepts a single leading `\wsl.localhost\…` (sometimes shown in UI / copied paths).
pub fn parse_wsl_unc(path: &str) -> Option<(String, String)> {
    let mut n = normalize_windows_cwd(path);
    // `\wsl.localhost\Distro\…` (one slash) → `\\wsl.localhost\Distro\…`
    if n.len() >= 2 && n.as_bytes()[0] == b'\\' && n.as_bytes()[1] != b'\\' {
        let lower = n.to_ascii_lowercase();
        if lower.starts_with(r"\wsl$") || lower.starts_with(r"\wsl.localhost") {
            n = format!(r"\{n}");
        }
    }
    let rest = n.strip_prefix(r"\\")?;
    let (host, after_host) = rest.split_once('\\')?;
    let host_l = host.to_ascii_lowercase();
    if host_l != "wsl$" && host_l != "wsl.localhost" {
        return None;
    }
    let (distro, linux_rel) = match after_host.split_once('\\') {
        Some((d, r)) => (d, r),
        None => (after_host, ""),
    };
    if distro.is_empty() {
        return None;
    }
    let linux = if linux_rel.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", linux_rel.replace('\\', "/"))
    };
    Some((distro.to_string(), linux))
}

/// True when the executable basename is `wsl` / `wsl.exe`.
pub fn is_wsl_executable(program: &str) -> bool {
    // Split on both separators so `C:\…\wsl.exe` works when tests/host are Unix.
    let name = program
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program);
    name.eq_ignore_ascii_case("wsl") || name.eq_ignore_ascii_case("wsl.exe")
}

/// Local drive/folder safe for CreateProcess when UNC or WSL is involved.
pub fn safe_windows_cwd() -> String {
    if let Ok(root) = std::env::var("SystemRoot").or_else(|_| std::env::var("SYSTEMROOT")) {
        let p = PathBuf::from(root).join("System32");
        if p.is_dir() {
            return p.to_string_lossy().into_owned();
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(profile);
        if p.is_dir() {
            return p.to_string_lossy().into_owned();
        }
    }
    std::env::temp_dir().to_string_lossy().into_owned()
}

fn looks_like_linux_abs(path: &str) -> bool {
    let t = path.trim();
    t.starts_with('/') && !t.starts_with("//")
}

fn wsl_args_have_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn wsl_args_have_distro(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "-d" || a == "-D" || a == "--distribution")
}

/// Prepend `-d` / `--cd` when missing so we do not clobber explicit user args.
pub fn inject_wsl_cd_args(
    args: Vec<String>,
    distro: Option<&str>,
    linux_cd: Option<&str>,
) -> Vec<String> {
    let mut prefix = Vec::new();
    if let Some(d) = distro {
        if !d.is_empty() && !wsl_args_have_distro(&args) {
            prefix.push("-d".to_string());
            prefix.push(d.to_string());
        }
    }
    if let Some(cd) = linux_cd {
        if !cd.is_empty() && !wsl_args_have_flag(&args, "--cd") {
            prefix.push("--cd".to_string());
            prefix.push(cd.to_string());
        }
    }
    prefix.extend(args);
    prefix
}

/// If `command` looks like `node app.js` (no path separators), treat the first token as
/// the executable and prepend the rest to `args`. Quoted paths and real path strings
/// are left alone.
pub fn normalize_command_args(command: &str, mut args: Vec<String>) -> (String, Vec<String>) {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return (String::new(), args);
    }

    let unquoted = strip_outer_quotes(trimmed);
    if looks_like_path(unquoted) {
        return (unquoted.to_string(), args);
    }

    let tokens = split_whitespace_tokens(unquoted);
    if tokens.len() <= 1 {
        return (unquoted.to_string(), args);
    }

    let mut iter = tokens.into_iter();
    let exe = iter.next().unwrap_or_default();
    let mut rest: Vec<String> = iter.collect();
    rest.append(&mut args);
    (exe, rest)
}

fn strip_outer_quotes(s: &str) -> &str {
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'"' && bytes[s.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[s.len() - 1] == b'\'')
        {
            return &s[1..s.len() - 1];
        }
    }
    s
}

fn looks_like_path(s: &str) -> bool {
    if s.contains('/') || s.contains('\\') {
        return true;
    }
    // Windows drive-relative: `C:foo` or `C:`
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn split_whitespace_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes: Option<char> = None;
    for ch in s.chars() {
        match in_quotes {
            Some(q) if ch == q => {
                in_quotes = None;
            }
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => {
                in_quotes = Some(ch);
            }
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(windows)]
fn resolve_windows(
    program: &str,
    args: Vec<String>,
    cwd: &str,
) -> Result<PreparedSpawn, String> {
    let cwd_norm = normalize_windows_cwd(cwd);
    // For PATH probing, prefer a usable local dir; UNC cwd is not a valid probe base.
    let probe_cwd = if is_unc_path(&cwd_norm) {
        None
    } else {
        Some(cwd_norm.as_str())
    };

    let resolved = find_windows_executable(program, probe_cwd).ok_or_else(|| {
        format!(
            "executable '{program}' not found on PATH (with PATHEXT). \
             Set Command to the full path of the .exe (e.g. C:\\\\Program Files\\\\nodejs\\\\node.exe), \
             or ensure the tool is on the system/user PATH. \
             If you only have a Microsoft Store 'App execution alias' under WindowsApps, \
             install Node from https://nodejs.org instead. \
             Working directory: {cwd_norm}"
        )
    })?;

    if is_windows_apps_stub(&resolved) {
        return Err(format!(
            "refusing to spawn Microsoft Store App execution alias '{resolved}' \
             (exits with STATUS_DLL_INIT_FAILED / 0xC0000142 under ConPTY). \
             Install a real Node.js (nodejs.org / fnm / nvm-windows) or set Command to \
             the full path of node.exe.",
            resolved = resolved.display()
        ));
    }

    let is_wsl = is_wsl_executable(program) || is_wsl_executable(&resolved.to_string_lossy());
    let (args, final_cwd) = finalize_windows_cwd_and_args(is_wsl, args, &cwd_norm)?;

    let (command, args) = if is_batch_file(&resolved) {
        wrap_batch_via_cmd(resolved, args)
    } else {
        (resolved.to_string_lossy().into_owned(), args)
    };

    Ok(PreparedSpawn {
        command,
        args,
        cwd: final_cwd,
    })
}

/// Sanitize Windows cwd for CreateProcess/cmd/WSL; may inject `wsl --cd` / `-d`.
pub fn finalize_windows_cwd_and_args(
    is_wsl: bool,
    args: Vec<String>,
    cwd: &str,
) -> Result<(Vec<String>, String), String> {
    // Linux absolute paths are only for `wsl --cd` — do not Windows-normalize
    // (`/` → `\`) or CreateProcess will get a bogus path.
    if is_wsl && looks_like_linux_abs(cwd) {
        let linux = cwd.trim().to_string();
        let args = inject_wsl_cd_args(args, None, Some(&linux));
        return Ok((args, safe_windows_cwd()));
    }

    let cwd = normalize_windows_cwd(cwd);
    let unc = is_unc_path(&cwd);

    if is_wsl {
        if let Some((distro, linux)) = parse_wsl_unc(&cwd) {
            let args = inject_wsl_cd_args(args, Some(&distro), Some(&linux));
            return Ok((args, safe_windows_cwd()));
        }
        if unc {
            return Err(format!(
                "UNC working directory is not usable as a Windows process cwd \
                 (CreateProcess/cmd.exe reject UNC). For WSL, either:\n\
                 • Browse to \\\\wsl$\\Distro\\… (auto-converted to wsl -d Distro --cd /linux/path), or\n\
                 • Leave Working directory empty and put `--cd /linux/path` in Args, or\n\
                 • Use a local drive path (e.g. C:\\Users\\…). \
                 Got: {cwd}"
            ));
        }
        // Local Windows path (or `.`): never inherit a UNC parent — use a known-local dir.
        // Project dir for Linux is expected via Args `--cd` when cwd is empty/local-only.
        if cwd == "." || cwd.is_empty() {
            return Ok((args, safe_windows_cwd()));
        }
        // Keep a real local drive cwd (harmless for wsl.exe); still avoid relative `.`.
        if looks_like_windows_drive_path(&cwd) {
            return Ok((args, cwd));
        }
        return Ok((args, safe_windows_cwd()));
    }

    if unc {
        return Err(format!(
            "UNC working directory is not supported for CreateProcess/cmd.exe \
             (\"CMD does not support UNC paths as current directories\"). \
             Use a local drive path (C:\\…, Z:\\…), or for Linux projects set Command to \
             wsl.exe and either Browse \\\\wsl$\\Distro\\… or leave cwd empty and use \
             `--cd /linux/path` in Args. Got: {cwd}"
        ));
    }

    // Relative `.` still resolves against the parent process cwd — which may be UNC
    // if NoViewLog itself was started from \\\\wsl$\\…. Prefer a known-local folder.
    if cwd.is_empty() || cwd == "." {
        return Ok((args, safe_windows_cwd()));
    }

    Ok((args, cwd))
}

fn looks_like_windows_drive_path(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Human-readable spawn line for status / errors (`exe arg1 arg2` + optional cwd).
pub fn format_spawn_cmdline(command: &str, args: &[String], cwd: Option<&str>) -> String {
    let mut line = command.to_string();
    for a in args {
        line.push(' ');
        if a.is_empty() || a.contains(char::is_whitespace) {
            line.push('"');
            line.push_str(a);
            line.push('"');
        } else {
            line.push_str(a);
        }
    }
    if let Some(c) = cwd.map(str::trim).filter(|s| !s.is_empty()) {
        line.push_str("  [cwd: ");
        line.push_str(c);
        line.push(']');
    }
    line
}

#[cfg(windows)]
fn is_batch_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            e == "cmd" || e == "bat"
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn wrap_batch_via_cmd(script: PathBuf, args: Vec<String>) -> (String, Vec<String>) {
    let comspec = std::env::var_os("COMSPEC")
        .or_else(|| std::env::var_os("ComSpec"))
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\cmd.exe"));

    // `call` is required so cmd returns the script's exit code; /D skips AutoRun hooks
    // that sometimes break headless ConPTY sessions.
    let mut line = format!("call {}", quote_win_arg(&script.to_string_lossy()));
    for arg in &args {
        line.push(' ');
        line.push_str(&quote_win_arg(arg));
    }

    (
        comspec.to_string_lossy().into_owned(),
        vec!["/D".to_string(), "/C".to_string(), line],
    )
}

/// Quote one argument for cmd.exe parsing (double internal quotes).
#[cfg(windows)]
fn quote_win_arg(arg: &str) -> String {
    let mut out = String::from("\"");
    for ch in arg.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

#[cfg(windows)]
fn find_windows_executable(program: &str, cwd: Option<&str>) -> Option<PathBuf> {
    let program_path = Path::new(program);
    let extensions = pathext_list();

    if program_path.components().count() > 1 || looks_like_path(program) {
        return resolve_existing_candidate(program_path, cwd, &extensions)
            .filter(|p| !is_unusable_stub(p));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(dir) = cwd.map(Path::new).filter(|p| p.is_dir()) {
        if let Some(found) = probe_dir(dir, program, &extensions) {
            push_unique(&mut candidates, found);
        }
    }

    for dir in std::env::split_paths(&windows_path_env()) {
        if let Some(found) = probe_dir(&dir, program, &extensions) {
            push_unique(&mut candidates, found);
        }
    }

    for dir in well_known_bin_dirs() {
        if let Some(found) = probe_dir(&dir, program, &extensions) {
            push_unique(&mut candidates, found);
        }
    }

    pick_best_candidate(candidates)
}

#[cfg(windows)]
fn push_unique(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if !candidates.iter().any(|c| c == &path) {
        candidates.push(path);
    }
}

/// Prefer real installs over Microsoft Store App Execution Alias stubs in WindowsApps.
pub fn pick_best_candidate(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, i32)> = None;
    for candidate in candidates {
        if is_unusable_stub(&candidate) {
            continue;
        }
        let score = candidate_score(&candidate);
        match &best {
            Some((_, best_score)) if score <= *best_score => {}
            _ => best = Some((candidate, score)),
        }
    }
    best.map(|(p, _)| p)
}

/// Score a resolved executable path. Higher is better.
pub fn candidate_score(path: &Path) -> i32 {
    let s = path.to_string_lossy().to_ascii_lowercase();
    let mut score = 0;
    if is_windows_apps_path(&s) {
        score -= 1000;
    }
    if s.contains(r"\nodejs\")
        || s.contains(r"\fnm\")
        || s.contains(r"\volta\")
        || s.contains(r"\nvm\")
        || s.contains(r"\nvs\")
    {
        score += 50;
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("exe") => score += 10,
        Some(ext) if ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat") => {
            score += 5;
        }
        _ => {}
    }
    score
}

fn is_windows_apps_path(path_lower: &str) -> bool {
    path_lower.contains(r"\windowsapps\") || path_lower.contains("/windowsapps/")
}

fn is_windows_apps_stub(path: &Path) -> bool {
    is_windows_apps_path(&path.to_string_lossy().to_ascii_lowercase())
}

fn is_unusable_stub(path: &Path) -> bool {
    if is_windows_apps_stub(path) {
        return true;
    }
    // App-execution alias stubs are often 0 bytes even outside WindowsApps naming.
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.is_file() && meta.len() == 0 {
            return true;
        }
    }
    false
}

/// Merge process PATH with Machine + User PATH from the registry (same idea as
/// portable-pty's CommandBuilder base env).
#[cfg(windows)]
fn windows_path_env() -> std::ffi::OsString {
    use std::ffi::OsString;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    fn reg_path(root: winreg::HKEY, subkey: &str) -> Option<OsString> {
        let key = RegKey::predef(root).open_subkey(subkey).ok()?;
        // Prefer expanded values when Windows stored REG_EXPAND_SZ.
        key.get_value::<String, _>("Path")
            .or_else(|_| key.get_value::<String, _>("PATH"))
            .ok()
            .map(OsString::from)
    }

    let mut parts: Vec<OsString> = Vec::new();
    if let Some(p) = std::env::var_os("PATH") {
        if !p.is_empty() {
            parts.push(p);
        }
    }
    if let Some(p) = reg_path(
        HKEY_LOCAL_MACHINE,
        r"System\CurrentControlSet\Control\Session Manager\Environment",
    ) {
        parts.push(p);
    }
    if let Some(p) = reg_path(HKEY_CURRENT_USER, "Environment") {
        parts.push(p);
    }

    let mut merged = OsString::new();
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            merged.push(";");
        }
        merged.push(part);
    }
    merged
}

#[cfg(windows)]
fn well_known_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        dirs.push(PathBuf::from(pf).join("nodejs"));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
        dirs.push(PathBuf::from(pf86).join("nodejs"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(&local).join(r"Programs\nodejs"));
        // nvm-windows symlink location
        dirs.push(PathBuf::from(&local).join("nvm"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join(r"npm"));
    }
    dirs
}

#[cfg(windows)]
fn resolve_existing_candidate(
    program_path: &Path,
    cwd: Option<&str>,
    extensions: &[String],
) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if program_path.is_absolute() {
        vec![program_path.to_path_buf()]
    } else if let Some(dir) = cwd.map(Path::new) {
        vec![dir.join(program_path)]
    } else {
        vec![program_path.to_path_buf()]
    };

    for base in candidates {
        if base.is_file() {
            return Some(base);
        }
        if base.extension().is_none() {
            for ext in extensions {
                let with_ext = base.with_extension(ext.trim_start_matches('.'));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn probe_dir(dir: &Path, program: &str, extensions: &[String]) -> Option<PathBuf> {
    let exact = dir.join(program);
    if exact.is_file() {
        // Prefer PE images over batch shims when both exist as the bare name.
        if !is_batch_file(&exact) {
            return Some(exact);
        }
        // Fall through to try .exe first via extensions, then accept batch.
    }

    let mut batch: Option<PathBuf> = if exact.is_file() && is_batch_file(&exact) {
        Some(exact)
    } else {
        None
    };

    for ext in extensions {
        let ext_body = ext.trim_start_matches('.');
        let candidate = dir.join(program).with_extension(ext_body);
        if !candidate.is_file() {
            continue;
        }
        if is_batch_file(&candidate) {
            if batch.is_none() {
                batch = Some(candidate);
            }
            continue;
        }
        return Some(candidate);
    }
    batch
}

#[cfg(windows)]
fn pathext_list() -> Vec<String> {
    let raw = std::env::var_os("PATHEXT")
        .unwrap_or_else(|| std::ffi::OsString::from(".COM;.EXE;.BAT;.CMD"));
    std::env::split_paths(&raw)
        .filter_map(|p| p.into_os_string().into_string().ok())
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn normalize_splits_node_app_js() {
        let (exe, args) = normalize_command_args("node app.js", vec![]);
        assert_eq!(exe, "node");
        assert_eq!(args, vec!["app.js"]);
    }

    #[test]
    fn normalize_prepends_split_tokens_to_existing_args() {
        let (exe, args) = normalize_command_args("node app.js", vec!["--flag".into()]);
        assert_eq!(exe, "node");
        assert_eq!(args, vec!["app.js", "--flag"]);
    }

    #[test]
    fn normalize_keeps_path_with_spaces_when_quoted() {
        let (exe, args) =
            normalize_command_args(r#""C:\Program Files\nodejs\node.exe""#, vec!["a.js".into()]);
        assert_eq!(exe, r"C:\Program Files\nodejs\node.exe");
        assert_eq!(args, vec!["a.js"]);
    }

    #[test]
    fn normalize_keeps_unix_path() {
        let (exe, args) = normalize_command_args("/usr/bin/node", vec!["a.js".into()]);
        assert_eq!(exe, "/usr/bin/node");
        assert_eq!(args, vec!["a.js"]);
    }

    #[test]
    fn normalize_bare_node_unchanged() {
        let (exe, args) = normalize_command_args("node", vec!["app.js".into()]);
        assert_eq!(exe, "node");
        assert_eq!(args, vec!["app.js"]);
    }

    #[test]
    fn normalize_npm_run_develop_in_command() {
        let (exe, args) = normalize_command_args("npm run develop", vec![]);
        assert_eq!(exe, "npm");
        assert_eq!(args, vec!["run", "develop"]);
    }

    #[test]
    fn normalize_windows_cwd_strips_extended_prefix() {
        assert_eq!(
            normalize_windows_cwd(r"\\?\Z:\sample"),
            r"Z:\sample"
        );
        assert_eq!(normalize_windows_cwd(r"Z:/sample/app"), r"Z:\sample\app");
        assert_eq!(normalize_windows_cwd(r"C:\Prog"), r"C:\Prog");
    }

    #[test]
    fn normalize_windows_cwd_rewrites_extended_unc() {
        assert_eq!(
            normalize_windows_cwd(r"\\?\UNC\wsl$\Ubuntu\home\x"),
            r"\\wsl$\Ubuntu\home\x"
        );
    }

    #[test]
    fn is_unc_detects_wsl_and_share() {
        assert!(is_unc_path(r"\\wsl$\Ubuntu\home\x"));
        assert!(is_unc_path(r"//wsl$/Ubuntu/home/x"));
        assert!(is_unc_path(r"\\?\UNC\server\share\dir"));
        assert!(is_unc_path(r"\\nas\projects"));
        assert!(!is_unc_path(r"C:\Users\x"));
        assert!(!is_unc_path(r"Z:\sample"));
        assert!(!is_unc_path(r"\\?\C:\Windows"));
    }

    #[test]
    fn parse_wsl_unc_distro_and_linux_path() {
        assert_eq!(
            parse_wsl_unc(r"\\wsl$\Ubuntu\home\user\proj"),
            Some(("Ubuntu".into(), "/home/user/proj".into()))
        );
        assert_eq!(
            parse_wsl_unc(r"\\wsl.localhost\Debian\var\log"),
            Some(("Debian".into(), "/var/log".into()))
        );
        assert_eq!(
            parse_wsl_unc(r"\\?\UNC\wsl$\Ubuntu\home\x"),
            Some(("Ubuntu".into(), "/home/x".into()))
        );
        assert_eq!(
            parse_wsl_unc(r"\\wsl$\Ubuntu"),
            Some(("Ubuntu".into(), "/".into()))
        );
        assert_eq!(parse_wsl_unc(r"\\nas\share\dir"), None);
        assert_eq!(parse_wsl_unc(r"C:\Users"), None);
    }

    #[test]
    fn is_wsl_executable_basename() {
        assert!(is_wsl_executable("wsl"));
        assert!(is_wsl_executable("WSL.EXE"));
        assert!(is_wsl_executable(r"C:\Windows\System32\wsl.exe"));
        assert!(!is_wsl_executable("wsl-helper"));
        assert!(!is_wsl_executable("node"));
    }

    #[test]
    fn inject_wsl_cd_preserves_existing_cd_adds_distro() {
        let out = inject_wsl_cd_args(
            vec!["--cd".into(), "/tmp".into(), "bash".into()],
            Some("Ubuntu"),
            Some("/home/x"),
        );
        assert_eq!(out, vec!["-d", "Ubuntu", "--cd", "/tmp", "bash"]);
    }

    #[test]
    fn inject_wsl_cd_adds_distro_and_cd() {
        let out = inject_wsl_cd_args(
            vec!["bash".into(), "-lc".into(), "npm run develop".into()],
            Some("Ubuntu"),
            Some("/home/x/proj"),
        );
        assert_eq!(
            out,
            vec![
                "-d",
                "Ubuntu",
                "--cd",
                "/home/x/proj",
                "bash",
                "-lc",
                "npm run develop"
            ]
        );
    }

    #[test]
    fn finalize_wsl_converts_wsl_unc() {
        let (args, cwd) = finalize_windows_cwd_and_args(
            true,
            vec!["bash".into(), "-lc".into(), "echo hi".into()],
            r"\\wsl$\Ubuntu\home\user\app",
        )
        .unwrap();
        assert!(!is_unc_path(&cwd), "cwd must be local: {cwd}");
        assert_eq!(
            args,
            vec![
                "-d",
                "Ubuntu",
                "--cd",
                "/home/user/app",
                "bash",
                "-lc",
                "echo hi"
            ]
        );
    }

    #[test]
    fn finalize_wsl_linux_abs_cwd_injects_cd() {
        let (args, cwd) =
            finalize_windows_cwd_and_args(true, vec!["uname".into(), "-a".into()], "/home/user")
                .unwrap();
        assert!(!is_unc_path(&cwd));
        assert_eq!(args, vec!["--cd", "/home/user", "uname", "-a"]);
    }

    #[test]
    fn finalize_wsl_keeps_explicit_cd() {
        let (args, cwd) = finalize_windows_cwd_and_args(
            true,
            vec!["--cd".into(), "/opt/app".into(), "bash".into()],
            r"\\wsl$\Ubuntu\home\other",
        )
        .unwrap();
        assert!(!is_unc_path(&cwd));
        // distro injected; user's --cd preserved
        assert_eq!(args, vec!["-d", "Ubuntu", "--cd", "/opt/app", "bash"]);
    }

    #[test]
    fn finalize_non_wsl_rejects_unc() {
        let err = finalize_windows_cwd_and_args(false, vec![], r"\\nas\share\proj").unwrap_err();
        assert!(err.contains("UNC"), "{err}");
    }

    #[test]
    fn finalize_wsl_rejects_non_wsl_unc() {
        let err = finalize_windows_cwd_and_args(true, vec![], r"\\nas\share\proj").unwrap_err();
        assert!(err.contains("UNC"), "{err}");
    }

    #[test]
    fn finalize_non_wsl_keeps_local_drive() {
        let (args, cwd) =
            finalize_windows_cwd_and_args(false, vec!["run".into()], r"Z:\sample").unwrap();
        assert_eq!(args, vec!["run"]);
        assert_eq!(cwd, r"Z:\sample");
    }

    #[test]
    fn candidate_score_prefers_nodejs_over_windowsapps() {
        let stub = PathBuf::from(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\node.exe");
        let real = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
        assert!(candidate_score(&real) > candidate_score(&stub));
    }

    #[test]
    fn candidate_score_prefers_exe_over_cmd() {
        let exe = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
        let cmd = PathBuf::from(r"C:\Program Files\nodejs\npm.cmd");
        assert!(candidate_score(&exe) > candidate_score(&cmd));
    }

    #[test]
    fn pick_best_skips_windowsapps_when_real_exists() {
        let stub = PathBuf::from(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\node.exe");
        let real = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
        // is_unusable_stub checks filesystem for 0-byte — WindowsApps path is always skipped.
        let picked = pick_best_candidate(vec![stub, real.clone()]);
        assert_eq!(picked, Some(real));
    }

    #[test]
    fn pick_best_returns_none_when_only_windowsapps() {
        let stub = PathBuf::from(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\node.exe");
        assert_eq!(pick_best_candidate(vec![stub]), None);
    }

    #[test]
    fn status_dll_init_failed_exit_code_maps() {
        // Confirm the signed mapping the UI shows: 0xC0000142 → -1073741502
        let code = 0xC0000142u32 as i32;
        assert_eq!(code, -1073741502);
    }

    #[test]
    fn build_wsl_argv_full() {
        let (exe, args) = build_wsl_argv(
            "npm",
            &["run".into(), "dev".into()],
            Some("/home/user/projects/my-app"),
            Some("Ubuntu"),
        )
        .unwrap();
        assert!(
            exe.eq_ignore_ascii_case("wsl")
                || exe.to_ascii_lowercase().ends_with(r"\wsl.exe")
                || exe.to_ascii_lowercase().ends_with("/wsl.exe"),
            "exe={exe}"
        );
        assert_eq!(
            args,
            vec![
                "--shell-type",
                "login",
                "-d",
                "Ubuntu",
                "--cd",
                "/home/user/projects/my-app",
                "--",
                "npm",
                "run",
                "dev"
            ]
        );
    }

    #[test]
    fn build_wsl_argv_default_distro_no_cwd() {
        let (exe, args) = build_wsl_argv("uname", &["-a".into()], None, Some("  ")).unwrap();
        assert!(
            exe.to_ascii_lowercase().contains("wsl"),
            "exe={exe}"
        );
        assert_eq!(
            args,
            vec!["--shell-type", "login", "--", "uname", "-a"]
        );
    }

    #[test]
    fn build_wsl_argv_splits_command_line() {
        let (exe, args) =
            build_wsl_argv("npm run develop", &[], Some("/opt/app"), None).unwrap();
        assert!(exe.to_ascii_lowercase().contains("wsl"), "exe={exe}");
        assert_eq!(
            args,
            vec![
                "--shell-type",
                "login",
                "--cd",
                "/opt/app",
                "--",
                "npm",
                "run",
                "develop"
            ]
        );
    }

    #[test]
    fn build_wsl_argv_rejects_nested_wsl() {
        let err = build_wsl_argv("wsl", &["-e".into(), "bash".into()], None, None).unwrap_err();
        assert!(err.contains("not wsl"), "{err}");
    }

    #[test]
    fn build_wsl_argv_converts_unc_cwd_to_linux_cd() {
        let (exe, args) = build_wsl_argv(
            "pnpm",
            &["run".into(), "dev".into()],
            Some(r"\\wsl.localhost\Ubuntu\home\user\projects\my-app"),
            None,
        )
        .unwrap();
        assert!(exe.to_ascii_lowercase().ends_with("wsl.exe") || exe.eq_ignore_ascii_case("wsl"));
        assert!(args.iter().any(|a| a == "--cd"));
        assert!(args.iter().any(|a| a == "/home/user/projects/my-app"));
        assert!(args.iter().any(|a| a == "-d"));
        assert!(args.iter().any(|a| a == "Ubuntu"));
        assert!(args.iter().any(|a| a == "--"));
        assert!(args.iter().any(|a| a == "pnpm"));
        assert!(!args.iter().any(|a| a.contains("wsl.localhost")));
    }

    #[test]
    fn build_wsl_argv_converts_single_slash_wsl_localhost() {
        let (_, args) = build_wsl_argv(
            "pnpm",
            &["run".into(), "dev".into()],
            Some(r"\wsl.localhost\Ubuntu\home\x\app"),
            Some("Ubuntu"),
        )
        .unwrap();
        assert!(args.iter().any(|a| a == "/home/x/app"));
        assert!(!args.iter().any(|a| a.contains("wsl.localhost")));
    }

    #[test]
    fn resolve_process_launch_wsl_full_plan() {
        use crate::core::types::LaunchConfig;
        let launch = LaunchConfig {
            wsl: true,
            command: Some("pnpm".into()),
            args: vec!["run".into(), "dev".into()],
            cwd: Some("/home/user/projects/my-app".into()),
            wsl_distro: Some("Ubuntu".into()),
            ..Default::default()
        };
        let result = resolve_process_launch(&launch);
        if cfg!(windows) {
            let (exe, args, cwd) = result.unwrap();
            assert!(
                exe.to_ascii_lowercase().ends_with("wsl.exe"),
                "exe must be wsl.exe, got {exe}"
            );
            assert!(args.iter().any(|a| a == "--cd"));
            assert!(args.iter().any(|a| a == "/home/user/projects/my-app"));
            assert!(args.iter().any(|a| a == "--"));
            assert!(args.iter().any(|a| a == "pnpm"));
            assert!(args.iter().any(|a| a == "--shell-type"));
            let windows_cwd = cwd.expect("windows cwd");
            assert!(!is_unc_path(&windows_cwd), "cwd must be local: {windows_cwd}");
            assert!(!windows_cwd.contains("wsl.localhost"));
            assert!(!windows_cwd.contains("wsl$"));
        } else {
            let err = result.unwrap_err();
            assert!(err.contains("Windows-only"), "{err}");
        }
    }

    #[test]
    fn resolve_process_launch_wsl_unc_cwd_still_linux_cd() {
        use crate::core::types::LaunchConfig;
        let launch = LaunchConfig {
            wsl: true,
            command: Some("pnpm".into()),
            args: vec!["run".into(), "dev".into()],
            cwd: Some(r"\\wsl.localhost\Ubuntu\home\user\projects\my-app".into()),
            wsl_distro: Some("Ubuntu".into()),
            ..Default::default()
        };
        // build_wsl_argv is pure — always test argv shape; resolve gates on cfg(windows).
        let (exe, args) = build_wsl_argv(
            launch.command.as_deref().unwrap(),
            &launch.args,
            launch.cwd.as_deref(),
            launch.wsl_distro.as_deref(),
        )
        .unwrap();
        assert!(exe.to_ascii_lowercase().contains("wsl"));
        assert!(args.iter().any(|a| a == "--cd"));
        assert!(args.iter().any(|a| a == "/home/user/projects/my-app"));
        assert!(args.iter().any(|a| a == "pnpm"));
        assert!(!args.iter().any(|a| a.contains(r"\\") || a.contains("wsl.localhost")));

        let result = resolve_process_launch(&launch);
        if cfg!(windows) {
            let (exe, args, cwd) = result.unwrap();
            assert!(exe.to_ascii_lowercase().ends_with("wsl.exe"));
            assert!(args.iter().any(|a| a == "/home/user/projects/my-app"));
            assert!(!is_unc_path(cwd.as_deref().unwrap_or("")));
        } else {
            assert!(result.unwrap_err().contains("Windows-only"));
        }
    }

    #[test]
    fn resolve_process_launch_wsl_windows_only() {
        use crate::core::types::LaunchConfig;
        let launch = LaunchConfig {
            wsl: true,
            command: Some("npm".into()),
            args: vec!["run".into(), "dev".into()],
            cwd: Some("/home/x/app".into()),
            wsl_distro: Some("Ubuntu".into()),
            ..Default::default()
        };
        let result = resolve_process_launch(&launch);
        if cfg!(windows) {
            let (exe, args, cwd) = result.unwrap();
            assert!(exe.to_ascii_lowercase().contains("wsl"));
            assert!(args.iter().any(|a| a == "--cd"));
            assert!(cwd.is_some());
            assert!(!is_unc_path(cwd.as_deref().unwrap_or("")));
        } else {
            let err = result.unwrap_err();
            assert!(err.contains("Windows-only"), "{err}");
        }
    }

    #[test]
    fn resolve_process_launch_rejects_process_mode_wsl_unc() {
        use crate::core::types::LaunchConfig;
        let launch = LaunchConfig {
            wsl: false,
            command: Some("pnpm".into()),
            args: vec!["run".into(), "dev".into()],
            cwd: Some(r"\\wsl.localhost\Ubuntu\home\x".into()),
            ..Default::default()
        };
        let err = resolve_process_launch(&launch).unwrap_err();
        assert!(err.contains("WSL") || err.contains("UNC"), "{err}");
    }

    #[test]
    fn resolve_interactive_shell_unix_uses_shell_env_or_bash() {
        use crate::core::types::LaunchConfig;
        let launch = LaunchConfig {
            cwd: Some("/tmp".to_string()),
            ..Default::default()
        };
        let (exe, args, cwd) = resolve_interactive_shell(&launch).unwrap();
        assert!(args.is_empty());
        assert_eq!(cwd.as_deref(), Some("/tmp"));
        #[cfg(not(windows))]
        {
            let expected = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            assert_eq!(exe, expected);
        }
        #[cfg(windows)]
        {
            assert_eq!(exe, "powershell.exe");
        }
    }

    #[test]
    fn resolve_process_launch_plain_passthrough() {
        use crate::core::types::LaunchConfig;
        let launch = LaunchConfig {
            command: Some("node".into()),
            args: vec!["app.js".into()],
            cwd: Some(r"C:\proj".into()),
            ..Default::default()
        };
        let (exe, args, cwd) = resolve_process_launch(&launch).unwrap();
        assert_eq!(exe, "node");
        assert_eq!(args, vec!["app.js"]);
        assert_eq!(cwd.as_deref(), Some(r"C:\proj"));
    }

    #[test]
    fn normalize_wsl_linux_cwd_plain_and_unc() {
        assert_eq!(
            normalize_wsl_linux_cwd("/home/x/app").unwrap(),
            ("/home/x/app".into(), None)
        );
        assert_eq!(
            normalize_wsl_linux_cwd(r"\\wsl$\Ubuntu\home\x\app").unwrap(),
            ("/home/x/app".into(), Some("Ubuntu".into()))
        );
    }

    #[test]
    fn parse_wsl_unc_accepts_single_leading_slash() {
        assert_eq!(
            parse_wsl_unc(r"\wsl.localhost\Ubuntu\home\x"),
            Some(("Ubuntu".into(), "/home/x".into()))
        );
    }

    #[test]
    fn wsl_launch_yaml_roundtrip_preserves_wsl_flag() {
        use crate::core::types::{LaunchConfig, ProgramConfig, ProjectsStore};
        let store = ProjectsStore {
            active_project: 0,
            projects: vec![crate::core::types::ProjectConfig {
                id: "p1".into(),
                name: "p1".into(),
                default_cwd: None,
                path_hint: None,
                active_program: 0,
                programs: vec![ProgramConfig {
                    id: "prog1".into(),
                    name: "api".into(),
                    launch: LaunchConfig {
                        wsl: true,
                        wsl_distro: Some("Ubuntu".into()),
                        command: Some("pnpm".into()),
                        args: vec!["run".into(), "dev".into()],
                        cwd: Some("/home/user/projects/my-app".into()),
                        ..Default::default()
                    },
                    workspace: Default::default(),
                }],
            }],
        };
        let yaml = serde_yaml::to_string(&store).unwrap();
        assert!(yaml.contains("wsl: true") || yaml.contains("wsl:true"), "{yaml}");
        let parsed: ProjectsStore = serde_yaml::from_str(&yaml).unwrap();
        let launch = &parsed.projects[0].programs[0].launch;
        assert!(launch.wsl);
        assert_eq!(launch.wsl_distro.as_deref(), Some("Ubuntu"));
        assert_eq!(launch.command.as_deref(), Some("pnpm"));
        assert_eq!(
            launch.cwd.as_deref(),
            Some("/home/user/projects/my-app")
        );
    }
}