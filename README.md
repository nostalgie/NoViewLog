# NoViewLog

NoViewLog is a native desktop log viewer: a Rust engine plus a **Slint** UI.
There is no WebView or separate frontend. The log viewport is rendered by Rust
(`fontdue` bitmap).

**Status:** active development. The primary supported platform is **Ubuntu
Linux**. Other operating systems (including Windows) are best-effort — they may
work, but they are not the main development focus yet.

[<img src="https://github.com/slint-ui/slint/raw/master/logo/MadeWithSlint-logo-dark.png" alt="Made with Slint" width="160">](https://slint.dev)

## Features

- Launch commands through a PTY or open existing log files
- Run multiple independent terminal sessions
- Group multiline records such as stack traces
- Add include and exclude filters, presets, and per-tab searches
- Follow live output and navigate large files with a virtualized viewport

## Run (Ubuntu / Linux)

Primary target: Ubuntu. Requires [Rust](https://rustup.rs/) and native build
tools:

```bash
sudo apt install build-essential pkg-config libssl-dev
```

```bash
bash scripts/run-slint.sh
bash scripts/run-slint.sh -- app.log
bash scripts/run-slint.sh -- npm run dev
bash scripts/run-slint.sh -- --preset node-dev -- node server.js
```

This builds and runs `noviewlog-slint`. First run may fetch local fontconfig
deps via `scripts/setup-slint-deps.sh` into `.deps/`.

Arguments after `--` are executed in a PTY. Each terminal has its own Console
and filter tabs; switching terminals does not stop background sessions.
See [`docs/terminals.md`](docs/terminals.md) and
[`docs/architecture.md`](docs/architecture.md).

The bundled preset is [`presets/node-dev.yaml`](presets/node-dev.yaml). User
configuration is stored in `~/.config/noviewlog/config.yaml`.

## Other platforms (best-effort)

Windows and other OSes are supported on a best-effort basis while the project
is in active development. Expect rough edges; Ubuntu remains the reference
environment.

### Windows (optional)

Prerequisites: Windows 10+ x64.

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   with the **Desktop development with C++** workload.
2. Install Rust via **rustup-init.exe** from
   [https://win.rustup.rs/x86_64](https://win.rustup.rs/x86_64). Keep the
   default host `x86_64-pc-windows-msvc`.

   Do **not** use the Linux/macOS `curl … | sh` rustup one-liner on Windows.

3. Verify the toolchain:

```bash
rustc -vV
# host: x86_64-pc-windows-msvc
```

4. Build (Git Bash/MSYS or a normal Windows shell):

```bash
bash scripts/publish-slint-windows.sh
# or:
cargo build --release -p noviewlog-slint
```

Output: `dist/noviewlog-slint-win-x64/NoViewLog.exe`. Copy that folder to the
target machine and run `NoViewLog.exe`.

Do not use the Linux `run-slint.sh` / fontconfig `.deps` helpers on Windows.

## Tests

```bash
cargo test -p noviewlog-core --lib
```

## Architecture

| Path | Role |
|------|------|
| [`crates/noviewlog-core/`](crates/noviewlog-core/) | Engine: PTY, filters, buffer, fontdue viewport |
| [`crates/noviewlog-slint/`](crates/noviewlog-slint/) | Slint desktop UI |
| [`docs/architecture.md`](docs/architecture.md) | Engine ↔ UI boundary (commands, stats, paint) |
| [`presets/`](presets/) | Bundled filter presets |

## Filter logic

1. Exclude rules hide matching records.
2. If any include rule is active, a record must match at least one.
3. Exclude rules take precedence.

## License

NoViewLog is licensed under the [MIT License](LICENSE).

The UI toolkit is [Slint](https://slint.dev), used under the
[Slint Royalty-free License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md).
Bundled Noto fonts are under the SIL Open Font License (see [`assets/OFL.txt`](assets/OFL.txt)).
