#!/usr/bin/env bash
# Build/run the Slint UI (default product) without system libfontconfig-dev.
# Always --release: debug pegs a core on PTY flood (Follow+WRAP cat).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ ! -f "${ROOT}/.deps/pkgconfig/fontconfig.pc" ]]; then
  bash "${ROOT}/scripts/setup-slint-deps.sh"
fi
export PKG_CONFIG_PATH="${ROOT}/.deps/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LIBRARY_PATH="${ROOT}/.deps/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
# Add .deps -L once. Appending on every nest (parent already set RUSTFLAGS)
# changes the rustc fingerprint and forces a full rebuild every launch.
_NVL_L="-L${ROOT}/.deps/lib"
case " ${RUSTFLAGS-} " in
  *" ${_NVL_L} "*) ;;
  *) export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }${_NVL_L}" ;;
esac
cd "$ROOT"
exec cargo run --release -p noviewlog-slint -- "$@"
