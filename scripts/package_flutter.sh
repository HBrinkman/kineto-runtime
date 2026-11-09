#!/usr/bin/env bash
# Stage the publishable kineto_runtime Flutter package and validate it with
# `dart pub publish --dry-run`. Never publishes.
#
#   scripts/package_flutter.sh [--build] [--out DIR] [--skip-dry-run]
#
#   --build         build the iOS and Android engine libraries first
#                   (scripts/bootstrap.sh ios / android); otherwise they must
#                   already be built
#   --out DIR       staging root (default build/dist/flutter, ignored by Git)
#   --skip-dry-run  stage only
#
# The checkout's packages/kineto_runtime is a development package: it links
# into runtimes/ and has no native libraries. The staged copy at
# DIR/kineto_runtime is self-contained:
#   - links are resolved into real files (Kotlin sources, licenses);
#   - ios/kineto_runtime/KinetoRuntimeC.xcframework holds the stripped device
#     and simulator static libraries (from scripts/package_swift.sh);
#   - android/src/main/jniLibs/{arm64-v8a,x86_64}/libkineto_engine.so;
#   - web/pkg holds the committed wasm runtime;
#   - LICENSE, THIRD_PARTY_NOTICES.md and licenses/ carry the notices the
#     bundled binaries need;
#   - BUILD_INFO.json records the commit, toolchain and binary hashes;
#   - pubspec.yaml loses `publish_to: none`, nothing else.
#
# Output next to the package: pub-dry-run.txt (the full validator output).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

OUT="$ROOT/build/dist/flutter"
BUILD=0
DRY_RUN=1
while (($#)); do
  case "$1" in
    --build) BUILD=1 ;;
    --skip-dry-run) DRY_RUN=0 ;;
    --out) OUT="$(mkdir -p "$2" && cd "$2" && pwd)"; shift ;;
    -h | --help) sed -n '2,27p' "$0"; exit 0 ;;
    *) echo "error: unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done

fail() { echo "error: $*" >&2; exit 1; }
SRC="packages/kineto_runtime"
PKG="$OUT/kineto_runtime"
ABIS=(arm64-v8a x86_64)
XC_NAME="KinetoRuntimeC.xcframework"

if ((BUILD)); then
  scripts/bootstrap.sh ios
  scripts/bootstrap.sh android
fi

echo "==> preflight: native libraries"
[[ "$(uname -s)" == Darwin ]] || fail "staging needs macOS: the iOS XCFramework is part of the package"
scripts/package_swift.sh --skip-build --out "$OUT/swift" >/dev/null
SWIFT_STAGE="$OUT/swift/stage/$XC_NAME"
[[ -d "$SWIFT_STAGE" ]] || fail "scripts/package_swift.sh did not stage $SWIFT_STAGE"

# llvm-nm and llvm-strip from the NDK when present (they read ELF); Xcode's
# llvm-nm otherwise.
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "$NDK" ]]; then
  for sdk in "${ANDROID_SDK_ROOT:-}" "${ANDROID_HOME:-}" /opt/homebrew/share/android-commandlinetools "$HOME/Library/Android/sdk"; do
    [[ -n "$sdk" && -d "$sdk/ndk" ]] || continue
    NDK="$(find "$sdk/ndk" -maxdepth 1 -mindepth 1 -type d | sort -V | tail -n 1)"
    break
  done
fi
LLVM_BIN=""
[[ -n "$NDK" ]] && LLVM_BIN="$(ls -d "$NDK"/toolchains/llvm/prebuilt/*/bin 2>/dev/null | head -n 1)"
NM="${LLVM_BIN:+$LLVM_BIN/llvm-nm}"
NM="${NM:-$(xcrun --find llvm-nm 2>/dev/null || echo nm)}"
STRIP="${LLVM_BIN:+$LLVM_BIN/llvm-strip}"

