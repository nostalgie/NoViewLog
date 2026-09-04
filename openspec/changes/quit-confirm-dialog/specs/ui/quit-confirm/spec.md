## Purpose

Always-on quit confirmation so closing the window does not kill live Terminals
or drop scrollback without an explicit Confirm.

## ADDED Requirements

### Requirement: Quit always prompts before the window hides

Every attempt to close the application Window SHALL show an in-app confirm
overlay and SHALL keep the Window visible until the user confirms. The prompt
SHALL appear even when only one stopped live Terminal exists and no PTY is
running. Caption close, File → Exit, and the window-manager close request
(Alt+F4 / taskbar) SHALL use this same prompt. The overlay SHALL use Theme
form chrome (`AppButton`, square borders) with English copy, SHALL NOT use
Unicode icon glyphs, and SHALL NOT use accent-colored borders.

#### Scenario: Caption close keeps the Window

- **GIVEN** the application Window is visible
- **WHEN** the user activates the caption close control
- **THEN** a confirm overlay is shown
- **AND** the Window remains visible

#### Scenario: File Exit keeps the Window

- **GIVEN** the application Window is visible
- **WHEN** the user activates File → Exit
- **THEN** a confirm overlay is shown
- **AND** the Window remains visible

#### Scenario: Window-manager close keeps the Window

- **GIVEN** the application Window is visible
- **WHEN** the window manager requests close (Alt+F4 or taskbar close)
- **THEN** a confirm overlay is shown
- **AND** the Window remains visible

#### Scenario: Prompt when idle

- **GIVEN** a single stopped live Terminal and no running PTY
- **WHEN** the user requests quit
- **THEN** the confirm overlay is still shown

### Requirement: Cancel does not quit

Cancel, Escape, and click-outside on the quit overlay SHALL dismiss the overlay
and SHALL leave the Window open. Confirm SHALL hide the Window. After hide,
running PTY children stop with process exit; Project settings remain as already
persisted.

#### Scenario: Cancel stays in the app

- **GIVEN** the quit confirm overlay is visible
- **WHEN** the user chooses Cancel, presses Escape, or clicks outside the overlay
- **THEN** the overlay is dismissed
- **AND** the Window remains visible

#### Scenario: Confirm hides the Window

- **GIVEN** the quit confirm overlay is visible
- **WHEN** the user chooses Close
- **THEN** the Window is hidden
- **AND** the application event loop ends
