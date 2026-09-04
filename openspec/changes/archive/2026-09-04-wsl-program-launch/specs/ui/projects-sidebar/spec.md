## MODIFIED Requirements

### Requirement: Edit Launch

The UI SHALL allow editing a live Terminal’s launch `command`, `args`, and `cwd`. On Windows the dialog SHALL also expose WSL mode (checkbox, no Unicode icon glyphs) and an optional distro field when WSL is on. Cwd shown in the dialog SHALL be the saved launch cwd, not the live session cwd. Clearing the command SHALL leave a shell-only Program (WSL bash when WSL is on). On Linux the WSL controls SHALL be hidden.

#### Scenario: Set command via Edit Launch

- **WHEN** the user sets a Terminal’s launch command to `npm` with args `run` `dev` and a cwd
- **THEN** subsequent Run uses that launch config
- **AND** the active Project’s matching Program is updated in the projects store

#### Scenario: Enable WSL in Edit Launch

- **GIVEN** the app is running on Windows
- **WHEN** the user enables WSL, sets command `uname`, args `-a`, and a Linux cwd, then saves
- **THEN** subsequent Start uses WSL launch mode
- **AND** the active Project’s matching Program stores `wsl: true`
