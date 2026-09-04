## Why

Users need to launch a Linux command inside WSL from a Project Program (one saved command, not wrapping `wsl.exe` by hand). The spawn resolver and `LaunchConfig.wsl` already exist, but Edit Launch never exposed them, Save always cleared `wsl`, and a previous Windows 10 run failed without a way to debug ConPTY + `wsl.exe`.

## What Changes

- Edit Launch on Windows: WSL checkbox, optional distro, Linux cwd hint.
- `ProgramSetLaunch` persists `wsl` / `wsl_distro` instead of wiping them.
- Stats expose saved launch cwd/WSL (not live OSC 7 cwd) so the dialog round-trips.
- Prove a real WSL spawn in the Windows `release-dev` GUI (`uname` / login bash). Fallback if this host’s `wsl.exe` rejects `--shell-type login` / `--cd`.

## Capabilities

### New Capabilities

- (none)

### Modified Capabilities

- `terminals/projects`: Program launch may be WSL mode (`wsl: true`, optional `wsl_distro`); persist and restore through `projects.yaml`.
- `ui/projects-sidebar`: Edit Launch includes WSL controls on Windows.

## Impact

- `noviewlog-core`: `ProgramSetLaunch`, stats, spawn fallback if needed; tests; `docs/terminals.md`
- `noviewlog-slint`: Edit Launch WSL chrome + bridge
- Verify: `cargo test -p noviewlog-core --lib`; Windows daily run `.\scripts\run-slint-windows.ps1` with a live WSL command (not compile-only)
