## Context

`LaunchConfig` already has `wsl` / `wsl_distro`. `resolve_process_launch` / `build_wsl_argv` build `wsl.exe --shell-type login [-d Distro] [--cd /linux] -- cmd args` with a local Windows CreateProcess cwd. A prior Windows 10 run failed (UNC cwd, Windows interop `pnpm`/`node`, ConPTY `STATUS_DLL_INIT_FAILED`, older `wsl.exe` rejecting `--shell-type` / `--cd`). Edit Launch never set the flags; `program_set_launch` always wrote `wsl = false`.

## Goals / Non-Goals

**Goals:** Persist WSL on Programs; Edit Launch on Windows; real ConPTY spawn on this host.

**Non-Goals:** Distro picker from `wsl -l`; converting `C:\…` to `/mnt/c/…`; SSH; WSL UI on Linux.

## Decisions

1. **Reuse existing argv builder** — do not reimplement `wsl.exe` wrapping. Add a fallback only if this host’s `wsl.exe` rejects `--shell-type login` or `--cd`.
2. **Saved launch vs live cwd** — stats add `launch_cwd` / `launch_wsl` / `launch_wsl_distro`. Edit Launch drafts those, not OSC 7 `term.cwd`.
3. **Per-Program, not per-Project** — matches `LaunchConfig`.
4. **Theme checkbox** — Path check like FILTERS rows; no fluent `CheckBox`, no Unicode `✓`.
5. **Acceptance is a GUI spawn** — `cargo test` of argv is not enough.

## Risks / Trade-offs

- [Older `wsl.exe` flags] → Mitigation: fallback argv that still runs the Linux command.
- [ConPTY + `wsl.exe`] → Mitigation: keep System32 `wsl.exe`, local Windows cwd, login shell when supported.
- [Edit Launch wiping WSL] → Mitigation: stop clearing flags in `program_set_launch`.

## Migration Plan

Existing `projects.yaml` without `wsl` stays process mode (`serde` default false). No conversion.

## Open Questions

None — distro remains an optional text field (empty = default distro).
