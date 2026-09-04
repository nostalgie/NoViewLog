## Purpose

Project open and cold-start behavior for Programs mapped to TERMINALS / FILES.
Same on Linux and Windows.

## Requirements

### Requirement: Live sessions open on the Terminal tab

When a Project is opened or restored, every restored **live** TERMINALS session
SHALL select the primary **Terminal** tab (`active_view == 0`), even if
`projects.yaml` stored a filter tab as `active_tab`. Filter tabs SHALL remain in
the tab strip. FILES sessions MAY keep a restored filter tab.

#### Scenario: Saved filter tab is not selected on open

- **GIVEN** a Program saved with `active_tab` pointing at a filter tab
- **WHEN** the user opens that Project (or the app restores it on startup)
- **THEN** the viewport shows the Terminal tab for the active live session
- **AND** the filter tabs are still present in the strip

### Requirement: Project open leaves live Programs stopped

Opening or restoring a Project SHALL restore live Programs **stopped**. The user
MUST press Start on the TERMINALS row to start a saved `command` (on Windows,
`wsl: true` runs that command inside WSL). Live Programs with no command SHALL
NOT spawn an interactive shell until the user types on the Terminal tab or
explicitly Starts.

FILES Programs MAY begin loading the log file on open.

Empty Projects (zero Programs) SHALL still create one live Terminal that is
**not** running.

CLI launch (`noviewlog-slint -- cmd …` / `finish_startup` with a process or
log-file launch) MAY start that one-shot session. That is an explicit launch
argument, not Project restore. Restoring the last Project on startup MUST NOT
auto-start live Programs.

#### Scenario: Cold open stays stopped

- **GIVEN** a Project with a live Program that has a saved launch command
- **WHEN** the user opens the Project or the app restores it on startup
- **THEN** that session is not running and no PTY is spawned
- **AND** the active tab is Terminal
- **AND** Start on the TERMINALS row starts the saved command

#### Scenario: Blank Program stays stopped

- **GIVEN** a live Program with no saved command
- **WHEN** the Project is opened
- **THEN** no interactive shell PTY is started
- **AND** typing on the Terminal tab MAY start an interactive shell

### Requirement: WSL launch mode on Programs

A live Program SHALL be able to record WSL launch mode (`launch.wsl`) and an
optional distribution (`launch.wsl_distro`). When `wsl` is true on Windows,
Start SHALL spawn via `wsl.exe` so Command/Args run inside the distro. `cwd`
SHALL be passed as a Linux `--cd` path (or converted from `\\wsl$\Distro\…`).
Empty Command SHALL start an interactive login bash in that distro. On
non-Windows hosts, starting a WSL Program SHALL fail with a clear error.
Saving Edit Launch SHALL NOT clear `wsl` / `wsl_distro`.

#### Scenario: Saved WSL command starts inside the distro

- **GIVEN** a live Program with `wsl: true`, command `uname`, args `-a`, and a Linux cwd
- **WHEN** the user Starts that Terminal on Windows
- **THEN** the PTY child is `wsl.exe` with the Linux command after `--`
- **AND** viewport output is the Linux `uname` result (not WSL help text, not a Windows binary)

#### Scenario: Empty WSL command is a distro shell

- **GIVEN** a live Program with `wsl: true` and no command
- **WHEN** the user Starts that Terminal on Windows
- **THEN** an interactive bash inside the distro is started

#### Scenario: Edit Launch keeps WSL flags

- **GIVEN** a Terminal with WSL launch enabled and a distro name
- **WHEN** the user saves Edit Launch (command, args, cwd, WSL on, distro)
- **THEN** the Program in `projects.yaml` still has `wsl: true` and that distro

### Requirement: Manual Stop does not auto-respawn saved commands

After the user Stops a session that has a saved launch `command`, or that
process exits, the engine SHALL NOT auto-start an interactive shell for that
session. Manual Start remains available.

#### Scenario: Stop stays stopped

- **WHEN** a Terminal with a saved launch command is stopped or its child exits
- **THEN** the session reports not running
- **AND** no new interactive shell is started automatically

### Requirement: Empty viewport hints are the stopped-open path

Empty-center viewport messages for a stopped empty buffer SHALL use ASCII text
and MUST NOT imply that Start lives on a filter tab. Project open of a live
Program SHALL leave the session stopped, so these hints (`EMPTY_TERMINAL_TAB_STOPPED`
and the filter-tab variant) ARE the open path — not fallback-only copy.

#### Scenario: Filter-tab empty copy has no play glyph

- **GIVEN** a stopped live session somehow shows an empty filter tab
- **THEN** the center message points at Start on the TERMINALS row
- **AND** the message does not contain a Unicode play glyph

#### Scenario: Terminal tab hint after Project open

- **GIVEN** a restored live Program with a saved command and an empty buffer
- **WHEN** the Project is opened
- **THEN** the Terminal tab shows the stopped empty-buffer hint
- **AND** the session is not running
