## Context

See proposal.md for motivation. Today file sessions already exist (`is_file_session`, `FileBackedLog`, sliding window) but share the TERMINALS sidebar, use a full in-RAM `LineIndex` (`Vec<u64>` per line), and apply filters only to the ≤30k-line window. Live PTY path stays on `RecordBuffer` + scrollback cap.

Key modules: `terminal_state`, `file_index`, `file_load`, `engine/file_session`, `engine/stats`, `log_view`, `core/filter`, `core/visible`; Slint sidebar in `crates/noviewlog-slint/ui/app.slint` and models in `main.rs`. Docs: `docs/terminals.md`, `docs/architecture.md`.

## Goals / Non-Goals

**Goals:**
- Dual collapsible sidebar (TERMINALS / FILES) with separate stats models
- File UX: tab0 = basename, no Follow
- Multi-GB open via sparse/on-disk line index
- Whole-file filter/search via match-offset index over the original file

**Non-Goals:**
- Derived temp file of matching lines (option A) in v1
- Seek-next-only UX without continuous filtered scroll (option C)
- Changing live PTY scrollback semantics or raising the 30k cap
- Cross-drag reorder between TERMINALS and FILES
- Tail -f / Follow for growing files in v1

## Decisions

### 1. One `Vec<TerminalState>`, two stats lists

Keep a unified session vector in the engine; discriminate with `is_file_session()`. Stats expose `terminals` (non-file) and `files` (file) for Slint models. Active session remains an index/id into the unified list.

**Alternatives:** Separate `files: Vec<FileState>` — clearer types, larger refactor; deferred.

### 2. Match index (B) for file filters

Per filter Tab: background scan → `match_offsets: Vec<u64>` (byte starts of matching lines/Records). Scroll = match ordinal; paint seeks via sparse index + read window from the source file. Rescan on rule change; cancel prior scan.

**Alternatives considered:** Full derived file on disk (A) — simple reuse of windowing but duplicates data; seek-next only (C) — constant memory but different UX. User chose B.

### 3. Sparse / on-disk line index for the source file

Replace dense `LineIndex` with checkpoints (every N lines or M bytes) plus local walk, and/or a cache file under the user cache dir. Exact checkpoint density can be tuned after a spike; requirement is no O(lines) full-density RAM vector.

**Alternatives:** mmap entire file — helpful for reads but does not solve offset index size alone; keep dense index — fails multi-GB goal.

### 4. Phased delivery

1. UX split + tab name + no Follow (may still use current windowing)
2. Sparse/on-disk index
3. Match-index filters/search

Ship intermediate UX without waiting for full multi-GB internals.

### 5. Collapse persistence

Persist `terminals_section_expanded` / `files_section_expanded` in `AppConfig` when wiring is cheap; otherwise UI-only for the first slice and persist in the same change if config touch is already needed.

## Risks / Trade-offs

- **[Risk] Dense filters → huge `match_offsets`** → Mitigation: offsets only (8 bytes × hits); later optional on-disk match index if needed; show progress during scan.
- **[Risk] Sparse index makes random line seek slower** → Mitigation: tune checkpoint interval; prefetch windows around scroll.
- **[Risk] Prefetch/scroll heuristics tuned for unfiltered height break on sparse matches** → Mitigation: scroll height derived from match count × estimated line height for filtered file tabs.
- **[Risk] Record/multiline boundaries in match scan** → Mitigation: reuse existing record parser over streaming chunks where possible; document line-oriented fallback if record spanning across chunk edges needs a follow-up.

## Migration Plan

No user data migration. Existing open-file CLI/`LoadFile` keeps working; sessions appear under FILES. Users with only live terminals see an empty FILES section.

## Open Questions

None that block specs or tasks; checkpoint density and whether match scan is line- or Record-oriented can be decided during the sparse-index / match-index implementation spikes.
