## Purpose

Persist and restore Projects as groups of Programs mapped to live Terminals with saved launch config and filter tabs, including Run/Stop lifecycle without auto-start on open.

## ADDED Requirements

### Requirement: Project open restores stopped Programs as Terminals

Opening a Project SHALL replace the live TERMINALS session list with one Terminal per Program in that Project. Each Terminal SHALL receive that Program’s launch config and filter tabs from the Program’s tab snapshot. Terminals SHALL be restored with `running` false. FILES sessions SHALL remain unchanged. Opening SHALL always leave at least one live Terminal.

#### Scenario: Open project restores stopped terminals

- **WHEN** the user opens a Project that has two Programs with launch commands and filter tabs
- **THEN** TERMINALS shows two live sessions matching those Programs
- **AND** neither session is running
- **AND** each session’s non-primary Tabs match the saved filter tabs
- **AND** FILES sessions are unchanged

#### Scenario: Empty project still has one terminal

- **WHEN** the user opens a Project that has zero Programs
- **THEN** TERMINALS still contains at least one live Terminal
- **AND** that Terminal is not running

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

### Requirement: No shell respawn after process exit when command is set

When a Terminal has a saved launch `command` and its PTY child exits (including after Stop), the engine SHALL NOT automatically start an interactive shell for that Terminal. Terminals without a saved command MAY keep interactive shell respawn behavior.

#### Scenario: Process exit stays stopped

- **WHEN** a Terminal with a saved launch command was running
- **AND** the child process exits
- **THEN** the Terminal reports not running
- **AND** no new interactive shell PTY is started for that Terminal

### Requirement: Persist Projects store

Projects and Programs SHALL persist in the user projects store (`projects.yaml`). Creating a Project from current TERMINALS SHALL snapshot each live session’s launch and tabs into Programs. While a Project is active, changes to program order, titles, launch, and tabs SHALL be saved back to that Project.

#### Scenario: Create and reopen

- **WHEN** the user creates a Project from two live Terminals with distinct commands and tabs
- **AND** later opens that Project
- **THEN** both Programs’ commands and filter tabs are restored as stopped Terminals
