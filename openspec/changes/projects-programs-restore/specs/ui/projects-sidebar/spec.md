## Purpose

Slint chrome for listing and opening Projects, and for Run/Stop plus minimal launch editing on Terminal rows in the sidebar.

## ADDED Requirements

### Requirement: PROJECTS sidebar section

The UI SHALL provide a collapsible PROJECTS section listing saved Projects. The user SHALL be able to open, create, rename, and delete Projects from that section. Selection chrome SHALL NOT use accent borders (background tint only if needed).

#### Scenario: Open from list

- **WHEN** the user activates a Project in the PROJECTS list
- **THEN** the engine opens that Project
- **AND** TERMINALS reflects the restored Programs

### Requirement: Run and Stop on Terminal rows

Each live Terminal row in TERMINALS SHALL expose Run when not running and Stop when running. Activating Run or Stop SHALL invoke the engine for that Terminal id (including when the row is not the active session).

#### Scenario: Run from inactive row

- **WHEN** Terminal B is not active and is stopped with a saved command
- **AND** the user clicks Run on Terminal B’s row
- **THEN** Terminal B’s process starts

### Requirement: Edit Launch

The UI SHALL allow editing a live Terminal’s launch `command`, `args`, and `cwd` without WSL toggles in v1. Clearing the command SHALL leave a shell-only Program.

#### Scenario: Set command via Edit Launch

- **WHEN** the user sets a Terminal’s launch command to `npm` with args `run` `dev` and a cwd
- **THEN** subsequent Run uses that launch config
- **AND** the active Project’s matching Program is updated in the projects store
