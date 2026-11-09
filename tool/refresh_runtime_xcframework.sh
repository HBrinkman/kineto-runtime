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

ROOT="$(pwd)"

# Reproducible-build inputs. The same commit, toolchain (rust-toolchain pinned
# by docs/BUILDING.md) and Cargo.lock then produce the same archive bytes on
# any machine, whatever the checkout or cargo home path:
#   - absolute paths baked into panic locations and file!() are remapped;
#   - the deployment target matches Package.swift (iOS 15), so the binaries
#     do not depend on the host Xcode's default;
#   - ZERO_AR_DATE makes Apple's ar/ranlib/libtool/strip write zero mtimes.
# CARGO_TARGET_DIR is remapped too: generated FlatBuffers code is include!d
# from OUT_DIR.
IOS_DEPLOYMENT_TARGET="${KINETO_IOS_DEPLOYMENT_TARGET:-15.0}"
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
TARGET_DIR="$(
  cd engine && cargo metadata --format-version 1 --no-deps |
    python3 -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])'
)"
export IPHONEOS_DEPLOYMENT_TARGET="$IOS_DEPLOYMENT_TARGET"
export ZERO_AR_DATE=1
# rustc applies the last matching prefix, so the target directory (inside the
# checkout) comes after the checkout. Cargo spells the default target dir as
# written in engine/.cargo/config.toml, unnormalised (engine/../build/cargo),
# so that spelling is remapped too.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$CARGO_HOME_DIR=/cargo --remap-path-prefix=$ROOT=/kineto-runtime --remap-path-prefix=$TARGET_DIR=/kineto-build --remap-path-prefix=$ROOT/engine/../build/cargo=/kineto-build"

echo "==> building the engine for iOS $IOS_DEPLOYMENT_TARGET (device + both simulator slices)"
(
  cd engine
  for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
    cargo build --locked --release -p kineto_engine --target "$target" --features metal-render
  done
)

# TARGET_DIR (above) is asked of cargo rather than assumed: the output
# directory moved into <repo>/build/cargo (engine/.cargo/config.toml) and
# CARGO_TARGET_DIR can move it again, so a hardcoded path would break
# silently — the copies below would fail, or worse, copy a stale artifact.
if [[ ! -d "$TARGET_DIR" ]]; then
  echo "error: cargo reports a target directory that does not exist: $TARGET_DIR" >&2
  exit 2
fi

# KINETO_XCFRAMEWORK_OUT writes a separate copy instead of the checkout's
# Artifacts tree (scripts/package_swift.sh --check-reproducible uses it).
OUT_XC="${KINETO_XCFRAMEWORK_OUT:-$XC}"
if [[ "$OUT_XC" != "$XC" ]]; then
  rm -rf "$OUT_XC"
  mkdir -p "$OUT_XC/ios-arm64/Headers" "$OUT_XC/ios-arm64_x86_64-simulator/Headers"
  cp "$XC/Info.plist" "$OUT_XC/Info.plist"
  cp "$XC/ios-arm64/Headers/module.modulemap" "$OUT_XC/ios-arm64/Headers/"
  cp "$XC/ios-arm64_x86_64-simulator/Headers/module.modulemap" "$OUT_XC/ios-arm64_x86_64-simulator/Headers/"
fi

echo "==> refreshing $OUT_XC"
cp "$TARGET_DIR/aarch64-apple-ios/release/libkineto_engine.a" \
   "$OUT_XC/ios-arm64/libkineto_engine.a"
lipo -create \
  "$TARGET_DIR/aarch64-apple-ios-sim/release/libkineto_engine.a" \
  "$TARGET_DIR/x86_64-apple-ios/release/libkineto_engine.a" \
  -output "$OUT_XC/ios-arm64_x86_64-simulator/libkineto_engine_sim.a"

# `build.rs` regenerates the header through cbindgen on every cargo build, so
# it is current by the time we get here.
cp "$HEADER" "$OUT_XC/ios-arm64/Headers/kineto_engine.h"
cp "$HEADER" "$OUT_XC/ios-arm64_x86_64-simulator/Headers/kineto_engine.h"
if [[ "$OUT_XC" == "$XC" ]]; then
  cp "$HEADER" runtimes/swift/KinetoRuntime/Artifacts/Headers/kineto_engine.h
fi

echo "==> slices"
lipo -info "$OUT_XC/ios-arm64_x86_64-simulator/libkineto_engine_sim.a"

echo "==> done. Verify with:"
echo "   cd runtimes/swift/KinetoRuntime && xcodebuild -scheme KinetoRuntime-Package \\"
echo "     -destination 'platform=iOS Simulator,id=<SIM_UDID>' test"
