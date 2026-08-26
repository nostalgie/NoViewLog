## Purpose

Defines how multiline Records collapse to a preview line and expand on demand so dense logs stay scannable while full stack text remains available.

## Requirements

### Requirement: Multiline Records support collapse

A Record with two or more physical lines SHALL be collapsible. A Record with one physical line SHALL always display fully and SHALL NOT show a collapse disclosure.

#### Scenario: Single-line Record

- **WHEN** a visible Record has exactly one physical line
- **THEN** that line is shown with no disclosure cue

#### Scenario: Multiline Record can collapse

- **WHEN** a visible Record has two or more physical lines and is collapsed
- **THEN** the Viewport shows a single preview row for that Record, not all physical lines

### Requirement: Default collapse for multiline Records

Newly appearing multiline Records in a Tab/View SHALL start collapsed unless the user has expanded that Record id in that view or has used expand-all for the current generation of the view's expand policy.

#### Scenario: New stack trace arrives collapsed

- **WHEN** a new multiline Record becomes visible in the active Tab/View and the user has not expanded it
- **THEN** it is shown as a collapsed preview

### Requirement: Preview content

A collapsed Record preview SHALL include the first physical line text (ANSI coloring preserved via existing paint path) and a muted indicator of how many additional lines are hidden.

#### Scenario: Preview shows first line and count

- **WHEN** a three-line Record is collapsed
- **THEN** the preview shows the first line and indicates that two lines are hidden

### Requirement: Per-view expand state and Commands

Each Tab/View SHALL track which Record ids are expanded. The engine SHALL accept Commands to toggle one Record, expand-all multiline Records currently in the filtered set, and collapse-all. Stats SHALL expose enough state for the UI to reflect expand-all availability if needed.

#### Scenario: Toggle expands full Record

- **WHEN** the user toggles a collapsed multiline Record
- **THEN** all of that Record's physical lines become visible in order

#### Scenario: Collapse-all

- **WHEN** the host sends collapse-all for the active Tab/View
- **THEN** all multiline Records in that view render as collapsed previews

### Requirement: Search and filters use full Record text

Include/exclude and search matching SHALL continue to use the full Record text even when the Record is collapsed. When a search match lies only on a hidden line, the engine SHALL treat that Record as expanded for display while the match is current, or otherwise make the match navigable without leaving the match stranded on a hidden line.

#### Scenario: Exclude still applies to full text

- **WHEN** a collapsed Record's hidden lines match an exclude rule
- **THEN** the Record remains hidden from the view entirely

#### Scenario: Goto search match reveals line

- **WHEN** the user navigates to a search match on a non-preview line of a collapsed Record
- **THEN** that Record is shown expanded so the match line is visible
