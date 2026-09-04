# NoViewLog

NoViewLog is a native desktop log viewer: a Rust engine plus a **Slint** UI.
There is no WebView or separate frontend. The log viewport is rendered by Rust
(`fontdue` bitmap).

**Status:** active development. **Linux** and **Windows** are equally
supported. macOS and other OSes are best-effort.

## Features

### Sessions

- Launch a command through a PTY (`bash scripts/run-slint.sh -- …` on Linux,
  `.\scripts\run-slint-windows.ps1 -- …` on Windows) or open a log file (CLI
  path, `--file` / `-f`, or File → Open log file)
- Run multiple independent terminals under **TERMINALS**; log files under **FILES** — switching the viewport does not stop other live sessions
- Type into a live process on the Terminal tab; copy and paste (including middle-click paste)
- Open a log in a dedicated view-only file session (no PTY or stdin); reopening the same path switches to it and reloads
- Sidebar: add or close sessions, rename **TERMINALS** rows (FILES are not renamable), Start/Stop or Refresh, drag-reorder TERMINALS; working directory updates via OSC 7

### Projects

- **File → Projects…** — create, open, rename, or delete Projects (Programs with launch + filter tabs)
- Opening a Project (or restoring the last Project on startup) replaces TERMINALS and FILES, selects the **Terminal** tab on each live session, and **auto-starts** processes / shells / file loads (no extra Start click)
- On Windows, Edit Launch can enable **WSL** so a Program’s command (or an empty-command bash) runs inside a distro
- See [`docs/terminals.md`](docs/terminals.md) (Persistence → Project open / cold start)

### Tabs and filters

- Each session has a pinned primary tab (**Terminal** for live sessions, file basename for FILES) plus optional filter tabs
- Include and exclude rules (literal or regex): add, toggle, edit, and remove; the draft highlights matches while you type
- After include/exclude, a per-tab severity mode: All, Errors, Warnings, Info, Debug, or Unleveled
- Add, close, restore the last closed tab, rename, and drag-reorder filter tabs (the primary tab stays at index 0)

### Find and viewport

- Find bar (Ctrl/Cmd+F): case, whole word, and regex; next/previous match and match count
- Follow live output; wrap lines or scroll horizontally; virtualized viewport for large files
- Zoom (View menu, Ctrl/Cmd +/−/0, or Ctrl+wheel); font size is saved in config
- Select text (drag, double-click a word, triple-click a record) and copy
- ANSI colors; multiline records such as stack traces are grouped and collapsed by default (click to expand, or View → Expand/Collapse all)
- Severity gutter cues on leveled records

### Config

- User config: `~/.config/noviewlog/config.yaml` (Windows: `%USERPROFILE%\.config\noviewlog\config.yaml`)
- Projects store: `~/.config/noviewlog/projects.yaml` (Windows: `%USERPROFILE%\.config\noviewlog\projects.yaml`)
- Settings: maximum scrollback lines
- Bundled filter presets in [`presets/defaults.yaml`](presets/defaults.yaml)
  (`node-dev`, `node-errors`, `php-dev`, `php-errors`, `python-dev`,
  `python-errors`, `go-errors`, `nginx-access`, `docker-compose`). Apply at
  launch with `--preset` / `-p` (no in-app preset manager yet)
- Edit or add presets under `presets:` in your user config — same id overrides
  the bundled definition; new ids are added. Bundled presets you omit still load
- Other CLI flags: `--file` / `-f`, `--config` / `-c`

## Run (Linux)

