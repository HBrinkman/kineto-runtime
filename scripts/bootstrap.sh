#!/usr/bin/env bash
# Runtime-only bootstrap for a fresh clone of this repository.
#
#   scripts/bootstrap.sh check     report which prerequisites are present
#   scripts/bootstrap.sh host      build the engine for this machine (Rust tests,
#                                  Dart FFI tests and desktop hosts load it)
#   scripts/bootstrap.sh test      run every host-side check: engine tests, wasm
#                                  parity, extraction boundary, license inventory,
#                                  and Flutter package tests if Flutter is present
#   scripts/bootstrap.sh web       rebuild runtimes/web/pkg (wasm-bindgen)
#   scripts/bootstrap.sh ios       build KinetoRuntimeC.xcframework's libraries (macOS)
#   scripts/bootstrap.sh android   build libkineto_engine.so for the Android modules
#
# Nothing here downloads an SDK or touches a device. Missing tools are reported
# with the version this repository is tested against.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE="$ROOT/engine"
FLATC_VERSION="25.12.19"          # pinned by flatbuffers-build in engine/Cargo.lock
WASM_BINDGEN_VERSION="0.2.122"    # pinned by wasm-bindgen in engine/Cargo.lock

[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

say() { printf '%s\n' "$*"; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

need_cargo() {
  have cargo || fail "cargo not found. Install Rust from https://rustup.rs"
}

need_flatc() {
  local flatc="${FLATC_PATH:-flatc}"
  have "$flatc" || fail "flatc $FLATC_VERSION not found (set FLATC_PATH or put it on PATH). Releases: https://github.com/google/flatbuffers/releases/tag/v$FLATC_VERSION"
  local got
  got="$("$flatc" --version | awk '{print $3}')"
  [[ "$got" == "$FLATC_VERSION" ]] || fail "flatc $got found, the engine build needs exactly $FLATC_VERSION"
}

# Where cargo writes when run from engine/ (engine/.cargo/config.toml sends it
# to <repo>/build/cargo; CARGO_TARGET_DIR overrides).
target_dir() {
  (cd "$ENGINE" && cargo metadata --format-version 1 --no-deps) |
    python3 -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])'
}

host_lib_name() {
  case "$(uname -s)" in
    Darwin) echo libkineto_engine.dylib ;;
    Linux) echo libkineto_engine.so ;;
    MINGW* | MSYS* | CYGWIN*) echo kineto_engine.dll ;;
    *) fail "unsupported host $(uname -s)" ;;
  esac
}

host_lib_path() { echo "$(target_dir)/debug/$(host_lib_name)"; }

version_of() {
  if have "$1"; then "$@" 2>/dev/null | head -n 1; else echo "missing"; fi
}

cmd_check() {
  say "repo:          $ROOT"
  say "cargo:         $(version_of cargo --version)"
  say "flatc:         $(version_of "${FLATC_PATH:-flatc}" --version)   (required: $FLATC_VERSION)"
  say "python3:       $(version_of python3 --version)"
  say "node:          $(version_of node --version)   (web parity)"
  say "wasm-bindgen:  $(version_of wasm-bindgen --version)   (web rebuild only: $WASM_BINDGEN_VERSION)"
  say "flutter:       $(version_of flutter --version)   (Flutter package, optional)"
  if [[ "$(uname -s)" == Darwin ]]; then
    say "xcodebuild:    $(version_of xcodebuild -version)   (iOS, optional)"
  fi
  say "android ndk:   ${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-unset}}   (Android, optional)"
}

cmd_host() {
  need_cargo
  need_flatc
  (cd "$ENGINE" && cargo build --locked -p kineto_engine)
  local lib
  lib="$(host_lib_path)"
  [[ -f "$lib" ]] || fail "cargo finished but $lib is missing"
  say "host engine library: $lib"
  say "Dart/Flutter host tests find it automatically; elsewhere export KINETO_ENGINE_DYLIB=$lib"
}

cmd_test() {
  need_cargo
  need_flatc
  have python3 || fail "python3 is required"

  say "==> extraction boundary"
  python3 -I "$ROOT/tool/test_extraction_boundary.py"
  python3 -I "$ROOT/tool/check_extraction_boundary.py"

  say "==> Rust license inventory"
  python3 -I "$ROOT/tool/rust_license_inventory.py" --check

  say "==> engine tests (host)"
  (cd "$ENGINE" && cargo test --locked)
  (cd "$ENGINE" && cargo test --locked --features vello-render)

  say "==> generated C header is current"
  git -C "$ROOT" diff --exit-code -- engine/kineto_engine/kineto_engine.h

  say "==> web contract: committed wasm against the shared fixtures"
  have node || fail "node is required for the wasm parity runner"
  (cd "$ROOT/runtimes/web" && node ../parity/run_wasm_parity.mjs)
  cmp -s "$ROOT/runtimes/web/pkg/kineto_engine_bg.wasm" "$ROOT/packages/kineto_runtime/web/pkg/kineto_engine_bg.wasm" ||
    fail "runtimes/web/pkg and packages/kineto_runtime/web/pkg disagree; rerun scripts/build_runtime_web.sh"

  if have flutter; then
    say "==> Flutter package (host FFI)"
    cmd_host
    local lib
    lib="$(host_lib_path)"
    (cd "$ROOT/packages/kineto_runtime" &&
      flutter pub get --enforce-lockfile &&
      flutter analyze &&
      KINETO_ENGINE_DYLIB="$lib" flutter test)
    (cd "$ROOT/packages/kineto_runtime/example" &&
      flutter pub get --enforce-lockfile &&
      flutter analyze)
  else
    say "==> Flutter not found: skipped the Flutter package tests"
  fi
  say "OK: runtime host checks passed"
}

cmd_web() {
  need_flatc
  "$ROOT/scripts/build_runtime_web.sh"
}

cmd_ios() {
  [[ "$(uname -s)" == Darwin ]] || fail "iOS libraries build on macOS only"
  need_flatc
  (cd "$ROOT" && tool/refresh_runtime_xcframework.sh)
}

cmd_android() {
  need_flatc
  "$ROOT/scripts/build_runtime_android.sh"
}

case "${1:-check}" in
  check) cmd_check ;;
  host) cmd_host ;;
  test) cmd_test ;;
  web) cmd_web ;;
  ios) cmd_ios ;;
  android) cmd_android ;;
  -h | --help | help) sed -n '2,15p' "$0" ;;
  *) fail "unknown command '$1' (try --help)" ;;
esac
