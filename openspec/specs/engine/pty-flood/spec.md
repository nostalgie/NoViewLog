## Purpose

Defines how live Terminal PTY output must stay UI-responsive under floods (e.g. `cat` of tens of MB) via backpressure and bounded per-tick ingest, without raising the scrollback retention cap.

## Requirements

### Requirement: PTY reader backpressure

The engine SHALL bound pending live-PTY output so that when the bound is reached, the PTY reader stops accepting more bytes until the UI drains work. The bound MUST be large enough for interactive typing bursts and MUST NOT grow without limit with writer speed.

#### Scenario: Flood stalls the writer instead of growing RAM unboundedly

- **WHEN** a live Terminal child writes a large continuous stream faster than the UI can ingest
- **THEN** pending PTY output remains within a fixed bound and the child writer is stalled by the PTY rather than buffering the entire stream in process memory

### Requirement: Bounded ingest per UI tick

On each engine poll, the engine SHALL ingest at most a fixed budget of PTY bytes into the active Terminal’s VT/scrollback path. Remaining pending bytes MUST be left for subsequent polls. While pending PTY bytes remain, the host SHALL continue polling at display cadence (~33 ms) so Follow can catch up without a multi-second UI stall and without a busy ingest loop. Those follow-up polls MAY skip Viewport rasterize when paint is not yet due.

#### Scenario: Large cat does not freeze a single tick

- **WHEN** tens of megabytes of PTY output are pending
- **THEN** a single UI poll processes only up to the ingest budget and returns while work remains
- **AND** subsequent polls continue until the pending output is drained or the session ends

#### Scenario: Flood ingest is not an event-loop busy-drain

- **WHEN** pending PTY bytes remain after a budgeted poll
- **THEN** the next ingest is not required before the display-cadence interval elapses
- **AND** a PTY reader wake during that interval MUST NOT force an immediate extra poll

#### Scenario: Ingest-only drain between paints

- **WHEN** pending PTY bytes remain and a Viewport paint is not due under the flood paint cadence
- **THEN** the host may poll and ingest without allocating or uploading a new Viewport Image
- **AND** the next poll is the display-cadence timer, not an immediate event-loop retick

### Requirement: Scrollback ring trim is efficient at cap

When a live Terminal’s record buffer is at its configured scrollback cap and new records arrive, the engine SHALL drop oldest records without performing work proportional to the full retained capacity per dropped record. The configured max scrollback (default 10 000, clamp ≤ 30 000) MUST remain unchanged by this capability.

#### Scenario: Sustained output at scrollback cap

- **WHEN** a live Terminal is already at its max scrollback and continues receiving line output
- **THEN** the UI remains responsive and retained records stay within the configured cap

### Requirement: Flat lines stay consistent under ring trim

When ring trim drops oldest raw lines from a live Terminal buffer, the Terminal tab’s committed flat-line prefix MUST drop the matching prefix (or rebuild) so visible lines stay consistent with the buffer. Live-screen overlay updates from console-latency MUST continue to apply for echo when only the live screen changed.

When Follow is off and a prefix of flat lines is dropped, the engine MUST reduce `scroll_offset_y` by the visual height of the dropped prefix (clamped at 0) so the viewport stays anchored on the same logical content. When Follow is on, the engine MAY keep pinning to the bottom instead.

#### Scenario: Trim during flood with Follow

- **WHEN** Follow is on and flood output causes scrollback trim
- **THEN** the Terminal tab Viewport continues to show a coherent tail without requiring the entire historical flat-line list to be rebuilt on every trimmed line

#### Scenario: Scrolled-up view stays anchored under ring eviction

- **WHEN** Follow is off, the user has scrolled away from the bottom, and ring trim drops oldest flat lines
- **THEN** `scroll_offset_y` decreases by the dropped prefix height so the same lines remain under the viewport rather than sliding

### Requirement: Viewport paint under flood

When the active Terminal ingests PTY bytes, the engine SHALL update scrollback state and, when Follow is on, SHALL pin `scroll_offset_y` to the current max scroll on that same poll even if a Viewport paint is deferred. The engine SHALL NOT mark the Viewport dirty for paint on every budgeted ingest under continuous flood; paint dirtying MUST be limited to approximately display cadence (on the order of 16–33 ms) while PTY work remains, except when a non-flood UI change requires an immediate paint (resize, tab/view switch, selection, wrap/geometry change, scroll-away). After a successful Viewport rasterize, the paint cadence clock MUST advance.

