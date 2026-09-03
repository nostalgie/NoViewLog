# NoViewLog — agent guide

Native **desktop** log viewer: Rust engine + Slint UI. No WebView. The log
viewport is a Rust `fontdue` bitmap.

`npm` / `node` in the README are **commands the app wraps** (via PTY), not this
repository’s tech stack.

## Docs

| Doc | Audience |
|-----|----------|
| [`README.md`](README.md) | Users: run, Windows build, filters |
| [`docs/architecture.md`](docs/architecture.md) | Engine ↔ UI boundary |
| [`docs/terminals.md`](docs/terminals.md) | Terminals / tabs / **Project open + auto-start** |
| [`.cursor/rules/`](.cursor/rules/) | Enforced agent details (UI chrome, build, project open) |
| [`openspec/specs/`](openspec/specs/) | Living behavior specs (grow via archived changes) |
| [`openspec/changes/`](openspec/changes/) | Active change proposals and artifacts |

## OpenSpec workflow

Use OpenSpec for non-trivial features and behavior changes — not one-line fixes.

| Step | Cursor command | Purpose |
|------|----------------|---------|
| Explore (optional) | `/opsx-explore` | Read the area before proposing |
| Propose | `/opsx-propose <slug>` | Create proposal, delta specs, design, tasks |
| Implement | `/opsx-apply` | Build against agreed `tasks.md` |
| Archive | `/opsx-archive` | Merge deltas into `openspec/specs/` |

**Relationship to other docs:** [`.cursor/rules/`](.cursor/rules/) are permanent
constraints injected on every session. `openspec/` holds per-change agreement
(proposal, design, tasks, spec deltas). Do not backfill specs for the whole
codebase — let them grow one archived change at a time.

Project context and artifact rules live in [`openspec/config.yaml`](openspec/config.yaml).
After upgrading the global CLI: `openspec update` in the repo root.

## Layout

| Task | Location |
|------|----------|
| Slint UI | `crates/noviewlog-slint/` |
| Engine | `crates/noviewlog-core/` |
| Engine façade / commands | `crates/noviewlog-core/src/engine/` |
| Sessions / tabs | `crates/noviewlog-core/src/terminal_state.rs`, `log_view.rs` |
| Parser / filters / buffer | `crates/noviewlog-core/src/core/` |
| FILES (load / index / match) | `crates/noviewlog-core/src/file_load.rs`, `file_index.rs`, `file_match.rs` |
| PTY | `crates/noviewlog-core/src/pty.rs` |
| Viewport | `crates/noviewlog-core/src/viewport.rs` |
| Bundled presets | `presets/defaults.yaml` |
| User config (runtime) | `~/.config/noviewlog/config.yaml` (edit `presets:` to override/add) |

## Git workflow

- Default branch: `main` (protected).
- Ship changes via **feature branch → pull request → merge**.
- **Never** push (or force-push) directly to `main`.
- **Never** `git commit`, `git push`, or `gh pr create` unless the user
  explicitly asked in this turn. Plan todos do not count.
- **Never** prompt the user to commit or push.
- Do not create or switch branches unless the task needs it.
- After a PR merges (including when the user merged it on GitHub), switch back
  to `main` and pull **without being asked** — at session start and before new
  work. See [`.cursor/rules/git-workflow.mdc`](.cursor/rules/git-workflow.mdc).
- Commit messages, PR titles/bodies, and tracked docs/comments: **English only**
  (no Cyrillic in the tree).

## Build and verify

After changes to `crates/noviewlog-slint/`, shared engine code used by Slint, or
Slint packaging scripts, **build or run yourself** — do not ask the user to
rebuild:

| Platform | Daily run | Build only |
|----------|-----------|------------|
| Linux | `bash scripts/run-slint.sh` | `cargo build --profile release-dev -p noviewlog-slint` |
| Windows | `.\scripts\run-slint-windows.ps1` | same (after MSVC env is active) |

Publish / fat LTO: `cargo build --release -p noviewlog-slint` or
`bash scripts/publish-slint-windows.sh` (native Windows MSVC host only).

CI: `.github/workflows/windows-slint.yml` runs `cargo test -p noviewlog-core --lib`,
`cargo test -p noviewlog-slint --lib --test inline_rename_wiring --test chrome_icon_wiring`, and
`cargo build --profile release-dev -p noviewlog-slint` on `windows-latest`.

When touching engine / parser / filters:

```bash
cargo test -p noviewlog-core --lib
```

PTY ingest, Follow, WRAP, Viewport paint, or HOST_TICK: **also** run the
daily GUI (`release-dev`) and measure flood CPU (no Follow jumps). On Linux,
confirm `/proc/<pid>/exe` is `target/release-dev/...`. On Windows, confirm
`Get-Process noviewlog-slint | Select-Object Path` contains
`target\release-dev\` and sample CPU while `type big.log` runs in the terminal.
Debug pegs ~100% on the same flood. Tests + compile are not enough. See
[`.cursor/rules/terminal-flood-verify.mdc`](.cursor/rules/terminal-flood-verify.mdc).

## Where to edit

- Desktop chrome and Slint UI → only `crates/noviewlog-slint/`
- Engine, PTY, viewport, parsing → `crates/noviewlog-core/`

## Out of scope

Do **not** exploratory-search for `package.json`, JS/TS/CSS/HTML, React,
webpack, or vite unless the user explicitly asks. This is not a web app.

## UI chrome (summary)

- Never use `Theme.accent` (or bright fluent blue) as chrome borders, focus
  rings, left strips, or tab/TERMINALS reorder drop markers.
- Selection may use a soft background tint (`Theme.accent-soft`) without an
  accent border.
- Inline rename: square `TextInput` in a `Rectangle` with `Theme.border` (or no
  border) — not fluent `LineEdit`. Click-away (including empty sidebar space
  under FILES) MUST end rename; mouse leave MUST NOT. Spec:
  [`openspec/specs/ui/inline-rename/spec.md`](openspec/specs/ui/inline-rename/spec.md).
- Chrome icons: **Path** geometry or colored discs (`SectionDot`) — never
  Unicode symbols in `Text`. Guard:
  `cargo test -p noviewlog-slint --test chrome_icon_wiring`.
- Dialog buttons: **`AppButton`** (Theme + `TouchArea`) — never fluent
  `Button` from `std-widgets`. Chrome labels use bundled **Noto Sans**
  (proportional); never mono on Window `default-font-family`.
- **Project open:** Terminal tab + auto-start — see
  [`docs/terminals.md`](docs/terminals.md) and
  [`.cursor/rules/project-open-autostart.mdc`](.cursor/rules/project-open-autostart.mdc).
- FILES rows: no inline rename (`can-rename: false`).

Details: [`.cursor/rules/no-blue-chrome-borders.mdc`](.cursor/rules/no-blue-chrome-borders.mdc),
[`.cursor/rules/no-system-chrome-fonts.mdc`](.cursor/rules/no-system-chrome-fonts.mdc),
[`.cursor/rules/inline-rename-dismiss.mdc`](.cursor/rules/inline-rename-dismiss.mdc).

## Safety

- Do not commit secrets, `target/`, `.deps/`, `dist/`, or local `sample/`.
- Do not add GitHub Actions workflows unless explicitly requested.
