#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ "$(uname -s)" == Darwin ]] || { echo 'Find Anything Native requires macOS 13+.' >&2; exit 1; }
LIB_DIR="${FINDANYTHING_LIB_DIR:-$HERE/../../../target/release}"
if [[ -z "${FINDANYTHING_LIB_DIR:-}" ]]; then
  cargo build --manifest-path "$HERE/../../../Cargo.toml" --locked --release -p findanything-ffi
fi
export FINDANYTHING_LIB_DIR="$(cd "$LIB_DIR" && pwd)"
swift test --package-path "$HERE" -c release
swift build --package-path "$HERE" -c release
