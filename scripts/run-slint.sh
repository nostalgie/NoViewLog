#!/usr/bin/env bash
# Build/run the Slint UI (default product) without system libfontconfig-dev.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ ! -f "${ROOT}/.deps/pkgconfig/fontconfig.pc" ]]; then
  bash "${ROOT}/scripts/setup-slint-deps.sh"
fi
export PKG_CONFIG_PATH="${ROOT}/.deps/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LIBRARY_PATH="${ROOT}/.deps/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
export RUSTFLAGS="${RUSTFLAGS:-} -L${ROOT}/.deps/lib"
cd "$ROOT"
exec cargo run -p noviewlog-slint -- "$@"
