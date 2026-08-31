## MODIFIED Requirements

### Requirement: Incremental volatile flat lines

When PTY bytes change only the live VT screen of the active terminal, the engine SHALL update the Terminal tab visible lines by replacing that live overlay and MUST NOT rebuild flat lines from the entire committed scrollback solely because of that echo. The live overlay MUST NOT be stored as Records in the session buffer.

#### Scenario: Single-byte echo with large scrollback

- **WHEN** the Terminal tab already contains a large committed scrollback and a single echoed byte updates the live screen
- **THEN** the engine updates Terminal tab visible lines for the live overlay without a full-buffer flat-line rebuild
- **AND** the Record buffer length is unchanged by that echo
