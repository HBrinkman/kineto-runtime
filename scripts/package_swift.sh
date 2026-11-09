#!/usr/bin/env bash
# Package KinetoRuntimeC.xcframework for distribution as a SwiftPM binary target.
#
#   scripts/package_swift.sh [--skip-build] [--out DIR] [--check-reproducible]
#                            [--asset-url URL]
#
# Output (default DIR: build/dist/swift, ignored by Git):
#   KinetoRuntimeC.xcframework.zip   the SwiftPM binary artifact: the
#                                    XCFramework (stripped static libraries for
#                                    ios-arm64 and ios-arm64_x86_64-simulator)
#                                    plus LICENSE, THIRD_PARTY_NOTICES.md and
#                                    the Rust license texts it must ship with
#   KinetoRuntimeC.xcframework.zip.checksum   `swift package compute-checksum`
#   SHA256SUMS                       zip and per-slice library hashes
#   build-info.json                  commit, toolchain and flags that made it
#
# Nothing is uploaded and no manifest is edited. With --asset-url, the script
# prints the `releasedBinary` line a release commit would put in Package.swift;
# the URL is yours to supply, there is no default. See docs/RELEASING.md.
#
# --check-reproducible rebuilds everything in a second, empty cargo target
# directory and fails unless the zip is byte-identical.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

OUT="$ROOT/build/dist/swift"
SKIP_BUILD=0
CHECK_REPRO=0
ASSET_URL=""

while (($#)); do
  case "$1" in
    --skip-build) SKIP_BUILD=1 ;;
    --check-reproducible) CHECK_REPRO=1 ;;
    --out) OUT="$(mkdir -p "$2" && cd "$2" && pwd)"; shift ;;
    --asset-url) ASSET_URL="$2"; shift ;;
    -h | --help) sed -n '2,24p' "$0"; exit 0 ;;
    *) echo "error: unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done

[[ "$(uname -s)" == Darwin ]] || { echo "error: XCFramework packaging needs macOS/Xcode" >&2; exit 1; }
if [[ -n "$ASSET_URL" && "$ASSET_URL" != https://* ]]; then
  echo "error: --asset-url must be an https URL (SwiftPM requires it)" >&2
  exit 2
fi

export ZERO_AR_DATE=1
XC_SRC="runtimes/swift/KinetoRuntime/Artifacts/KinetoRuntimeC.xcframework"
NAME="KinetoRuntimeC.xcframework"

# stage <xcframework-src> <stage-dir>: copy the XCFramework with stripped
# libraries and the notices next to it.
stage() {
  local src="$1" dir="$2"
  rm -rf "$dir"
  mkdir -p "$dir/$NAME" "$dir/licenses"
  cp "$src/Info.plist" "$dir/$NAME/Info.plist"
  local slice lib
  for slice in ios-arm64 ios-arm64_x86_64-simulator; do
    mkdir -p "$dir/$NAME/$slice/Headers"
    cp "$src/$slice/Headers/kineto_engine.h" "$src/$slice/Headers/module.modulemap" "$dir/$NAME/$slice/Headers/"
    for lib in "$src/$slice"/*.a; do
      # Debug symbols only (-S): local and global symbols stay, so the
      # archive still links and the C ABI stays visible to dart:ffi hosts.
      # strip warns once per Rust object that has nothing to remove.
      strip -S -o "$dir/$NAME/$slice/$(basename "$lib")" "$lib" 2>"$dir/strip.log" || {
        cat "$dir/strip.log" >&2
        exit 1
      }
      grep -v "input object file already stripped" "$dir/strip.log" >&2 || true
      rm -f "$dir/strip.log"
    done
  done
  cp LICENSE THIRD_PARTY_NOTICES.md "$dir/"
  cp licenses/RUST_LICENSE_TEXTS.txt licenses/rust-crates.tsv "$dir/licenses/"
}

package() {
  local src="$1" out="$2"
  local stage_dir="$out/stage"
  "$ROOT/tool/verify_xcframework.sh" "$src"
  stage "$src" "$stage_dir"
  "$ROOT/tool/verify_xcframework.sh" "$stage_dir/$NAME"
  python3 -I "$ROOT/tool/check_binary_paths.py" "$stage_dir/$NAME"
  python3 -I "$ROOT/tool/deterministic_zip.py" --base "$stage_dir" --output "$out/$NAME.zip" \
    "$NAME" LICENSE THIRD_PARTY_NOTICES.md licenses >/dev/null
}

if ((SKIP_BUILD == 0)); then
  tool/refresh_runtime_xcframework.sh
fi

mkdir -p "$OUT"
package "$XC_SRC" "$OUT"

CHECKSUM="$(swift package compute-checksum "$OUT/$NAME.zip")"
SHA="$(shasum -a 256 "$OUT/$NAME.zip" | awk '{print $1}')"
[[ "$CHECKSUM" == "$SHA" ]] || { echo "error: compute-checksum ($CHECKSUM) != sha256 ($SHA)" >&2; exit 1; }
echo "$CHECKSUM" >"$OUT/$NAME.zip.checksum"
(
  cd "$OUT"
  shasum -a 256 "$NAME.zip" stage/$NAME/*/*.a | sed 's#stage/##'
) >"$OUT/SHA256SUMS"

COMMIT="$(git rev-parse HEAD)"
DIRTY=false
git diff --quiet HEAD -- engine schema runtimes/swift || DIRTY=true
python3 -I - "$OUT/build-info.json" "$COMMIT" "$DIRTY" "$CHECKSUM" <<'PY'
import json, subprocess, sys
out, commit, dirty, checksum = sys.argv[1:5]
def run(*cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
json.dump({
    "artifact": "KinetoRuntimeC.xcframework.zip",
    "checksum": checksum,
    "source_commit": commit,
    "source_dirty": dirty == "true",
    "rustc": run("rustc", "--version"),
    "cargo": run("cargo", "--version"),
    "xcode": " ".join(run("xcodebuild", "-version").split()),
    "ios_deployment_target": "15.0",
    "features": ["metal-render"],
    "slices": {"ios-arm64": ["arm64"], "ios-arm64_x86_64-simulator": ["arm64", "x86_64"]},
}, open(out, "w"), indent=2, sort_keys=True)
open(out, "a").write("\n")
PY

if ((CHECK_REPRO)); then
  REPRO="$ROOT/build/repro"
  echo "==> reproducibility: rebuilding in an empty target directory ($REPRO/cargo)"
  rm -rf "$REPRO"
  mkdir -p "$REPRO"
  # A scratch copy of the Artifacts tree, so the rebuild cannot overwrite the
  # libraries just packaged.
  CARGO_TARGET_DIR="$REPRO/cargo" KINETO_XCFRAMEWORK_OUT="$REPRO/$NAME" \
    tool/refresh_runtime_xcframework.sh
  package "$REPRO/$NAME" "$REPRO/out"
  if cmp -s "$OUT/$NAME.zip" "$REPRO/out/$NAME.zip"; then
    echo "OK: reproducible ($CHECKSUM)"
  else
    echo "FAIL: second build differs" >&2
    (cd "$REPRO/out" && shasum -a 256 stage/$NAME/*/*.a) >&2
    exit 1
  fi
fi

echo "==> $OUT/$NAME.zip"
echo "    checksum $CHECKSUM"
if [[ -n "$ASSET_URL" ]]; then
  echo "==> after uploading the zip unchanged to $ASSET_URL, a release commit sets:"
  echo "let releasedBinary: (url: String, checksum: String)? = ("
  echo "    url: \"$ASSET_URL\","
  echo "    checksum: \"$CHECKSUM\""
  echo ")"
fi