Ingest under continuous flood MUST also be limited to approximately display cadence. The host MUST NOT busy-loop `poll_pty` / event-loop reticks as fast as the PTY reader can wake. “Promptly” for leftover flood bytes means the next display-cadence tick (~33 ms), not an immediate ingest-only drain. Interactive echo (no leftover flood queue) MUST still wake immediately.

#### Scenario: Active ingest snaps Follow without painting every chunk

- **WHEN** the active Terminal receives several budgeted PTY ingests within one display frame interval and Follow is on
- **THEN** `scroll_offset_y` is pinned to max scroll after each ingest
- **AND** the Viewport is not required to be marked dirty for paint on every one of those ingests

#### Scenario: Paint cadence under flood

- **WHEN** a continuous PTY flood is being drained across many polls
- **THEN** Viewport paints occur at most about once per display cadence interval while the flood continues
- **AND** when a paint does occur, it reflects the Follow-snapped tail

#### Scenario: Catch-up after flood

- **WHEN** a flood ends and the PTY queue drains to empty
- **THEN** the Viewport reflects the final scrollback tail without leaving a stale frame indefinitely

#### Scenario: Interactive echo still updates promptly

- **WHEN** the user types into a running Terminal tab and the shell echoes a small amount of output
- **THEN** that output is ingested and becomes visible without waiting for a flood budget to fill
- **AND** a paint is scheduled promptly (not deferred for a full flood cadence when idle of flood)

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

### Requirement: Filter tabs show committed scrollback plus filtered live overlay

Live Terminal filter tabs SHALL rebuild from the session Record ring (at most `max_scrollback_lines`, ≤30k) when the user switches onto that tab, when its include/exclude or severity filters change, and when new rows commit while that tab is selected. After that rebuild, the engine SHALL apply the current live-screen overlay once as a filtered tail so short in-screen output (for example `uname`) is visible. Overlay frames MUST NOT become a Record per spinner tick. Overlay-only PTY updates MUST NOT replace the tab with only the current live screen or drop already-rebuilt committed matches. Inactive filter tabs SHALL stay stale until selected. File-session filter tabs keep the whole-file match index; live Terminals MUST NOT use that path.

#### Scenario: Unfiltered filter tab shows short live output

- **WHEN** a running Program has output still on the live screen (not yet scrolled into Records) and the user opens a filter tab with no include/exclude rules
- **THEN** that tab shows the live overlay text
- **AND** the Record buffer length does not increase solely because the overlay was shown

#### Scenario: Spinner does not add Records on a filter tab

- **WHEN** a process redraws a spinner on the live screen and a filter tab is open
- **THEN** the Terminal tab shows the spinner updating in place
- **AND** the filter tab does not gain a new Record for each spinner frame
- **AND** already-rebuilt committed matching lines on that filter tab remain

#### Scenario: Switch onto include filter after scrollback commit

- **WHEN** matching lines have scrolled off the VT screen into Records and the user switches onto an include filter tab
- **THEN** that tab shows those committed matching lines
- **AND** a later overlay-only live-screen update does not drop that committed prefix

### Requirement: Cheap committed ingest under flood

When committing scrolled-off rows into the Record buffer during a live PTY flood, the engine SHALL NOT perform per-line work that is redundant with the Terminal tab overlay path (in particular it MUST NOT rebuild the entire Terminal tab flat-line list from all retained Records on every committed batch). Terminal tab visible lines for newly committed rows MUST be derived once from those committed strings. Severity classification of firehose lines MUST NOT be required at ingest; the Terminal tab MAY classify only visible rows at paint, and filter or severity views MAY classify when they rebuild.

#### Scenario: Flood commit does not full-rebuild Terminal tab

- **WHEN** a large `cat` causes many rows to scroll off the screen across budgeted ingests and Follow is on
- **THEN** the Terminal tab Viewport updates from the live VT grid
- **AND** it does not rebuild or extend Terminal tab `flat_lines` from the entire scrollback on each of those ingests
