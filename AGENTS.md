# NoViewLog — agent guide

Native **desktop** log viewer: Rust engine + Slint UI. No WebView. The log
viewport is a Rust `fontdue` bitmap.

`npm` / `node` in the README are **commands the app wraps** (via PTY), not this
repository’s tech stack.

**Linux and Windows are equally supported.** macOS and other OSes are
best-effort. Detect the **current host** before any build, run, or verify.
Never run the other OS’s script.

## Host OS (mandatory)

| Host | Daily run | Binary |
|------|-----------|--------|
| Linux | `bash scripts/run-slint.sh` | `target/release-dev/noviewlog-slint` |
| Windows | `.\scripts\run-slint-windows.ps1` | `target\release-dev\noviewlog-slint.exe` |

Build only (both, after the toolchain is in `PATH`):

```
cargo build --profile release-dev -p noviewlog-slint
```

- Windows run/publish scripts import MSVC when `link.exe` is missing.
- **Never** on Windows: `run-slint.sh`, `setup-slint-deps.sh`, `.deps`.
- **Never** on Linux: `run-slint-windows.ps1` / `publish-slint-windows.ps1`.
- Debug `cargo run` is not the product path (PTY flood pegs a core).
- Profiles and `RUSTFLAGS`: [`.cursor/rules/slint-release-build.mdc`](.cursor/rules/slint-release-build.mdc).
- PTY flood / Follow: [`.cursor/rules/terminal-flood-verify.mdc`](.cursor/rules/terminal-flood-verify.mdc).

Publish (Windows, native MSVC): `.\scripts\publish-slint-windows.ps1`
(or `bash scripts/publish-slint-windows.sh` from Git Bash).

## Docs

| Doc | Audience |
|-----|----------|
| [`README.md`](README.md) | Users: run on Linux and Windows, filters |
| [`docs/architecture.md`](docs/architecture.md) | Engine ↔ UI boundary |
| [`docs/terminals.md`](docs/terminals.md) | Terminals / tabs / Project open + manual Start |
| [`.cursor/rules/`](.cursor/rules/) | Enforced agent details (do not copy into this file) |
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

`.cursor/rules/` are permanent constraints. `openspec/` holds per-change
agreement. Do not backfill specs for the whole codebase.

Project context: [`openspec/config.yaml`](openspec/config.yaml). After upgrading
the global CLI: `openspec update` in the repo root.

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
| User config | `~/.config/noviewlog/config.yaml` · Windows: `%USERPROFILE%\.config\noviewlog\config.yaml` |

## Git workflow

- Default branch: `main` (protected). Feature branch → PR → merge. Never push `main`.
- Never `git commit` / `git push` / `gh pr create` unless the user asked **this turn**.
- Never offer commit or push. Do not create/switch branches unless the task needs it.
- After a merged PR: resync to `main` without being asked. Details:
  [`.cursor/rules/git-workflow.mdc`](.cursor/rules/git-workflow.mdc).
- Commits, PRs, tracked docs/comments: **English only**.
- On Windows PowerShell: `git commit -m "title" -m "body"` — no bash heredoc.

## Where to edit

- Desktop chrome and Slint UI → only `crates/noviewlog-slint/`
- Engine, PTY, viewport, parsing → `crates/noviewlog-core/`

Do **not** exploratory-search for `package.json`, JS/TS/CSS/HTML, React,
webpack, or vite unless the user explicitly asks. This is not a web app.

## UI chrome

Details live in the always-on rules — do not restate them here:

- [`.cursor/rules/no-blue-chrome-borders.mdc`](.cursor/rules/no-blue-chrome-borders.mdc)
- [`.cursor/rules/no-system-chrome-fonts.mdc`](.cursor/rules/no-system-chrome-fonts.mdc)
- [`.cursor/rules/inline-rename-dismiss.mdc`](.cursor/rules/inline-rename-dismiss.mdc)
- [`.cursor/rules/project-open-autostart.mdc`](.cursor/rules/project-open-autostart.mdc)

## Safety

- Do not commit secrets, `target/`, `.deps/`, `dist/`, or local `sample/`.
- Do not add GitHub Actions workflows. They were removed on purpose; this
  project will move to a self-hosted server. Do not recreate `.github/workflows`
  or any third-party CI until the user explicitly requests it.
