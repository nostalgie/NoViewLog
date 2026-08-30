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
| [`docs/terminals.md`](docs/terminals.md) | Terminals / tabs model |
| [`.cursor/rules/`](.cursor/rules/) | Enforced agent details (UI chrome, build) |
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
| Parser / filters / buffer | `crates/noviewlog-core/src/core/` |
| PTY | `crates/noviewlog-core/src/pty.rs` |
| Viewport | `crates/noviewlog-core/src/viewport.rs` |
| Bundled presets | `presets/defaults.yaml` |
| User config (runtime) | `~/.config/noviewlog/config.yaml` (edit `presets:` to override/add) |

## Git workflow

- Default branch: `main` (protected).
- Ship changes via **feature branch → pull request → merge**.
- **Never** push (or force-push) directly to `main`.
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

```bash
bash scripts/run-slint.sh   # cargo run --release (not debug)
# or
cargo build --release -p noviewlog-slint
```

Windows release staging (native MSVC host only):

```bash
bash scripts/publish-slint-windows.sh
```

When touching engine / parser / filters:

```bash
cargo test -p noviewlog-core --lib
```

PTY ingest, Follow, WRAP, Viewport paint, or HOST_TICK: **also** run the
release GUI and measure `cat` (CPU, no Follow jumps). Confirm
`/proc/<pid>/exe` is `target/release/...` — debug pegs ~100% on the same
flood. Tests + compile are not enough. See
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
  border) — not fluent `LineEdit`.

Details: [`.cursor/rules/no-blue-chrome-borders.mdc`](.cursor/rules/no-blue-chrome-borders.mdc).

## Safety

- Do not commit secrets, `target/`, `.deps/`, `dist/`, or local `sample/`.
- Do not add GitHub Actions workflows unless explicitly requested.
