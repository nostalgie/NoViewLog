## ADDED Requirements

### Requirement: Live screen is not stored as Records

The live VT screen of a running Terminal SHALL remain in the emulator grid. The session Record buffer SHALL receive only rows that have scrolled off that screen (committed scrollback). While Follow is on and the Terminal tab is active, the Viewport SHALL paint that cell grid directly. The engine MUST NOT append live overlay or committed firehose lines onto the Terminal tab `flat_lines` list on each ingest solely to keep Follow in view. When Follow is off, the Terminal tab MAY compose committed scrollback plus a live overlay.

#### Scenario: Overlay is visible without Records

- **WHEN** the PTY draws or updates the live screen without scrolling rows off the top
- **THEN** the Terminal tab Viewport shows those live rows
- **AND** the Record buffer length does not increase solely because of that live-screen update

#### Scenario: Follow flood does not grow Terminal tab flat lines

- **WHEN** Follow is on and a running Terminal ingests a PTY flood on the Terminal tab
- **THEN** Terminal tab `flat_lines` length does not grow with each ingest
- **AND** scrolled-off rows are still committed as Records

#### Scenario: Scrolled-off rows become Records

- **WHEN** live output causes rows to scroll off the top of the VT screen
- **THEN** those rows are committed into the Record buffer as scrollback

### Requirement: Filter tabs see committed scrollback only

Filter tabs SHALL present Records from committed scrollback. In-place live-screen frames (spinners, the current prompt line) MUST NOT appear on a filter tab until the corresponding rows have scrolled off the VT screen and been committed.

#### Scenario: Spinner stays on the Terminal tab

- **WHEN** a process redraws a spinner on the live screen and a filter tab is open
- **THEN** the Terminal tab shows the spinner updating in place
- **AND** the filter tab does not gain a new Record for each spinner frame

### Requirement: Cheap committed ingest under flood

When committing scrolled-off rows into the Record buffer during a live PTY flood, the engine SHALL NOT perform per-line work that is redundant with the Terminal tab overlay path (in particular it MUST NOT rebuild the entire Terminal tab flat-line list from all retained Records on every committed batch). Terminal tab visible lines for newly committed rows MUST be derived once from those committed strings. Severity classification of firehose lines MUST NOT be required at ingest; the Terminal tab MAY classify only visible rows at paint, and filter or severity views MAY classify when they rebuild.

#### Scenario: Flood commit does not full-rebuild Terminal tab

- **WHEN** a large `cat` causes many rows to scroll off the screen across budgeted ingests and Follow is on
- **THEN** the Terminal tab Viewport updates from the live VT grid
- **AND** it does not rebuild or extend Terminal tab `flat_lines` from the entire scrollback on each of those ingests

## MODIFIED Requirements

### Requirement: Flat lines stay consistent under ring trim

When ring trim drops oldest raw lines from a live Terminal buffer, the Terminal tab’s committed flat-line prefix MUST drop the matching prefix (or rebuild) so visible lines stay consistent with the buffer. Live-screen overlay updates from console-latency MUST continue to apply for echo when only the live screen changed.

When Follow is off and a prefix of flat lines is dropped, the engine MUST reduce `scroll_offset_y` by the visual height of the dropped prefix (clamped at 0) so the viewport stays anchored on the same logical content. When Follow is on, the engine MAY keep pinning to the bottom instead.

#### Scenario: Trim during flood with Follow

- **WHEN** Follow is on and flood output causes scrollback trim
- **THEN** the Terminal tab Viewport continues to show a coherent tail without requiring the entire historical flat-line list to be rebuilt on every trimmed line

#### Scenario: Scrolled-up view stays anchored under ring eviction

- **WHEN** Follow is off, the user has scrolled away from the bottom, and ring trim drops oldest flat lines
- **THEN** `scroll_offset_y` decreases by the dropped prefix height so the same lines remain under the viewport rather than sliding
