## Purpose

Defines how each Tab/View filters Records by detected LogLevel so users can show only errors, warnings, or other severities without writing include patterns.

## Requirements

### Requirement: Per-view severity filter mode

Each Tab/View (including the Terminal tab) SHALL have a severity filter mode among: All, Errors, Warnings, Info, Debug, Unleveled. Default SHALL be All.

#### Scenario: Default shows all levels

- **WHEN** a Tab/View is created or the severity mode is All
- **THEN** Records are not hidden solely because of LogLevel

#### Scenario: Errors mode keeps only error Records

- **WHEN** the active Tab/View severity mode is Errors
- **THEN** only Records whose detected level is Error are visible in that view

#### Scenario: Unleveled mode keeps only Records without a level

- **WHEN** the active Tab/View severity mode is Unleveled
- **THEN** only Records with no detected LogLevel are visible in that view

### Requirement: Severity filter applies after include/exclude

When building the visible line list for a Tab/View, the engine SHALL apply include/exclude filter rules first, then apply the severity mode to surviving Records.

#### Scenario: Exclude still wins before severity

- **WHEN** a Record matches an active exclude rule and severity mode is All or Errors
- **THEN** that Record remains hidden

#### Scenario: Severity narrows include results

- **WHEN** include rules leave both Error and Info Records and severity mode is Errors
- **THEN** only Error Records remain visible

### Requirement: Severity filter via engine Command and stats

The engine SHALL accept a typed Command to set the severity mode for the active Terminal's active Tab/View, and SHALL expose the current mode in stats/events consumed by the UI.

#### Scenario: Set severity on active view

- **WHEN** the host sends a severity-set Command for Errors
- **THEN** the active Tab/View mode becomes Errors and subsequent visible lines reflect that mode

#### Scenario: Stats report active severity

- **WHEN** the host polls stats after changing severity
- **THEN** stats include the active Tab/View severity mode
