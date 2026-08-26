## Purpose

Defines Viewport disclosure cues and host interactions so users can expand and collapse multiline Records without accent borders or a WebView list widget.

## Requirements

### Requirement: Disclosure cue on collapsed and expanded multiline Records

The Viewport SHALL paint a muted disclosure cue on the preview row of a collapsible Record (collapsed and expanded states distinguishable). The cue MUST NOT use Theme.accent as a border, focus ring, or left accent strip.

#### Scenario: Collapsed cue visible

- **WHEN** a multiline Record is collapsed
- **THEN** its preview row shows a muted collapsed disclosure cue

#### Scenario: No accent strip

- **WHEN** disclosure cues are painted
- **THEN** no accent-colored border or vertical accent strip is used for collapse chrome

### Requirement: Pointer toggle on disclosure

Clicking (primary button) the disclosure cue or the collapsed preview row SHALL toggle that Record's expand state for the active Tab/View via the engine.

#### Scenario: Click preview expands

- **WHEN** the user primary-clicks a collapsed Record preview
- **THEN** the Record expands in the Viewport

### Requirement: Expand-all and collapse-all controls

The Slint UI SHALL expose expand-all and collapse-all actions for the active Tab/View that send the corresponding engine Commands.

#### Scenario: Expand-all from chrome

- **WHEN** the user activates Expand all
- **THEN** all multiline Records in the active Tab/View's filtered set are expanded
