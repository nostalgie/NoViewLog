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

## Layout

| Task | Location |
|------|----------|
| Slint UI | `crates/noviewlog-slint/` |
| Engine | `crates/noviewlog-core/` |
| Parser / filters / buffer | `crates/noviewlog-core/src/core/` |
| PTY | `crates/noviewlog-core/src/pty.rs` |
| Viewport | `crates/noviewlog-core/src/viewport.rs` |
| Bundled presets | `presets/*.yaml` |
| User config (runtime) | `~/.config/noviewlog/config.yaml` |

## Git workflow

- Default branch: `main` (protected).
- Ship changes via **feature branch → pull request → merge**.
- **Never** push (or force-push) directly to `main`.
- Do not create or switch branches unless the task needs it.
- After a PR merges, delete the feature branch locally and on the remote.
- Commit messages, PR titles/bodies, and tracked docs/comments: **English only**
  (no Cyrillic in the tree).

## Build and verify

After changes to `crates/noviewlog-slint/`, shared engine code used by Slint, or
Slint packaging scripts, **build or run yourself** — do not ask the user to
rebuild:

```bash
bash scripts/run-slint.sh
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
