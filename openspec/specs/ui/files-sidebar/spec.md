## Purpose

Sidebar chrome that separates live PTY sessions from opened log files, with independent collapsible sections and an affordance to open a new file.

## Requirements

### Requirement: Dual sidebar sections

The Slint sidebar SHALL show a **TERMINALS** section and a **FILES** section. Live PTY sessions SHALL appear only under TERMINALS. Opened log-file sessions SHALL appear only under FILES.

#### Scenario: Open file appears under FILES

- **WHEN** the user opens a log file via File menu or the FILES `+` control
- **THEN** a row for that file appears under FILES
- **AND** that session does not appear under TERMINALS

#### Scenario: New shell appears under TERMINALS

- **WHEN** the user creates a new terminal via the TERMINALS `+` control
- **THEN** a live session row appears under TERMINALS
- **AND** no new FILES row is created

### Requirement: Collapsible sections

Both TERMINALS and FILES section headers SHALL be expandable and collapsible so the user can reclaim vertical space. Collapse state SHOULD persist across app restarts when config persistence is available.

#### Scenario: Collapse FILES

- **WHEN** the user collapses the FILES section
- **THEN** file session rows are hidden
- **AND** the FILES header and its `+` control remain available (or the header alone remains discoverable to expand again)

### Requirement: Open file from FILES

The FILES section SHALL provide a control that opens the platform file dialog (or equivalent) to load a log file into a file session.

#### Scenario: FILES plus opens dialog

- **WHEN** the user activates the FILES `+` control
- **THEN** the open-log-file flow starts (file dialog → load file session)

### Requirement: Refresh control on FILES rows

Each FILES sidebar row SHALL expose a Refresh control that reloads that file session from disk. Selection chrome SHALL NOT use Theme.accent as a border. The File menu SHALL offer Reload log while a file session is active.

#### Scenario: FILES row refresh

- **WHEN** a file session row is visible under FILES
- **AND** the user activates that row’s Refresh control
- **THEN** that file session is reloaded from its saved path

#### Scenario: File menu reload

- **WHEN** the active session is a file session
- **THEN** File menu includes Reload log
- **AND** activating it reloads the active file session
