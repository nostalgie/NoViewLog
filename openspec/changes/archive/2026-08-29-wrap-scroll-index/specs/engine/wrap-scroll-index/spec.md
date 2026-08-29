## Purpose

Defines that Wrap ON scroll and visual-row totals MUST use a maintained visual-row index so mid-history Viewport paints stay proportional to the visible slice, not to full scrollback length.

## ADDED Requirements

### Requirement: Visual-row totals are indexed

When Wrap is enabled, the engine SHALL maintain a visual-row prefix index for the Terminal/filter view’s flat lines for the current viewport cell geometry. Total visual row count used for scroll extent MUST be obtainable without scanning all flat lines on every scroll event.

#### Scenario: Max scroll does not rescan scrollback on each nudge

- **WHEN** Wrap is on, the view has a large flat-line scrollback, and the user changes scroll position without changing wrap or viewport width
- **THEN** computing max scroll / total visual rows does not require a full linear pass over all flat lines for that nudge alone

### Requirement: Mid-scroll visible collect is not O(scrollback)

When Wrap is on and the Viewport paints a window whose first visual row is not near the content end, locating the starting flat line MUST NOT walk every preceding flat line from index 0. Visible wrap materialization MUST be bounded by the painted window size (plus a small constant), not by scrollback length.

#### Scenario: Scroll mid-history with Wrap on

- **WHEN** Wrap is on, scrollback contains many thousands of flat lines, and the user scrolls to a mid-buffer position
- **THEN** collecting the visible visual lines for paint does not iterate all flat lines from the start of scrollback

#### Scenario: Open large file via FILES and scroll mid-history

- **WHEN** the user opens a large log file via FILES (not PTY `cat`), Wrap is on, and they scroll mid-history
- **THEN** the Viewport stays smooth and stable without jump/jitter caused by O(scrollback) wrap layout walks

#### Scenario: Near-bottom Follow still works

- **WHEN** Wrap is on and Follow keeps the Viewport at the content bottom
- **THEN** the Viewport continues to show the live tail correctly

### Requirement: Index stays coherent under ring trim and append

When flat lines append, drop a prefix (ring trim), or replace the volatile tail, the visual-row index MUST be updated or rebuilt so totals and mid-scroll jumps remain consistent with the flat-line list.

#### Scenario: Trim then scroll

- **WHEN** live PTY ring trim drops oldest flat lines and the user then scrolls mid-history with Wrap on
- **THEN** visible lines and scroll extent match the post-trim flat lines without requiring a from-zero walk of the remaining scrollback on each paint
