## Purpose

Defines how the Terminal tab must show typed and echoed characters promptly without full scrollback rebuilds on every PTY byte, so interactive typing feels comparable to a native terminal.

## Requirements

### Requirement: Prompt Terminal tab echo display

When the Terminal tab is active and the PTY produces output for the active terminal, the host SHALL wake the UI event loop so the engine can ingest and paint without waiting solely for the next interactive timer interval.

#### Scenario: Echo after keystroke is not timer-capped alone

- **WHEN** the user types a printable character into a running Terminal tab and the shell echoes it
- **THEN** the Viewport updates the echoed character without requiring a full idle-timer wait for the next scheduled poll

### Requirement: Incremental volatile flat lines

When PTY bytes change only the live VT screen of the active terminal, the engine SHALL update the Terminal tab visible lines by replacing that live overlay and MUST NOT rebuild flat lines from the entire committed scrollback solely because of that echo. The live overlay MUST NOT be stored as Records in the session buffer.

#### Scenario: Single-byte echo with large scrollback

- **WHEN** the Terminal tab already contains a large committed scrollback and a single echoed byte updates the live screen
- **THEN** the engine updates Terminal tab visible lines for the live overlay without a full-buffer flat-line rebuild
- **AND** the Record buffer length is unchanged by that echo

### Requirement: Caret blink does not force perpetual fast paint

While the Terminal tab caret is focused and idle (no viewport dirtiness from content), the host MUST NOT keep a perpetual ~30 Hz full Viewport paint schedule solely to blink the caret. Resetting the blink clock while the caret is already visible MUST NOT mark the Viewport dirty.

#### Scenario: Idle focused Terminal tab

- **WHEN** the Terminal tab has focus, a visible caret, and no content change
- **THEN** the host does not continuously repaint the full Viewport at the interactive fast cadence solely for caret blink

#### Scenario: Typing with caret already visible

- **WHEN** the user types while the block caret is already shown
- **THEN** resetting the blink period does not by itself mark the Viewport dirty

### Requirement: Separate Terminal tab caret overlay

When the Terminal tab can accept keyboard input (focused Terminal tab, running session, VT caret visible), the host SHALL show a block caret overlay. Caret blink SHALL toggle only that overlay and MUST NOT require a full Viewport Image repaint.

#### Scenario: Focused running Terminal tab shows caret

- **WHEN** the viewport is focused on a running Terminal tab with a visible VT caret
- **THEN** a block caret overlay is shown at the caret cell

#### Scenario: Blink without Image repaint

- **WHEN** the Terminal tab caret overlay is blinking while content is idle
- **THEN** blink changes overlay visibility without re-rasterizing the log Viewport Image

#### Scenario: Hidden when input is not accepted

- **WHEN** the viewport is unfocused, a filter tab is active, or no shell is running
- **THEN** the caret overlay is hidden
