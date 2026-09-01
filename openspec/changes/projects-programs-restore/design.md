## Context

See proposal.md — Why. Existing unused types: `ProjectsStore`, `ProjectConfig`, `ProgramConfig`, `WorkspaceConfig` (tab snapshot only), `LaunchConfig`. IO: `load_projects_store` / `save_projects_store`. Process path: `terminal_start` / `stop` / `start_launch_process`. `AppConfig.workspaces` stays unused.

## Goals / Non-Goals

**Goals:** Wire store into Engine + Slint; open → stopped restore; Run/Stop; no shell respawn when `launch.command` set; PROJECTS chrome.

**Non-Goals:** WSL UI/fixes; auto-start on open; FILES in projects; reviving `AppConfig.workspaces`.

## Decisions

1. **Reuse Project/Program names** — matches scaffolding and locked product naming; `WorkspaceConfig` remains internal tab snapshot.
2. **Open replaces TERMINALS only** — stop live PTYs, rebuild from Programs; FILES untouched.
3. **Link `program_id` on `TerminalState`** — stable save-back for tabs/launch/order.
4. **Save-on-command** — persist `projects.yaml` after Project/Program mutations and after tab/launch/rename/reorder while a project is active (same pattern as presets).
5. **Per-id Stop** — extend Stop to optional `terminal_id` so inactive rows work.
6. **Exit policy** — if `launch.command.is_some()`, do not call `start_interactive_shell_for` on child exit; else keep current respawn.

## Risks / Trade-offs

- [Empty project open] → Mitigation: synthesize one blank stopped Terminal.
- [Respawn regression for plain shells] → Mitigation: gate only on `command.is_some()`.
- [Prior WSL issues] → Phase 2 only; do not exercise WSL in v1 verify.

## Migration Plan

No migration of old data. Missing `projects.yaml` → empty store. Docs update `docs/terminals.md`.

## Open Questions

None for v1.