expected_android_symbols() {
  sed -nE 's/.*external fun (native[A-Za-z]+).*/Java_app_kineto_runtime_KinetoRuntimeJni_\1/p' \
    runtimes/android/KinetoRuntime/src/main/java/app/kineto/runtime/KinetoRuntimeJni.kt
  sed -nE 's/.*pub (unsafe )?extern "C" fn (kineto_[a-z_]+).*/\2/p' engine/kineto_engine/src/ffi/c.rs |
    grep -v '^kineto_surface_create_from_ca_metal_layer$'
}

for abi in "${ABIS[@]}"; do
  case "$abi" in arm64-v8a) machine=aarch64 ;; x86_64) machine=x86-64 ;; esac
  so="$SRC/android/src/main/jniLibs/$abi/libkineto_engine.so"
  [[ -s "$so" ]] || fail "$so missing; run scripts/bootstrap.sh android (or pass --build)"
  file -b "$so" | grep -qi "ELF 64-bit.*$machine" || fail "$so is not a 64-bit $machine ELF: $(file -b "$so")"
  missing="$(comm -23 <(expected_android_symbols | sort -u) <("$NM" -D --defined-only "$so" | awk '{print $3}' | sort -u))"
  [[ -z "$missing" ]] || fail "$so does not export: $(tr '\n' ' ' <<<"$missing")"
  echo "ok  android $abi  $(du -h "$so" | awk '{print $1}')"
done

echo "==> preflight: web runtime"
for f in kineto_engine.js kineto_engine_bg.wasm kineto_engine.d.ts kineto_engine_bg.wasm.d.ts; do
  cmp -s "runtimes/web/pkg/$f" "$SRC/web/pkg/$f" || fail "$SRC/web/pkg/$f differs from runtimes/web/pkg; run scripts/bootstrap.sh web"
done
cmp -s LICENSE "$SRC/LICENSE" || fail "$SRC/LICENSE differs from the repository LICENSE"
echo "ok  web pkg matches runtimes/web/pkg"

echo "==> staging $PKG"
rm -rf "$PKG" "$OUT/pub-dry-run.txt"
mkdir -p "$PKG"

