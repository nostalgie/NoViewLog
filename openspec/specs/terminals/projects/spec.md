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

- Live Program with a saved `command` → start that process
- Live Program with no command → start an interactive shell
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
