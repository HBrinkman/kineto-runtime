#!/usr/bin/env bash
# Ensure the generated KinetoRuntimeC iOS libraries exist before an iOS build.
# The libraries remain ignored; this script rebuilds them only when missing.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

XC="runtimes/swift/KinetoRuntime/Artifacts/KinetoRuntimeC.xcframework"
DEVICE="$XC/ios-arm64/libkineto_engine.a"
SIMULATOR="$XC/ios-arm64_x86_64-simulator/libkineto_engine_sim.a"

valid_archive() {
  local path="$1"
  [[ -s "$path" ]] && lipo -info "$path" >/dev/null 2>&1
}

if valid_archive "$DEVICE" && valid_archive "$SIMULATOR"; then
  echo "KinetoRuntimeC artifacts up to date:"
  echo "  $DEVICE"
  echo "  $SIMULATOR"
  lipo -info "$SIMULATOR"
  exit 0
fi

echo "KinetoRuntimeC artifacts missing or invalid; rebuilding..."
"$ROOT/tool/refresh_runtime_xcframework.sh"

if ! valid_archive "$DEVICE" || ! valid_archive "$SIMULATOR"; then
  echo "error: runtime refresh did not produce valid iOS archives" >&2
  exit 1
fi

echo "KinetoRuntimeC artifacts ready:"
echo "  $DEVICE"
echo "  $SIMULATOR"
lipo -info "$SIMULATOR"