Requires [Rust](https://rustup.rs/) and native build tools. On Ubuntu:

```bash
sudo apt install build-essential pkg-config libssl-dev
```

```bash
bash scripts/run-slint.sh
bash scripts/run-slint.sh -- app.log
bash scripts/run-slint.sh -- npm run dev
bash scripts/run-slint.sh -- --preset node-dev -- node server.js
```

This builds and runs `noviewlog-slint` with the daily `release-dev` profile
(`opt-level = 3`, incremental, no fat LTO). First run may fetch local fontconfig
deps via `scripts/setup-slint-deps.sh` into `.deps/`.

Arguments after `--` are executed in a PTY. Each terminal has its own Terminal
tab and filter tabs; switching terminals does not stop background sessions.
See [`docs/terminals.md`](docs/terminals.md) and
[`docs/architecture.md`](docs/architecture.md).

The bundled presets live in [`presets/defaults.yaml`](presets/defaults.yaml)
(default `node-dev`). User configuration is stored in
`~/.config/noviewlog/config.yaml` — add or override entries under `presets:`.
Projects live in `~/.config/noviewlog/projects.yaml` (see
[`docs/terminals.md`](docs/terminals.md)).

## Run (Windows)

Prerequisites: Windows 10+ x64.

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   with the **Desktop development with C++** workload.
2. Install Rust via **rustup-init.exe** from
   [https://win.rustup.rs/x86_64](https://win.rustup.rs/x86_64). Keep the
   default host `x86_64-pc-windows-msvc`.

   Do **not** use the Linux/macOS `curl … | sh` rustup one-liner on Windows.

3. Verify the toolchain:

```powershell
rustc -vV
# host: x86_64-pc-windows-msvc
```

4. Build and run (PowerShell from the repo root):

```powershell
.\scripts\run-slint-windows.ps1
.\scripts\run-slint-windows.ps1 -- README.md
.\scripts\run-slint-windows.ps1 -- npm run dev
.\scripts\run-slint-windows.ps1 -- --preset node-dev -- node server.js
```

This builds and runs `noviewlog-slint` with the daily `release-dev` profile
(same as Linux: `opt-level = 3`, incremental, no fat LTO). The script imports
the MSVC environment when `link.exe` is not already in `PATH`.

Build only (after MSVC env is active):

```powershell
cargo build --profile release-dev -p noviewlog-slint
```

Binary: `target\release-dev\noviewlog-slint.exe`.

### Windows publish

Stage a copyable folder with fat LTO (`--release`). From PowerShell:

```powershell
.\scripts\publish-slint-windows.ps1
```

Git Bash (if you prefer the shell script):

```powershell
bash scripts/publish-slint-windows.sh
# or, if bash is not in PATH:
& "C:\Program Files\Git\bin\bash.exe" scripts/publish-slint-windows.sh
```

Output: `dist\noviewlog-slint-win-x64\NoViewLog.exe`. Copy that folder to the
target machine and run `NoViewLog.exe`.

Do **not** use the Linux `run-slint.sh` / fontconfig `.deps` helpers on Windows.

User config on Windows: `%USERPROFILE%\.config\noviewlog\config.yaml`.
Projects: `%USERPROFILE%\.config\noviewlog\projects.yaml`.

## Other platforms (best-effort)

macOS and other OSes may work but are not actively tested.

## Tests

```
cargo test -p noviewlog-core --lib
cargo test -p noviewlog-slint --lib --test inline_rename_wiring --test chrome_icon_wiring
```

On Windows CI (`.github/workflows/windows-slint.yml`) the same core/Slint tests run,
plus `cargo build --profile release-dev -p noviewlog-slint`.

## Architecture

| Path | Role |
|------|------|
| [`crates/noviewlog-core/`](crates/noviewlog-core/) | Engine: PTY, filters, buffer, fontdue viewport, Projects |
| [`crates/noviewlog-slint/`](crates/noviewlog-slint/) | Slint desktop UI |
| [`docs/architecture.md`](docs/architecture.md) | Engine ↔ UI boundary (commands, stats, paint) |
| [`docs/terminals.md`](docs/terminals.md) | TERMINALS / FILES / Projects open + auto-start |
| [`presets/`](presets/) | Bundled filter presets |
| [`assets/`](assets/) | App icon + bundled Noto Sans (UI) / Noto Sans Mono (viewport) |

## Filter logic

1. Exclude rules hide matching records.
2. If any include rule is active, a record must match at least one.
3. Exclude rules take precedence.

Severity is applied after include/exclude and is not saved in `config.yaml`.

## License

NoViewLog is licensed under the [MIT License](LICENSE).

The UI toolkit is [Slint](https://slint.dev), used under the
[Slint Royalty-free License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md).
Bundled Noto fonts are under the SIL Open Font License (see [`assets/OFL.txt`](assets/OFL.txt)).

<p align="center">
  <a href="https://slint.dev" target="_blank">
    <img src="https://github.com/slint-ui/slint/raw/master/logo/MadeWithSlint-logo-dark.png" alt="Made with Slint" />
  </a>
</p>
