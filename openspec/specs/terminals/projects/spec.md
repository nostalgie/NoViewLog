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

### Requirement: Project open auto-starts sessions

Opening or restoring a Project SHALL start sessions without requiring the user
to press Start:

- Live Program with a saved `command` → start that process (on Windows,
  `wsl: true` runs the command inside WSL)
- Live Program with no command → start an interactive shell (WSL bash when
  `wsl: true`)
- FILES Program → begin loading the log file

Empty Projects (zero Programs) SHALL still create one live Terminal and start an
interactive shell.

#### Scenario: Cold open runs without Start

- **GIVEN** a Project with a live Program that has a saved launch command
- **WHEN** the user opens the Project or the app restores it on startup
- **THEN** that session’s process is started
- **AND** the active tab is Terminal

#### Scenario: Blank Program gets a shell

- **GIVEN** a live Program with no saved command
- **WHEN** the Project is opened
- **THEN** an interactive shell PTY is started for that session

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

### Requirement: Empty viewport hints are not the open path

Empty-center viewport messages for a stopped empty buffer SHALL use ASCII text
and MUST NOT imply that Start lives on a filter tab. Project open SHALL rely on
auto-start + Terminal tab selection, not on those hints.

#### Scenario: Filter-tab empty copy has no play glyph

- **GIVEN** a stopped live session somehow shows an empty filter tab
- **THEN** the center message points at Start on the TERMINALS row
- **AND** the message does not contain a Unicode play glyph
