## ADDED Requirements

### Requirement: Reload file from disk

The user SHALL be able to reload an open file session from its saved path on disk. Reload SHALL replace the in-memory file window and reindex the file. Filter Tabs on that session SHALL remain. File sessions SHALL remain without Follow. If the path is missing or unreadable, the engine SHALL report a status error and SHALL keep the file session.

#### Scenario: Reload picks up new lines

- **WHEN** a file session is open for a path whose contents have changed on disk
- **AND** the user reloads that file session
- **THEN** the Viewport reflects the current file contents

#### Scenario: Reload missing file keeps session

- **WHEN** a file session is open
- **AND** the path is missing on disk
- **AND** the user reloads that file session
- **THEN** a status error is reported
- **AND** the file session remains in FILES
