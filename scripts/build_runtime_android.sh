#!/usr/bin/env bash
# Build `kineto_engine` Android runtime libraries and copy them into the
# Flutter plugin plus native Android runtime package layout.
#
# Run from the repo root.

set -euo pipefail

[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE_DIR="$ROOT_DIR/engine"
CRATE="kineto_engine"
LIB_NAME="libkineto_engine.so"
ANDROID_FEATURES="android-render"

SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}}"
SDKMANAGER="${SDK_ROOT}/cmdline-tools/latest/bin/sdkmanager"
NDK_VERSION="${KINETO_ANDROID_NDK_VERSION:-29.0.14033849}"

declare -a TARGETS=(
  "aarch64-linux-android:arm64-v8a"
  "x86_64-linux-android:x86_64"
)

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust via https://rustup.rs" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup not found. Install Rust via https://rustup.rs" >&2
  exit 1
fi

find_ndk() {
  if [[ -n "${ANDROID_NDK_HOME:-}" && -d "${ANDROID_NDK_HOME}" ]]; then
    echo "${ANDROID_NDK_HOME}"
    return
  fi
  if [[ -n "${ANDROID_NDK_ROOT:-}" && -d "${ANDROID_NDK_ROOT}" ]]; then
    echo "${ANDROID_NDK_ROOT}"
    return
  fi
  if [[ -d "${SDK_ROOT}/ndk/${NDK_VERSION}" ]]; then
    echo "${SDK_ROOT}/ndk/${NDK_VERSION}"
    return
  fi
  if [[ -d "${SDK_ROOT}/ndk" ]]; then
    find "${SDK_ROOT}/ndk" -maxdepth 1 -mindepth 1 -type d | sort -V | tail -n 1
    return
  fi
}

NDK_ROOT="$(find_ndk)"

if [[ -z "${NDK_ROOT}" || ! -d "${NDK_ROOT}" ]]; then
  cat >&2 <<EOF
error: Android NDK not found.

Set ANDROID_NDK_HOME / ANDROID_NDK_ROOT, or install the expected NDK:

  "${SDKMANAGER}" "ndk;${NDK_VERSION}"

Detected SDK root: ${SDK_ROOT}
EOF
  exit 1
fi

HOST_TAG="darwin-x86_64"
if [[ "$(uname -s)" == "Darwin" && "$(uname -m)" == "arm64" ]]; then
  HOST_TAG="darwin-x86_64"
elif [[ "$(uname -s)" == "Linux" ]]; then
  HOST_TAG="linux-x86_64"
fi

TOOLCHAIN_BIN="${NDK_ROOT}/toolchains/llvm/prebuilt/${HOST_TAG}/bin"
if [[ ! -d "${TOOLCHAIN_BIN}" ]]; then
  echo "error: NDK LLVM toolchain not found at ${TOOLCHAIN_BIN}" >&2
  exit 1
fi

export ANDROID_NDK_HOME="${NDK_ROOT}"
export ANDROID_NDK_ROOT="${NDK_ROOT}"
export CC_aarch64_linux_android="${TOOLCHAIN_BIN}/aarch64-linux-android24-clang"
export CXX_aarch64_linux_android="${TOOLCHAIN_BIN}/aarch64-linux-android24-clang++"
export AR_aarch64_linux_android="${TOOLCHAIN_BIN}/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${TOOLCHAIN_BIN}/aarch64-linux-android24-clang"
export CC_x86_64_linux_android="${TOOLCHAIN_BIN}/x86_64-linux-android24-clang"
export CXX_x86_64_linux_android="${TOOLCHAIN_BIN}/x86_64-linux-android24-clang++"
export AR_x86_64_linux_android="${TOOLCHAIN_BIN}/llvm-ar"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="${TOOLCHAIN_BIN}/x86_64-linux-android24-clang"

# Ask cargo where it writes rather than assuming: engine/.cargo/config.toml moved
# output to build/cargo, and CARGO_TARGET_DIR can move it again.
TARGET_DIR="$(cd "${ENGINE_DIR}" && cargo metadata --format-version 1 --no-deps \
  | sed -E 's/.*"target_directory":"([^"]+)".*/\1/')"
if [[ ! -d "${TARGET_DIR}" && "${TARGET_DIR}" != /* ]]; then
  echo "error: could not read cargo's target_directory: ${TARGET_DIR}" >&2
  exit 1
fi

# Every native a Kotlin host declares and every C entry point Dart can look up.
# A symbol missing from the .so links fine and fails on the device, at the call.
expected_symbols() {
  sed -nE 's/.*external fun (native[A-Za-z]+).*/Java_app_kineto_runtime_KinetoRuntimeJni_\1/p' \
    "${ROOT_DIR}/runtimes/android/KinetoRuntime/src/main/java/app/kineto/runtime/KinetoRuntimeJni.kt"
  sed -nE 's/.*pub (unsafe )?extern "C" fn (kineto_[a-z_]+).*/\2/p' \
    "${ENGINE_DIR}/kineto_engine/src/ffi/c.rs" \
    | grep -v '^kineto_surface_create_from_ca_metal_layer$'
}

echo "→ Android SDK: ${SDK_ROOT}"
echo "→ Android NDK: ${NDK_ROOT}"

for entry in "${TARGETS[@]}"; do
  target="${entry%%:*}"
  abi="${entry##*:}"

  echo "→ ensuring Rust target ${target}"
  rustup target add "${target}"

  echo "→ building ${CRATE} for ${target} (${abi})"
  (cd "${ENGINE_DIR}" && cargo build --release -p "${CRATE}" --target "${target}" --features "${ANDROID_FEATURES}")

  src="${TARGET_DIR}/${target}/release/${LIB_NAME}"
  if [[ ! -f "${src}" ]]; then
    echo "error: expected artifact missing: ${src}" >&2
    exit 1
  fi

  exported="$("${TOOLCHAIN_BIN}/llvm-nm" -D --defined-only "${src}" | awk '{print $3}')"
  missing="$(comm -23 <(expected_symbols | sort -u) <(sort -u <<<"${exported}"))"
  if [[ -n "${missing}" ]]; then
    echo "error: ${src} does not export:" >&2
    echo "${missing}" >&2
    exit 1
  fi

  flutter_out="${ROOT_DIR}/packages/kineto_runtime/android/src/main/jniLibs/${abi}"
  native_out="${ROOT_DIR}/runtimes/android/KinetoRuntime/src/main/jniLibs/${abi}"
  mkdir -p "${flutter_out}" "${native_out}"
  cp "${src}" "${flutter_out}/${LIB_NAME}"
  cp "${src}" "${native_out}/${LIB_NAME}"
done

echo "✓ Android runtime artifacts copied:"
find "${ROOT_DIR}/packages/kineto_runtime/android/src/main/jniLibs" \
  "${ROOT_DIR}/runtimes/android/KinetoRuntime/src/main/jniLibs" \
  -name "${LIB_NAME}" -print
