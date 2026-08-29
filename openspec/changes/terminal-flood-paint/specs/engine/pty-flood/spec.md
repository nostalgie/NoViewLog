## MODIFIED Requirements

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
