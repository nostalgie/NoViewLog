# launch-preview Specification

## Purpose

Stopped live Terminals show a compact launch-summary strip above the viewport so
the user can see what Start will run without opening Edit launch.

## Requirements

### Requirement: Stopped live Terminal shows launch preview

When the active session is a live TERMINALS Terminal and it is not running, the
UI SHALL show a one-line ASCII summary of the saved launch above the viewport
(command and args, WSL when enabled, saved cwd when set). The strip SHALL hide
while that session is running and SHALL NOT appear for FILES sessions. Empty
command SHALL use type-to-open wording rather than implying a Start control that
shell-only rows do not have. The strip SHALL NOT replace the centered
empty-buffer hints, SHALL NOT add Start/Stop controls, and SHALL NOT use Unicode
icon glyphs or accent borders.

#### Scenario: Saved command while stopped

- **GIVEN** a live Terminal with command `uname`, args `-a`, WSL on, and a Linux cwd
- **WHEN** that Terminal is active and not running
- **THEN** a strip above the viewport includes the command, args, WSL, and cwd

#### Scenario: Hide while running

- **GIVEN** a live Terminal that shows the launch preview while stopped
- **WHEN** the user Starts that Terminal
- **THEN** the launch preview strip is hidden

#### Scenario: FILES has no preview

- **GIVEN** an active FILES session
- **WHEN** the viewport is shown
- **THEN** the launch preview strip is not shown

#### Scenario: Shell-only copy

- **GIVEN** a live Terminal with no saved command and WSL off
- **WHEN** that Terminal is active and not running
- **THEN** the strip uses type-to-open wording and does not use a `Start:` prefix
