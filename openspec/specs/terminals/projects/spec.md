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

#### Scenario: Empty project still has one terminal

- **WHEN** the user opens a Project that has zero Programs
- **THEN** TERMINALS still contains at least one live Terminal
- **AND** that Terminal is not running

#### Scenario: Open project restores FILES

- **WHEN** the user opens a Project that has a Program whose launch is a log file path
- **THEN** FILES shows a file session for that path
- **AND** that session does not appear under TERMINALS

#### Scenario: Open project replaces leftover FILES

- **WHEN** a file session is open that is not in the Project being opened
- **AND** the user opens that Project
- **THEN** that leftover file session is gone
- **AND** FILES matches the Project’s log-file Programs

### Requirement: Persist Projects store

Projects and Programs SHALL persist in the user projects store (`projects.yaml`). Creating a Project SHALL start with zero Programs (it SHALL NOT snapshot live TERMINALS). After create, the engine SHALL open that Project. While a Project is active, changes to program order, titles, launch, and tabs SHALL be saved back to that Project.

#### Scenario: Create starts empty

- **WHEN** the user creates a Project while live TERMINALS have launch commands and extra tabs
- **THEN** the new Project has zero Programs
- **AND** TERMINALS is replaced with one stopped Terminal that does not copy those launches or tabs

### Requirement: Active Project snapshots FILES as Programs

While a Project is active, the engine SHALL persist each FILES session as a Program with a log-file launch (path, display name, and Tabs). Adding, renaming, or closing a file session SHALL update that Project’s store. When no Project is active, FILES sessions SHALL NOT be written to the projects store.

#### Scenario: Open file while Project is active

- **WHEN** a Project is active
- **AND** the user opens a log file
- **THEN** that path is saved as a Program on the active Project

#### Scenario: Close file while Project is active

- **WHEN** a Project is active and a file session is saved on it
- **AND** the user closes that file session
- **THEN** that Program is removed from the Project store

### Requirement: Run and Stop for Program Terminals

The user SHALL be able to Run a live Terminal to start its saved launch command (or an interactive shell when no command is set). The user SHALL be able to Stop a live Terminal to kill its PTY. Stop and Run MAY target a non-active Terminal by id.

#### Scenario: Run starts saved command

- **WHEN** a stopped Terminal has a saved launch command
- **AND** the user Runs that Terminal
- **THEN** a PTY process starts with that command
- **AND** the Terminal reports running

#### Scenario: Stop leaves process-backed Terminal stopped

- **WHEN** a Terminal with a saved launch command is running
- **AND** the user Stops it
- **THEN** the PTY is killed
- **AND** the Terminal reports not running
- **AND** no interactive shell is auto-started for that Terminal

### Requirement: Project open restores stopped Programs as Terminals

Opening a Project SHALL replace the live TERMINALS session list with one Terminal per live Program in that Project, and SHALL replace the FILES session list with one file session per Program whose launch is a log file. Each session SHALL receive that Program’s launch config and filter tabs from the Program’s tab snapshot. Terminals SHALL be restored with `running` false. Opening SHALL always leave at least one live Terminal.

#### Scenario: Open project restores stopped terminals

- **WHEN** the user opens a Project that has two Programs with launch commands and filter tabs
- **THEN** TERMINALS shows two live sessions matching those Programs
- **AND** neither session is running
- **AND** each session’s non-primary Tabs match the saved filter tabs

#### Scenario: Empty project still has one terminal

- **WHEN** the user opens a Project that has zero Programs
- **THEN** TERMINALS still contains at least one live Terminal
- **AND** that Terminal is not running

#### Scenario: Open project restores FILES

- **WHEN** the user opens a Project that has a Program whose launch is a log file path
- **THEN** FILES shows a file session for that path
- **AND** that session does not appear under TERMINALS

#### Scenario: Open project replaces leftover FILES

- **WHEN** a file session is open that is not in the Project being opened
- **AND** the user opens that Project
- **THEN** that leftover file session is gone
- **AND** FILES matches the Project’s log-file Programs

### Requirement: No shell respawn after process exit when command is set

When a Terminal has a saved launch `command` and its PTY child exits (including after Stop), the engine SHALL NOT automatically start an interactive shell for that Terminal. Terminals without a saved command MAY keep interactive shell respawn behavior.

#### Scenario: Process exit stays stopped

- **WHEN** a Terminal with a saved launch command was running
- **AND** the child process exits
- **THEN** the Terminal reports not running
- **AND** no new interactive shell PTY is started for that Terminal

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