# Files git would publish (tracked plus not-ignored new files), minus lock
# files: a published package's lock file is ignored by pub consumers.
while IFS= read -r -d '' rel; do
  sub="${rel#"$SRC"/}"
  case "$sub" in
    pubspec.lock | example/pubspec.lock) continue ;;
    licenses | ios/kineto_runtime/"$XC_NAME") continue ;; # staged below
  esac
  [[ -e "$rel" || -L "$rel" ]] || continue
  mkdir -p "$PKG/$(dirname "$sub")"
  if [[ -L "$rel" ]]; then
    target="$(cd "$(dirname "$rel")" && cd "$(dirname "$(readlink "$rel")")" && pwd)/$(basename "$(readlink "$rel")")"
    [[ -f "$target" ]] || fail "unexpected link to a non-file: $rel -> $(readlink "$rel")"
    case "$target" in "$ROOT"/*) ;; *) fail "link leaves the repository: $rel" ;; esac
    cp "$target" "$PKG/$sub"
  else
    cp -p "$rel" "$PKG/$sub"
  fi
done < <(git ls-files -z --cached --others --exclude-standard -- "$SRC")

# Licenses the bundled binaries and the example's template files need.
mkdir -p "$PKG/licenses"
cp licenses/RUST_LICENSE_TEXTS.txt licenses/rust-crates.tsv licenses/Flutter-BSD-3-Clause.txt \
  licenses/Gradle-LICENSE.txt licenses/Gradle-NOTICE.txt "$PKG/licenses/"

# iOS: the stripped, verified XCFramework.
cp -R "$SWIFT_STAGE" "$PKG/ios/kineto_runtime/$XC_NAME"

# Android: debug info stripped (dynamic symbols stay; the JNI and C ABI are
# looked up by name at run time).
for abi in "${ABIS[@]}"; do
  dst="$PKG/android/src/main/jniLibs/$abi"
  mkdir -p "$dst"
  if [[ -n "$STRIP" && -x "$STRIP" ]]; then
    "$STRIP" --strip-debug -o "$dst/libkineto_engine.so" "$SRC/android/src/main/jniLibs/$abi/libkineto_engine.so"
  else
    echo "warning: llvm-strip not found (no NDK); bundling unstripped $abi library" >&2
    cp "$SRC/android/src/main/jniLibs/$abi/libkineto_engine.so" "$dst/"
  fi
done

# pubspec: drop the `publish_to: none` guard and the comment above it.
python3 -I - "$PKG/pubspec.yaml" <<'PY'
import re, sys
path = sys.argv[1]
text = open(path).read()
new, n = re.subn(r"(?m)^(#[^\n]*\n)*publish_to: none\n", "", text, count=1)
if n != 1:
    raise SystemExit("error: publish_to: none not found in the staged pubspec")
open(path, "w").write(new)
PY

# Pub reads .gitignore files in a non-Git directory too; keep local build
# output of the staged copy (pub get, example builds) out of the archive.
cat >"$PKG/.pubignore" <<'EOF'
.dart_tool/
build/
pubspec.lock
example/.dart_tool/
example/build/
example/pubspec.lock
example/ios/Pods/
example/ios/.symlinks/
example/ios/Flutter/ephemeral/
example/android/.gradle/
example/android/local.properties
EOF

COMMIT="$(git rev-parse HEAD)"
DIRTY=false
git diff --quiet HEAD -- engine schema runtimes packages/kineto_runtime || DIRTY=true
python3 -I - "$PKG" "$COMMIT" "$DIRTY" <<'PY'
import hashlib, json, pathlib, subprocess, sys
pkg, commit, dirty = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3] == "true"
def run(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
    except FileNotFoundError:
        return None
binaries = sorted(
    [p for p in pkg.glob("ios/kineto_runtime/KinetoRuntimeC.xcframework/*/*.a")]
    + [p for p in pkg.glob("android/src/main/jniLibs/*/libkineto_engine.so")]
    + [pkg / "web/pkg/kineto_engine_bg.wasm"]
)
info = {
    "package": "kineto_runtime",
    "source_commit": commit,
    "source_dirty": dirty,
    "rustc": run("rustc", "--version"),
    "xcode": " ".join((run("xcodebuild", "-version") or "").split()),
    "binaries": {str(p.relative_to(pkg)): hashlib.sha256(p.read_bytes()).hexdigest() for p in binaries},
}
(pkg / "BUILD_INFO.json").write_text(json.dumps(info, indent=2, sort_keys=True) + "\n")
PY

[[ -z "$(find "$PKG" -type l -print)" ]] || fail "staged package still contains a symlink"
python3 -I tool/check_binary_paths.py "$PKG/ios" "$PKG/android" "$PKG/web" || fail "a bundled binary embeds a build-machine path"
echo "==> staged $(du -sh "$PKG" | awk '{print $1}') in $PKG"

if ((DRY_RUN)); then
  echo "==> dart pub publish --dry-run (validation only; nothing is uploaded)"
  set +e
  # The staging directory sits inside this repository's Git worktree, where
  # build/ is ignored: pub would ask Git which files to include and get none.
  # Stop Git's repository discovery at the staging root so pub reads the
  # package's own .gitignore/.pubignore files instead.
  (cd "$PKG" && export GIT_CEILING_DIRECTORIES="$OUT" &&
    flutter pub get >/dev/null && dart pub publish --dry-run) 2>&1 | tee "$OUT/pub-dry-run.txt"
  status=${PIPESTATUS[0]}
  set -e
  # `flutter pub get` leaves .dart_tool and pubspec.lock behind; both are in
  # .pubignore, but remove them so the staged directory stays exactly what
  # pub would archive.
  rm -rf "$PKG/.dart_tool" "$PKG/pubspec.lock" "$PKG/example/.dart_tool" "$PKG/example/pubspec.lock"
  ((status == 0)) || fail "pub dry-run reported problems (exit $status); see $OUT/pub-dry-run.txt"
  echo "OK: dry-run passed with no warnings ($OUT/pub-dry-run.txt)"
fi
