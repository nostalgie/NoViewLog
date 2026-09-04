## Context

See proposal.md. Launch fields already flow on stats (`launch_command`,
`launch_args`, `launch_cwd`, `launch_wsl`, `launch_wsl_distro`) into
`TerminalInfo`. The empty viewport still uses engine `render_center_message`.
Find is a floating overlay on the bitmap; status is a 22px bar below it.

## Goals / Non-Goals

**Goals:**

- One-line chrome strip from already-synced launch fields.
- Visible for any stopped live session (empty buffer and leftover output).

**Non-Goals:**

- Start/Stop on the strip; click-to-open Edit launch.
- Changing PTY ingest, Follow, or center-hint copy.
- Preview on FILES.

## Decisions

1. **Layout strip, not overlay or bitmap** — Place the bar in the viewport
   `VerticalLayout` above `image-cell` (same column as status). Overlay would
   cover leftover output and collide with Find. Painting into the fontdue
   bitmap would miss the leftover-output case and use the mono face.
   Alternative considered: status-bar text — too easy to miss, already used
   for engine status.

2. **Format in Rust, bind two properties** — `show-launch-preview` and
   `launch-preview-text` from `stats_sync`. Slint is a poor place to join
   optional WSL/cwd segments. Engine stays unchanged; stats already carry
   the fields.

3. **Copy rules** — Saved command: `Start: <cmd> <args>` plus `WSL[ distro]`
   and cwd, joined with `  |  `. Empty command + WSL: `Start: interactive WSL
   shell`. Empty command, local: `Type to open a shell` (no `Start:` — those
   rows have no play control). Omit empty cwd and WSL-off.

4. **Chrome** — `Theme.bg-bar`, Noto Sans 11–12px, `Theme.text-secondary`,
   elide, ~22px, pointer-down dismisses rename. No accent border, no icons.

## Risks / Trade-offs

- [Viewport height jumps on Start/Stop] → Mitigation: existing
  `viewport-resized`; acceptable for a 22px bar.
- [Long command+cwd] → Mitigation: `overflow: elide`.
- [Shell-only vs has-launch] → Mitigation: distinct copy so the strip does not
  imply a missing Start button.

## Migration Plan

None. Chrome-only; no store or engine protocol change.

## Open Questions

None.
