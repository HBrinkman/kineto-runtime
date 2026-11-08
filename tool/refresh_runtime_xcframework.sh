#!/usr/bin/env bash
# Rebuilds the Rust engine for iOS and refreshes the local generated
# KinetoRuntimeC.xcframework the Swift package links against.
#
# The package consumes a generated local binary artifact, so any change to the
# engine's C ABI requires this before Swift can link it — keeping the binary
# out of Git avoids adding a large, non-reproducible blob on every rebuild.
# This is also how the artifact drifted far enough that the package stopped compiling
# (issue #111: `kineto_engine.h` predated the Rust source and was missing
# KinetoPointerEvent entirely).
#
# Prerequisites:
#   - Rust toolchain, with:
#       rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
#
# Run from the repo root. See docs/BUILDING.md for what else is
# generated and how to regenerate it after a `flutter clean`.

set -euo pipefail

# shellcheck source=/dev/null
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

XC="runtimes/swift/KinetoRuntime/Artifacts/KinetoRuntimeC.xcframework"
HEADER="engine/kineto_engine/kineto_engine.h"

if [[ ! -d "$XC" ]]; then
  echo "error: $XC not found — run from the repo root" >&2
  exit 2
fi

echo "==> building the engine for iOS (device + both simulator slices)"
(
  cd engine
  for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
    cargo build --release --target "$target" --features metal-render
  done
)

# Ask cargo where it put things rather than assuming engine/target. The output
# directory moved into the Flutter build directory (engine/.cargo/config.toml)
# and CARGO_TARGET_DIR can move it again, so a hardcoded path here would break
# silently — the copies below would fail, or worse, copy a stale artifact.
TARGET_DIR="$(
  cd engine && cargo metadata --format-version 1 --no-deps |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])'
)"
if [[ ! -d "$TARGET_DIR" ]]; then
  echo "error: cargo reports a target directory that does not exist: $TARGET_DIR" >&2
  exit 2
fi

echo "==> refreshing $XC"
cp "$TARGET_DIR/aarch64-apple-ios/release/libkineto_engine.a" \
   "$XC/ios-arm64/libkineto_engine.a"
lipo -create \
  "$TARGET_DIR/aarch64-apple-ios-sim/release/libkineto_engine.a" \
  "$TARGET_DIR/x86_64-apple-ios/release/libkineto_engine.a" \
  -output "$XC/ios-arm64_x86_64-simulator/libkineto_engine_sim.a"

# `build.rs` regenerates the header through cbindgen on every cargo build, so
# it is current by the time we get here.
cp "$HEADER" "$XC/ios-arm64/Headers/kineto_engine.h"
cp "$HEADER" "$XC/ios-arm64_x86_64-simulator/Headers/kineto_engine.h"
cp "$HEADER" runtimes/swift/KinetoRuntime/Artifacts/Headers/kineto_engine.h

echo "==> slices"
lipo -info "$XC/ios-arm64_x86_64-simulator/libkineto_engine_sim.a"

echo "==> done. Verify with:"
echo "   cd runtimes/swift/KinetoRuntime && xcodebuild -scheme KinetoRuntime-Package \\"
echo "     -destination 'platform=iOS Simulator,id=<SIM_UDID>' test"
