## Purpose

Inline rename for TERMINALS rows, FILES rows, and filter tabs. Same behavior on Linux and Windows. Click-away is defined by hit target, not by a list of widgets.

## Requirements

### Requirement: Click-away ends rename

While inline rename is open, a pointer-down or click **outside** the active rename `TextInput` (and its field chrome) SHALL end the session: commit a non-empty name, otherwise cancel. Platform SHALL NOT change this.

Surfaces that SHALL dismiss include:

- Viewport, find bar, status bar
- Follow, WRAP, menu bar, tab strip gutter, another tab chip
- Another TERMINALS or FILES row; chrome of the row being renamed (subtitle, padding)
- Section headers (TERMINALS / FILES), `+` controls, FILTERS rows
- **Sidebar dead space**: leftover stretch under the lists, including an empty FILES or TERMINALS section (list height is 0 when there are no rows)

#### Scenario: Click empty space under FILES

- **GIVEN** a TERMINALS row is in inline rename
- **AND** FILES has no rows (or leftover space below the lists)
- **WHEN** the user presses the leftover sidebar area
- **THEN** rename ends (commit if the draft name is non-empty)

#### Scenario: Click viewport

- **GIVEN** inline rename is open
- **WHEN** the user presses the log viewport
- **THEN** rename ends

### Requirement: Mouse leave does not end rename

Mouse leave, hover loss (`has-hover` false), and pointer-move SHALL NOT end rename. The user MUST keep typing.

#### Scenario: Cursor leaves the field without clicking

- **GIVEN** inline rename is open
- **WHEN** the pointer leaves the field or hovers the viewport without a button press
- **THEN** the editor stays open

### Requirement: Keyboard

Enter SHALL commit. Escape SHALL cancel. Real focus loss (viewport or another widget took focus) SHALL dismiss the same as click-away.

#### Scenario: Escape cancels

- **GIVEN** inline rename is open
- **WHEN** the user presses Escape
- **THEN** the previous name is kept and the editor closes

#### Scenario: Subtitle does not jump

- **GIVEN** a TERMINALS or FILES row shows a title and a path subtitle
- **WHEN** inline rename starts
- **THEN** the subtitle stays on the same baseline (fixed title-line height; action icons keep their layout slot)

