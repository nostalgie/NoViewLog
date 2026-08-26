## Context

See proposal.md — Why. Records are already multiline units (`LogRecord.lines`); `rebuild_flat_lines` emits one `FlatLine` per physical line (`core/visible.rs`). There is no expand set today. Viewport is fontdue bitmap with existing pointer/selection hit paths. Specs: `engine/collapsible-records`, `ui/record-collapse-chrome`. Independent of `expose-log-level-type-filter`.

## Goals / Non-Goals

**Goals:**

- Collapse multiline Records to a preview `FlatLine` (+ metadata) per view.
- Toggle / expand-all / collapse-all via Command; pointer hit on cue/preview.
- Keep include/exclude and search on full Record text; auto-expand when navigating to a hidden match.
- Preserve ANSI on the preview's first line via existing `ansi.rs` segment paint (no strip-for-collapse).

**Non-Goals:**

- CloudWatch DOM list / WebView.
- Changing parser grouping formats.
- Command-vs-output Shell Integration blocks.
- Persisting expand sets across app restarts in v1.
- Timestamp column chrome beyond optional later reuse of `received_at` (v1 preview = first line + hidden count).

## Decisions

### 1. Collapse in flat-list rebuild, not in the painter alone

**Choice:** `LogView` holds `expanded_record_ids: HashSet<u64>` (or equivalent). `rebuild_flat_lines` emits either all lines (expanded / single-line) or one preview `FlatLine` tagged with `record_id`, `collapsed: true`, `hidden_line_count`.

**Why:** Scroll metrics, follow-tail, and search indices must agree with what is painted.

**Alternatives:** Paint-only clip — rejected (scrollbar and goto would lie).

### 2. Default collapsed for multiline

**Choice:** Default collapsed for `lines.len() >= 2`. Expand-all sets a view flag or fills the set from current filtered ids; new Records stay collapsed unless expand-all-sticky is true.

**Sticky expand-all:** Prefer a boolean `expand_all_multiline` on the view: when true, new multiline Records render expanded; Collapse-all clears the flag and the id set; Toggle on one Record when expand-all is on removes sticky and keeps others expanded via explicit ids snapshot — keep v1 simpler: **no sticky**. Expand-all only expands currently known filtered multiline ids; new arrivals start collapsed. Document this in tasks/tests.

### 3. Search match on hidden line → auto-expand that Record

**Choice:** On `SearchGoto` / match navigation, if the match's record is collapsed, insert id into `expanded_record_ids` and rebuild before scrolling.

**Why:** Spec requires navigable matches; auto-expand is the least surprising.

### 4. Pointer: map y to FlatLine → toggle Command

**Choice:** Extend existing viewport pointer handling in engine/Slint bridge: hit preview/disclosure → `RecordCollapseToggle { record_id }`. Prefer toggling on disclosure glyph hit; also allow click on collapsed preview row (per UI spec).

**ANSI:** Disclosure is additive gutter/prefix; do not mutate stored SGR. Dual paths (`terminal.rs` / `ansi.rs`) unchanged for content.

### 5. Slint chrome

**Choice:** Expand all / Collapse all near Find or View menu. No accent borders on buttons or cues.

## Risks / Trade-offs

- [Follow-tail jumpiness when expanding above viewport] → Mitigation: preserve anchor record/line when toggling if feasible; accept minor jump in v1 if costly.
- [Large expand sets for busy buffers] → Mitigation: store ids only for expanded (default collapsed); expand-all materializes current ids only.
- [Selection across collapsed boundary] → Mitigation: selection stays on visible flat lines; document limitation.

## Migration Plan

- Default collapsed multiline changes density vs today (behavior change). Accept as the feature; users use Expand all for old flat look.
- No config migration.
- Rollback: remove collapse branch in rebuild and UI actions.

## Open Questions

- None blocking; sticky expand-all deferred (Decision 2).
