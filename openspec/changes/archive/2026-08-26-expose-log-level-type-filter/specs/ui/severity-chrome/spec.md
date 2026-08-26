## Purpose

Defines Slint controls and Viewport cues that surface Record LogLevel so severity is readable and one-click filterable without accent chrome borders.

## ADDED Requirements

### Requirement: Severity filter control in chrome

The Slint UI SHALL provide a control to select the active Tab/View severity mode (All, Errors, Warnings, Info, Debug, Unleveled) and SHALL send the corresponding engine Command when the selection changes.

#### Scenario: User picks Errors

- **WHEN** the user selects Errors in the severity control
- **THEN** the engine severity mode for the active Tab/View becomes Errors and the Viewport updates to the filtered set

#### Scenario: Control reflects stats

- **WHEN** the host receives stats with a severity mode
- **THEN** the severity control displays that mode

### Requirement: Viewport severity cue without accent borders

For each visible Record that has a detected LogLevel, the Viewport SHALL render a muted severity cue on the Record's first physical line (glyph and/or soft text color). The cue MUST NOT use Theme.accent as a border, focus ring, or left accent strip.

#### Scenario: Error Record shows error cue

- **WHEN** a visible Record has level Error
- **THEN** its first painted line includes a muted error severity cue

#### Scenario: Unleveled Record has no severity cue

- **WHEN** a visible Record has no LogLevel
- **THEN** no severity glyph or severity tint is applied for that Record

#### Scenario: No accent border chrome

- **WHEN** severity cues are painted
- **THEN** no accent-colored border or vertical accent strip is introduced for severity
