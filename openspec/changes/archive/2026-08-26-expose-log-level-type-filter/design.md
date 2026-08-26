## Context

See proposal.md — Why. Today `LogRecord.level` is set in `core/parser.rs` / `core/terminal.rs` via `detect_level` (`core/types.rs`) but never consulted in `log_view` / `core/visible` / `viewport` paint or Slint chrome. Include/exclude live in `FilterEngine` on each `LogView`. Specs: `engine/severity-filter`, `ui/severity-chrome`.

## Goals / Non-Goals

**Goals:**

- Orthogonal severity mode on every `LogView` (including Console).
- Flat-line rebuild and search operate on severity-filtered Records.
- Mild Viewport cue for leveled Records; Slint dropdown/segmented control wired through Command + stats.
- Preserve include/exclude and ANSI coloring paths.

**Non-Goals:**

- Command-vs-output block typing (Better Terminal Logs Shell Integration style).
- Changing `detect_level` keyword sets in v1 (reuse as-is).
- Persisting severity mode in presets / `config.yaml` in v1.
- Collapsible Records (separate change).

## Decisions

### 1. Severity mode enum on LogView, not a FilterRule

**Choice:** Add `SeverityFilter` (All | Error | Warn | Info | Debug | Unleveled) on `LogView`, applied after `FilterEngine` when rebuilding flat lines.

**Why:** Pattern filters are user-authored and preset-backed; severity is a transient reading mode. Mixing into `FilterRule` would confuse presets and Console (Console is not filter-editable).

**Alternatives:** Encode as hidden include regexes — rejected (fragile, fights user rules, Console blocked).

### 2. Pipeline order: exclude/include → severity → flat lines → search

**Choice:** Document and implement severity after include/exclude in `rebuild_flat_lines` / `LogView` rebuild path (`core/visible.rs`, `log_view.rs`).

**Why:** Matches proposal; exclude noise still wins; severity then narrows.

### 3. Command + stats surface

**Choice:** Typed `Command` e.g. `SeveritySet { mode }` targeting active terminal + active view; mirror in JSON for tests; add field on `StatsSnapshot` for active view mode.

**Modules:** `engine/` command dispatch, `log_view.rs`, stats builder; Slint `engine_bridge` + chrome in `ui/`.

### 4. Viewport cue via FlatLine / TextStyle, not Slint overlays

**Choice:** Carry optional level on `FlatLine` (or first-line flag) into `ViewportRenderer`; paint a one-cell muted prefix or soft fg tint using existing theme-ish colors in the bitmap path (`viewport.rs`). Do not introduce Theme.accent borders (see `.cursor/rules/no-blue-chrome-borders.mdc`).

**ANSI:** Severity cue is an overlay/prefix; do not strip or rewrite SGR from `ansi.rs` / VT re-emit. Dual ANSI paths stay owners of color content; cue is additive chrome on the first cell(s).

**Alternatives:** Slint labels beside the Image — rejected (scroll/sync with bitmap is harder).

### 5. Slint control placement

**Choice:** Compact selector near existing Find / filter chrome (not a fluent accent focus bar). Selection tint may use soft background only.

## Risks / Trade-offs

- [False positives from `detect_level`] → Mitigation: Unleveled + All modes; document heuristic limits; tune keywords later without API break.
- [Prefix cue shifts columns / selection] → Mitigation: reserve a fixed gutter cell for leveled lines or paint tint without inserting characters into selectable text; prefer tint-on-first-line if selection geometry is fragile.
- [Console users expect raw stream] → Mitigation: default All; severity is opt-in narrowing.

## Migration Plan

- Default mode All → no behavior change until user changes control.
- No config migration.
- Rollback: remove Command/UI and severity branch in rebuild; levels on records remain unused again.

## Open Questions

- None that block specs or tasks; gutter-vs-tint detail can be chosen during apply without changing requirements (cue MUST exist, accent borders MUST NOT).
