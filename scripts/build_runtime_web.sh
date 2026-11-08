#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE_DIR="$ROOT_DIR/engine"
OUT_DIR="$ROOT_DIR/runtimes/web/pkg"
FLUTTER_OUT_DIR="$ROOT_DIR/packages/kineto_runtime/web/pkg"

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust or source ~/.cargo/env." >&2
  exit 1
fi
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen not found. Install with: cargo install wasm-bindgen-cli --version 0.2.122" >&2
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-unknown-unknown
fi

cargo build \
  --manifest-path "$ENGINE_DIR/Cargo.toml" \
  -p kineto_engine \
  --target wasm32-unknown-unknown \
  --features web-runtime \
  --release

# Ask cargo where it wrote the wasm: engine/.cargo/config.toml moves the
# target dir to <repo>/build/cargo, so engine/target would be a stale (or
# missing) artifact. Same approach as tool/refresh_runtime_xcframework.sh.
TARGET_DIR="$(
  cargo metadata --manifest-path "$ENGINE_DIR/Cargo.toml" --format-version 1 --no-deps |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])'
)"
WASM_IN="$TARGET_DIR/wasm32-unknown-unknown/release/kineto_engine.wasm"
if [[ ! -f "$WASM_IN" ]]; then
  echo "built wasm not found at $WASM_IN" >&2
  exit 2
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
wasm-bindgen \
  "$WASM_IN" \
  --target web \
  --out-dir "$OUT_DIR" \
  --out-name kineto_engine

cat > "$OUT_DIR/README.md" <<'README'
# Generated Web Runtime Artifact

`kineto_engine.js`, `kineto_engine.d.ts` and `kineto_engine_bg.wasm` are
wasm-bindgen web-target output, produced by `scripts/build_runtime_web.sh`
from `engine/kineto_engine/src/wasm.rs` (feature `web-runtime`).

Do not edit these files by hand. Change the Rust source and rebuild. The same
output is mirrored to `packages/kineto_runtime/web/pkg`.
README

rm -rf "$FLUTTER_OUT_DIR"
mkdir -p "$FLUTTER_OUT_DIR"
cp "$OUT_DIR"/kineto_engine* "$FLUTTER_OUT_DIR"/
cp "$OUT_DIR/README.md" "$FLUTTER_OUT_DIR/README.md"

echo "✓ Web runtime artifact copied:"
echo "$OUT_DIR/kineto_engine.js"
echo "$OUT_DIR/kineto_engine_bg.wasm"
echo "$FLUTTER_OUT_DIR/kineto_engine.js"
echo "$FLUTTER_OUT_DIR/kineto_engine_bg.wasm"
