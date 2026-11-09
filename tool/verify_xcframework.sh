#!/usr/bin/env bash
# Verify a KinetoRuntimeC.xcframework: every slice declared in Info.plist has
# its library, the library holds exactly the declared architectures, every
# object is built for the declared platform (device vs simulator) and minimum
# iOS version, the C ABI symbols are defined, and the headers match the engine.
#
#   tool/verify_xcframework.sh [path/to/KinetoRuntimeC.xcframework]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
XC="${1:-$ROOT/runtimes/swift/KinetoRuntime/Artifacts/KinetoRuntimeC.xcframework}"
MIN_IOS="${KINETO_IOS_DEPLOYMENT_TARGET:-15.0}"
HEADER="$ROOT/engine/kineto_engine/kineto_engine.h"

fail() { echo "FAIL: $*" >&2; exit 1; }
[[ -f "$XC/Info.plist" ]] || fail "$XC/Info.plist missing"

# Every C ABI entry point. The Android surface constructor is a stub that
# reports an error on iOS, but it is still exported.
expected_symbols() {
  sed -nE 's/.*pub (unsafe )?extern "C" fn (kineto_[a-z_]+).*/_\2/p' \
    "$ROOT/engine/kineto_engine/src/ffi/c.rs" | sort -u
}

slices="$(python3 -I - "$XC/Info.plist" <<'PY'
import plistlib, sys
info = plistlib.load(open(sys.argv[1], "rb"))
for lib in info["AvailableLibraries"]:
    print(lib["LibraryIdentifier"], lib["LibraryPath"], lib.get("SupportedPlatformVariant", "device"),
          ",".join(sorted(lib["SupportedArchitectures"])), lib["HeadersPath"])
PY
)"
[[ -n "$slices" ]] || fail "no AvailableLibraries in Info.plist"

while read -r id lib variant archs headers; do
  path="$XC/$id/$lib"
  [[ -s "$path" ]] || fail "$id: $lib missing (run scripts/bootstrap.sh ios)"
  got="$(lipo -archs "$path" | tr ' ' '\n' | sort | paste -sd, -)"
  [[ "$got" == "$archs" ]] || fail "$id: architectures $got, Info.plist declares $archs"

  case "$variant" in
    device) want_platform=IOS ;;
    simulator) want_platform=IOSSIMULATOR ;;
    *) fail "$id: unexpected platform variant $variant" ;;
  esac
  for arch in ${archs//,/ }; do
    # Every object that carries a build-version load command must be for the
    # right platform: one device object in a simulator slice fails linking.
    platforms="$(otool -arch "$arch" -l "$path" 2>/dev/null |
      awk '/cmd LC_BUILD_VERSION/{b=1} b&&/platform/{print $2; b=0}' | sort | uniq -c)"
    minos="$(otool -arch "$arch" -l "$path" 2>/dev/null |
      awk '/cmd LC_BUILD_VERSION/{b=1} b&&/minos/{print $2; b=0}' | sort -u | paste -sd, -)"
    [[ -n "$platforms" ]] || fail "$id/$arch: no LC_BUILD_VERSION load commands"
    # otool prints the platform as a number (2 iOS, 7 iOS simulator) or a name.
    distinct="$(awk '{print $2}' <<<"$platforms" | sed -e 's/^2$/IOS/' -e 's/^7$/IOSSIMULATOR/' | sort -u)"
    [[ "$distinct" == "$want_platform" ]] ||
      fail "$id/$arch: objects built for $(tr '\n' ' ' <<<"$distinct"), slice is $variant ($want_platform)"
    # Engine objects carry the pinned deployment target; Rust's prebuilt std
    # objects carry the target's default (lower). Lower is compatible; an
    # object above the package's iOS minimum would not be.
    too_new="$(tr ',' '\n' <<<"$minos" | python3 -I -c '
import sys
want = tuple(int(x) for x in sys.argv[1].split("."))
print(",".join(v for v in sys.stdin.read().split() if tuple(int(x) for x in v.split(".")) > want))
' "$MIN_IOS")"
    [[ -z "$too_new" ]] || fail "$id/$arch: objects need iOS $too_new, above the package minimum $MIN_IOS"
    grep -qx "$MIN_IOS" < <(tr ',' '\n' <<<"$minos") || fail "$id/$arch: no object built for iOS $MIN_IOS (minos: $minos); deployment target not applied"

    missing="$(comm -23 <(expected_symbols) <(nm -arch "$arch" -gU "$path" 2>/dev/null | awk '{print $3}' | sort -u))"
    [[ -z "$missing" ]] || fail "$id/$arch: C ABI symbols missing: $(tr '\n' ' ' <<<"$missing")"
  done

  cmp -s "$XC/$id/$headers/kineto_engine.h" "$HEADER" || fail "$id: kineto_engine.h differs from $HEADER"
  [[ -f "$XC/$id/$headers/module.modulemap" ]] || fail "$id: module.modulemap missing"
  echo "ok  $id  $variant  [$archs]  minos $minos (<= $MIN_IOS)  $(du -h "$path" | awk '{print $1}')"
done <<<"$slices"

echo "OK: $XC"
