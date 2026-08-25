#!/usr/bin/env bash
# Native Windows MSVC publish for noviewlog-slint → dist/noviewlog-slint-win-x64/
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT/dist/noviewlog-slint-win-x64"

is_windows_host() {
    case "$(uname -s 2>/dev/null || echo unknown)" in
        MINGW*|MSYS*|CYGWIN*) return 0 ;;
    esac
    # Git Bash / some environments report Windows_NT via OSTYPE or env
    if [[ "${OS:-}" == "Windows_NT" ]]; then
        return 0
    fi
    return 1
}

if ! is_windows_host; then
    echo "error: publish-slint-windows.sh requires a native Windows host (MSVC Rust toolchain)." >&2
    echo "  Linux→Windows GUI cross-compile is not supported in v1." >&2
    echo "  On Windows: install rustup MSVC target, then re-run this script." >&2
    echo "  Or use a windows-latest CI job with the same commands." >&2
    exit 1
fi

cd "$ROOT"

echo "==> Building noviewlog-slint (release)..."
cargo build --release -p noviewlog-slint

SRC_EXE="$ROOT/target/release/noviewlog-slint.exe"
if [[ ! -f "$SRC_EXE" ]]; then
    echo "error: expected $SRC_EXE not found after cargo build" >&2
    exit 1
fi

echo "==> Staging $OUT_DIR ..."
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
cp -f "$SRC_EXE" "$OUT_DIR/NoViewLog.exe"

cat > "$OUT_DIR/README.txt" <<'EOF'
NoViewLog (Slint) — Windows x64

Requirements: Windows 10 or later, x64.

Run:
  NoViewLog.exe
  NoViewLog.exe -- app.log
  NoViewLog.exe -- npm run dev

Copy this entire folder if you move the app; the engine is linked into NoViewLog.exe
(no separate noviewlog_core.dll).
EOF

SIZE="$(du -sh "$OUT_DIR" | cut -f1)"
echo ""
echo "Slint Windows distribution ready."
echo "  Output: $OUT_DIR"
echo "  Size:   $SIZE"
echo ""
echo "Run:"
echo "  NoViewLog.exe"
