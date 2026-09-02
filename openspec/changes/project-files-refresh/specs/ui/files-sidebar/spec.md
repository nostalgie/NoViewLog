## ADDED Requirements

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
