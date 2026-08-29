## Purpose

Defines how live Terminal PTY output must stay UI-responsive under floods (e.g. `cat` of tens of MB) via backpressure and bounded per-tick ingest, without raising the scrollback retention cap.

## Requirements

### Requirement: PTY reader backpressure

The engine SHALL bound pending live-PTY output so that when the bound is reached, the PTY reader stops accepting more bytes until the UI drains work. The bound MUST be large enough for interactive typing bursts and MUST NOT grow without limit with writer speed.

#### Scenario: Flood stalls the writer instead of growing RAM unboundedly

- **WHEN** a live Terminal child writes a large continuous stream faster than the UI can ingest
- **THEN** pending PTY output remains within a fixed bound and the child writer is stalled by the PTY rather than buffering the entire stream in process memory

### Requirement: Bounded ingest per UI tick

On each engine poll, the engine SHALL ingest at most a fixed budget of PTY bytes into the active Terminal’s VT/scrollback path. Remaining pending bytes MUST be left for subsequent polls. While pending PTY bytes remain, the host SHALL continue scheduling polls promptly so Follow can catch up without a multi-second UI stall.

#### Scenario: Large cat does not freeze a single tick

- **WHEN** tens of megabytes of PTY output are pending
- **THEN** a single UI poll processes only up to the ingest budget and returns while work remains
- **AND** subsequent polls continue until the pending output is drained or the session ends

#### Scenario: Interactive echo still updates promptly

- **WHEN** the user types into a running Terminal tab and the shell echoes a small amount of output
- **THEN** that output is ingested and visible without waiting for a flood budget to fill

### Requirement: Scrollback ring trim is efficient at cap

When a live Terminal’s record buffer is at its configured scrollback cap and new records arrive, the engine SHALL drop oldest records without performing work proportional to the full retained capacity per dropped record. The configured max scrollback (default 10 000, clamp ≤ 30 000) MUST remain unchanged by this capability.

#### Scenario: Sustained output at scrollback cap

- **WHEN** a live Terminal is already at its max scrollback and continues receiving line output
- **THEN** the UI remains responsive and retained records stay within the configured cap

### Requirement: Flat lines stay consistent under ring trim

When ring trim drops oldest raw lines from a live Terminal buffer, the Terminal tab’s flat-line cache MUST drop the matching prefix (or rebuild) so visible lines stay consistent with the buffer. Volatile-tail incremental updates from console-latency MUST continue to apply for echo when only the live screen changed.

When Follow is off and a prefix of flat lines is dropped, the engine MUST reduce `scroll_offset_y` by the visual height of the dropped prefix (clamped at 0) so the viewport stays anchored on the same logical content. When Follow is on, the engine MAY keep pinning to the bottom instead.

#### Scenario: Trim during flood with Follow

- **WHEN** Follow is on and flood output causes scrollback trim
- **THEN** the Terminal tab Viewport continues to show a coherent tail without requiring the entire historical flat-line list to be rebuilt on every trimmed line

#### Scenario: Scrolled-up view stays anchored under ring eviction

- **WHEN** Follow is off, the user has scrolled away from the bottom, and ring trim drops oldest flat lines
- **THEN** `scroll_offset_y` decreases by the dropped prefix height so the same lines remain under the viewport rather than sliding

### Requirement: Viewport paint under flood

When the active Terminal ingests PTY bytes, the engine MUST mark the Viewport dirty for that tick (no paint-rate throttle that skips frames while content advances). Ingest MAY still be budgeted per tick; painting MUST keep up with ingested updates so Follow and scroll do not jump between omitted frames.

#### Scenario: Active ingest paints

- **WHEN** the active Terminal receives a budgeted PTY ingest that changes the buffer
- **THEN** the Viewport is marked dirty on that same poll

#### Scenario: Catch-up after flood

- **WHEN** a flood ends and the PTY queue drains to empty
- **THEN** the Viewport reflects the final scrollback tail without leaving a stale frame indefinitely
