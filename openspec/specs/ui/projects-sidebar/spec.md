# projects-sidebar Specification

## Purpose

Edit Launch chrome on TERMINALS rows: saved command, args, cwd, and on Windows
WSL mode. Projects listing lives under File → Projects, not this sidebar.

## Requirements

### Requirement: File Projects overlay

The UI SHALL provide a **File → Projects…** command that opens an overlay listing saved Projects. The user SHALL be able to open, create, rename, and delete Projects from that overlay. The sidebar SHALL NOT list Projects. When a Project is active, the sidebar SHALL show that Project’s name above the TERMINALS section. Selection chrome SHALL NOT use accent borders (background tint only if needed).

#### Scenario: Open from list

- **WHEN** the user activates a Project in the File → Projects list
- **THEN** the engine opens that Project
- **AND** TERMINALS reflects the restored Programs
- **AND** the overlay closes

#### Scenario: Create from overlay

- **WHEN** the user creates a Project from the overlay
- **THEN** the engine creates an empty Project and opens it
- **AND** the overlay list includes the new Project

#### Scenario: Active project name above TERMINALS

- **WHEN** a Project is active
- **THEN** the sidebar shows that Project’s name above the TERMINALS section
- **AND** the name is hidden when no Project is active

### Requirement: Run and Stop on Terminal rows

Each live Terminal row in TERMINALS SHALL expose Run when not running and Stop when running. Activating Run or Stop SHALL invoke the engine for that Terminal id (including when the row is not the active session).

#### Scenario: Run from inactive row

- **WHEN** Terminal B is not active and is stopped with a saved command
- **AND** the user clicks Run on Terminal B’s row
- **THEN** Terminal B’s process starts

### Requirement: Edit Launch

The UI SHALL allow editing a live Terminal’s launch `command`, `args`, and
`cwd`. On Windows the dialog SHALL also expose WSL mode (checkbox, no Unicode
icon glyphs) and an optional distro field when WSL is on. Cwd shown in the
dialog SHALL be the saved launch cwd, not the live session cwd. Clearing the
command SHALL leave a shell-only Program (WSL bash when WSL is on). On Linux
the WSL controls SHALL be hidden.

#### Scenario: Set command via Edit Launch

- **WHEN** the user sets a Terminal’s launch command to `npm` with args `run` `dev` and a cwd
- **THEN** subsequent Run uses that launch config
- **AND** the active Project’s matching Program is updated in the projects store

#### Scenario: Enable WSL in Edit Launch

- **GIVEN** the app is running on Windows
- **WHEN** the user enables WSL, sets command `uname`, args `-a`, and a Linux cwd, then saves
- **THEN** subsequent Start uses WSL launch mode
- **AND** the active Project’s matching Program stores `wsl: true`
