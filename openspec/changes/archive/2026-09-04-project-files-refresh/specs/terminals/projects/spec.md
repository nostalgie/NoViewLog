## MODIFIED Requirements

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

## ADDED Requirements

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
