## Purpose

Behavior of read-only log-file sessions as distinct from live PTY terminals: naming, Follow, close rules, and stats exposure for the FILES sidebar.

## Requirements

### Requirement: File sessions are distinct from live terminals

A file session SHALL be a read-only log-file session without a PTY. Stats exposed to the UI SHALL list live terminals and file sessions separately so the FILES sidebar can bind without mixing kinds.

#### Scenario: Stats split

- **WHEN** the engine has at least one live terminal and one open file
- **THEN** stats include a terminals list containing only live sessions
- **AND** stats include a files list containing only file sessions

### Requirement: Primary tab name for files

For a file session, the primary Tab (index 0) display name SHALL be the basename of the opened log file, not the literal string `Terminal`. That primary Tab SHALL remain non-filter-editable and not user-renamable (same constraints as today's Terminal Tab).

#### Scenario: Tab shows filename

- **WHEN** the user opens `/var/log/app.log` as a file session
- **THEN** the primary Tab label is `app.log`

### Requirement: No Follow on file sessions

File sessions SHALL NOT use auto-follow. The UI SHALL hide or disable Follow controls while a file session is active. Engine Follow commands SHALL not stick the file Viewport to the end based on follow state.

#### Scenario: Follow inactive for files

- **WHEN** a file session is active
- **THEN** Follow does not auto-scroll the Viewport to the end as new window data loads
- **AND** Follow chrome is not offered as an active control for that session

### Requirement: Close rules

The user SHALL be able to close any file session. The last remaining **live** terminal SHALL NOT be closable. The FILES list MAY be empty.

#### Scenario: Close last file

- **WHEN** only one file session is open and the user closes it
- **THEN** the FILES list becomes empty
- **AND** live terminals (if any) remain

#### Scenario: Cannot close last live terminal

- **WHEN** only one live terminal remains
- **THEN** closing that live terminal is refused
