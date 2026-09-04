## 1. Engine persist and stats

- [x] 1.1 Extend `Command::ProgramSetLaunch` with `wsl` / `wsl_distro`; stop wiping flags in `program_set_launch`
- [x] 1.2 Expose `launch_cwd`, `launch_wsl`, `launch_wsl_distro` on `StatsTerminal`
- [x] 1.3 Core tests: set WSL via `program_set_launch`, survive project open / YAML

## 2. Slint Edit Launch

- [x] 2.1 `host-is-windows`; `TerminalInfo` launch WSL fields; stats_sync
- [x] 2.2 Edit Launch checkbox + optional distro; draft saved launch cwd; wire `program-set-launch`

## 3. Docs and verify

- [x] 3.1 Note WSL per-Program mode in `docs/terminals.md`
- [x] 3.2 `cargo test -p noviewlog-core --lib`
- [x] 3.3 Windows `release-dev` GUI: WSL `uname -a`, empty-command bash, Project reopen, WSL-off PowerShell
