## ADDED Requirements

### Requirement: WSL launch mode on Programs

A live Program SHALL be able to record WSL launch mode (`launch.wsl`) and an optional distribution (`launch.wsl_distro`). When `wsl` is true on Windows, Start SHALL spawn via `wsl.exe` so Command/Args run inside the distro. `cwd` SHALL be passed as a Linux `--cd` path (or converted from `\\wsl$\Distro\…`). Empty Command SHALL start an interactive login bash in that distro. On non-Windows hosts, starting a WSL Program SHALL fail with a clear error. Saving Edit Launch SHALL NOT clear `wsl` / `wsl_distro`.

#### Scenario: Saved WSL command starts inside the distro

- **GIVEN** a live Program with `wsl: true`, command `uname`, args `-a`, and a Linux cwd
- **WHEN** the user Starts that Terminal on Windows
- **THEN** the PTY child is `wsl.exe` with the Linux command after `--`
- **AND** viewport output is the Linux `uname` result (not WSL help text, not a Windows binary)

#### Scenario: Empty WSL command is a distro shell

- **GIVEN** a live Program with `wsl: true` and no command
- **WHEN** the user Starts that Terminal on Windows
- **THEN** an interactive bash inside the distro is started

#### Scenario: Edit Launch keeps WSL flags

- **GIVEN** a Terminal with WSL launch enabled and a distro name
- **WHEN** the user saves Edit Launch (command, args, cwd, WSL on, distro)
- **THEN** the Program in `projects.yaml` still has `wsl: true` and that distro
