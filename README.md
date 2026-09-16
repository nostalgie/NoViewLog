# NoViewLog

NoViewLog is a native desktop live log workspace for developers who run builds, dev
servers, and other processes locally and need to actually read their output:
launch a command through a PTY or open a log file, slice it with filter tabs,
follow live output, and search large logs fast. The UI is **Slint** rendered by
a Rust engine — no WebView, no browser; the log viewport is drawn by Rust
(`fontdue` bitmap). It is not a log-collection or monitoring tool: it does not
aggregate logs from remote machines, ship them anywhere, or alert.

**Status:** active development. **Linux** and **Windows** are equally
supported; other OSes are best-effort.

## Rendering

The log viewport is a fixed monospace grid drawn in Rust (`fontdue` bitmaps —
no WebView). Each advance character occupies one cell; that is the layout
model for width, wrapping, and selection.

What the viewport actually renders today:

- **ANSI colors** — SGR: 16 colors, 256-color, and truecolor; bold, underline, and dim are applied.
- **Emoji** — when Noto Color Emoji (CBDT) is installed, base emoji, skin-tone modifiers, ZWJ sequences (👨‍👩‍👧‍👦, 👩‍💻), flag pairs, and keycaps are painted as composite glyphs; without it, a monochrome Noto Sans Symbols fallback covers symbols and everything else falls through to the mono font.
- **Combining diacritics** — Latin, Arabic, Hebrew, Devanagari, and Thai marks share the base character's cell and never advance the column.
- **OSC 8 hyperlinks** — links emitted by tools (GitHub Actions, docker build, cargo, gh, git, `ls --hyperlink`) render underlined; a click without a drag selection opens them via the system handler.

What is **not** rendered (yet):

- **BiDi / RTL** — Arabic and Hebrew text renders in logical order, left to right, without shaping (no UAX #9 reordering, no bracket mirroring).
- **CJK / wide glyphs** — no double-width measuring; CJK text can misalign columns and tables.
- **SGR italic / reverse / strike** — parsed away, not styled.

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
- Opening a Project (or restoring the last Project on startup) replaces TERMINALS and FILES, selects the **Terminal** tab on each live session, and leaves Programs **stopped** until Start (FILES may begin load)
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

## Run

Requires [Rust](https://rustup.rs/). Both platforms build and run
`noviewlog-slint` with the daily `release-dev` profile; the run scripts build
on first launch, so there is no separate build step unless you want one:
`cargo build --profile release-dev -p noviewlog-slint`.

### Linux

Native build tools are required. On Ubuntu:

```bash
sudo apt install build-essential pkg-config libssl-dev
```

```bash
bash scripts/run-slint.sh
bash scripts/run-slint.sh -- app.log
bash scripts/run-slint.sh -- npm run dev
bash scripts/run-slint.sh -- --preset node-dev -- node server.js
```

The first run may fetch local fontconfig deps via `scripts/setup-slint-deps.sh`
into `.deps/`. Binary: `target/release-dev/noviewlog-slint`.

### Windows

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

The script imports the MSVC environment when `link.exe` is not already in
`PATH`. Binary: `target\release-dev\noviewlog-slint.exe`.

Do **not** use the Linux `run-slint.sh` / fontconfig `.deps` helpers on
Windows.

#### Windows publish

Stage a copyable folder with fat LTO (`--release`):

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

## Tests

```
cargo test -p noviewlog-core --lib
cargo test -p noviewlog-slint --lib --test inline_rename_wiring --test chrome_icon_wiring
```

Run the tests locally; also rebuild with
`cargo build --profile release-dev -p noviewlog-slint` after Slint or engine UI
changes.

## Architecture

| Path | Role |
|------|------|
| [`crates/noviewlog-core/`](crates/noviewlog-core/) | Engine: PTY, filters, buffer, fontdue viewport, Projects |
| [`crates/noviewlog-slint/`](crates/noviewlog-slint/) | Slint desktop UI |
| [`docs/architecture.md`](docs/architecture.md) | Engine ↔ UI boundary (commands, stats, paint) |
| [`docs/terminals.md`](docs/terminals.md) | TERMINALS / FILES / Projects open + manual Start |
| [`presets/`](presets/) | Bundled filter presets |
| [`assets/`](assets/) | App icon + bundled Noto Sans (UI) / Noto Sans Mono (viewport) |

## Filter logic

1. Exclude rules hide matching records.
2. If any include rule is active, a record must match at least one.
3. Exclude rules take precedence.

Severity is applied after include/exclude and is not saved in `config.yaml`.

## Configuration

### File locations

- User config: `~/.config/noviewlog/config.yaml` (Windows: `%USERPROFILE%\.config\noviewlog\config.yaml`)
- Projects store: `~/.config/noviewlog/projects.yaml` (Windows: `%USERPROFILE%\.config\noviewlog\projects.yaml`)
- Settings: maximum scrollback lines

### Presets

- Bundled filter presets in [`presets/defaults.yaml`](presets/defaults.yaml)
  (`node-dev`, `node-errors`, `php-dev`, `php-errors`, `python-dev`,
  `python-errors`, `go-errors`, `nginx-access`, `docker-compose`)
- Edit or add presets under `presets:` in your user config — same id overrides
  the bundled definition; new ids are added. Bundled presets you omit still load

### CLI flags

- `--preset` / `-p` — apply a filter preset at launch (no in-app preset manager yet)
- `--file` / `-f` — open a log file
- `--config` / `-c` — use a different config file

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
