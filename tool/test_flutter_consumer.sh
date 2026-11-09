#!/usr/bin/env bash
# Create a brand-new Flutter app outside the repository, depend on a pristine
# copy of the staged kineto_runtime package, and build it. Build only: nothing
# is installed on or launched in a device, emulator or simulator.
#
#   tool/test_flutter_consumer.sh [--package DIR] [--keep] [TARGET ...]
#
#   --package DIR  staged package (default build/dist/flutter/kineto_runtime,
#                  from scripts/package_flutter.sh)
#   --keep         keep the temporary app and print its path
#   TARGET         any of: web apk ios-spm ios-pods ios-sim (default: all)
#
# Per target, beyond "the build succeeded":
#   web       the wasm, its glue and the bridge are in build/web/assets and
#             identical to the package's
#   apk       lib/arm64-v8a and lib/x86_64 libkineto_engine.so are in the APK
#   ios-spm   release device build with Swift Package Manager; every C ABI
#             symbol dart:ffi looks up is in Runner (tool/check_runtime_symbols.sh)
#   ios-pods  the same with CocoaPods
#   ios-sim   debug simulator build (SPM)
#
# The package copy is used through a `path:` dependency, which pub treats like
# an unpacked hosted package: the same files `dart pub publish` would upload.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG_SRC="$ROOT/build/dist/flutter/kineto_runtime"
KEEP=0
TARGETS=()
while (($#)); do
  case "$1" in
    --package) PKG_SRC="$(cd "$2" && pwd)"; shift ;;
    --keep) KEEP=1 ;;
    -h | --help) sed -n '2,24p' "$0"; exit 0 ;;
    web | apk | ios-spm | ios-pods | ios-sim) TARGETS+=("$1") ;;
    *) echo "error: unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done
((${#TARGETS[@]})) || TARGETS=(web apk ios-spm ios-pods ios-sim)
[[ -f "$PKG_SRC/pubspec.yaml" ]] || { echo "error: no staged package at $PKG_SRC (run scripts/package_flutter.sh)" >&2; exit 1; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/kineto-flutter-consumer.XXXXXX")"
if ((KEEP)); then trap 'echo "kept: $WORK"' EXIT; else trap 'rm -rf "$WORK"' EXIT; fi

fail() { echo "FAIL: $*" >&2; exit 1; }

echo "==> pristine package copy and new app in $WORK"
cp -R "$PKG_SRC" "$WORK/kineto_runtime"
rm -rf "$WORK/kineto_runtime/.dart_tool" "$WORK/kineto_runtime/build"
flutter create --no-pub --org dev.example.kinetoconsumer --project-name kineto_consumer \
  --platforms ios,android,web "$WORK/app" >"$WORK/create.log" 2>&1 || { cat "$WORK/create.log"; fail "flutter create"; }

APP="$WORK/app"
mkdir -p "$APP/assets"
cp "$WORK/kineto_runtime/example/assets/sample.kvk" "$APP/assets/sample.kvk"

# Dependency, asset and (per build) the SPM switch, all project-local: the
# machine's global `flutter config` is never touched.
set_spm() {
  python3 -I - "$APP/pubspec.yaml" "$1" "$WORK/kineto_runtime" <<'PY'
import re, sys
path, spm, pkg = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(path).read()
if "kineto_runtime:" not in text:
    text = text.replace("dependencies:\n  flutter:\n    sdk: flutter\n",
                        f"dependencies:\n  flutter:\n    sdk: flutter\n  kineto_runtime:\n    path: {pkg}\n", 1)
text = re.sub(r"\n  config:\n    enable-swift-package-manager: \w+\n", "\n", text)
text = text.replace("\nflutter:\n", f"\nflutter:\n  config:\n    enable-swift-package-manager: {spm}\n  assets:\n    - assets/sample.kvk\n", 1)
text = re.sub(r"(  assets:\n    - assets/sample.kvk\n)+", "  assets:\n    - assets/sample.kvk\n", text)
open(path, "w").write(text)
PY
}
set_spm true

cat >"$APP/lib/main.dart" <<'EOF'
import 'package:flutter/material.dart';
import 'package:kineto_runtime/kineto_runtime.dart';

void main() => runApp(const MaterialApp(home: Scaffold(body: Player())));

class Player extends StatefulWidget {
  const Player({super.key});

  @override
  State<Player> createState() => _PlayerState();
}

class _PlayerState extends State<Player> {
  KinetoPlayer? _player;
  Object? _error;

  @override
  void initState() {
    super.initState();
    KinetoPlayer.loadAsset('assets/sample.kvk').then(
      (player) => setState(() => _player = player),
      onError: (Object error) => setState(() => _error = error),
    );
  }

  @override
  void dispose() {
    _player?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_error != null) return Center(child: Text('$_error'));
    final player = _player;
    if (player == null) return const Center(child: CircularProgressIndicator());
    return player.asWidget();
  }
}
EOF
rm -rf "$APP/test"

(cd "$APP" && flutter pub get >"$WORK/pub-get.log" 2>&1) || { cat "$WORK/pub-get.log"; fail "flutter pub get"; }
(cd "$APP" && flutter analyze >"$WORK/analyze.log" 2>&1) || { cat "$WORK/analyze.log"; fail "flutter analyze"; }
echo "ok  pub get + analyze"

run() {
  local name="$1"; shift
  echo "==> $name: $*"
  (cd "$APP" && "$@") >"$WORK/$name.log" 2>&1 || { tail -60 "$WORK/$name.log" >&2; fail "$name (log: $WORK/$name.log)"; }
}

for target in "${TARGETS[@]}"; do
  case "$target" in
    web)
      run web flutter build web --release
      for f in web/pkg/kineto_engine_bg.wasm web/pkg/kineto_engine.js web/kineto_runtime_bridge.js; do
        cmp -s "$APP/build/web/assets/packages/kineto_runtime/$f" "$WORK/kineto_runtime/$f" ||
          fail "web build is missing or altered assets/packages/kineto_runtime/$f"
      done
      echo "ok  web: wasm, glue and bridge bundled unchanged"
      ;;
    apk)
      run apk flutter build apk --debug
      apk="$APP/build/app/outputs/flutter-apk/app-debug.apk"
      listing="$(unzip -l "$apk")"
      for abi in arm64-v8a x86_64; do
        grep -q "lib/$abi/libkineto_engine.so" <<<"$listing" || fail "APK lacks lib/$abi/libkineto_engine.so"
      done
      echo "ok  apk: $(grep -c 'libkineto_engine.so' <<<"$listing") engine libraries ($(du -h "$apk" | awk '{print $1}'))"
      ;;
    ios-spm | ios-pods)
      if [[ "$target" == ios-spm ]]; then set_spm true; else set_spm false; fi
      rm -rf "$APP/build/ios" "$APP/ios/Pods" "$APP/ios/Podfile.lock"
      run "$target" flutter build ios --release --no-codesign
      if [[ "$target" == ios-spm ]]; then
        grep -q "kineto_runtime" "$APP/ios/Flutter/ephemeral/Packages/FlutterGeneratedPluginSwiftPackage/Package.swift" ||
          fail "SPM build did not use the plugin's Swift package"
      else
        [[ -d "$APP/ios/Pods" ]] && grep -q "kineto_runtime" "$APP/ios/Podfile.lock" || fail "CocoaPods build did not install the kineto_runtime pod"
      fi
      "$ROOT/tool/check_runtime_symbols.sh" "$APP/build/ios/iphoneos/Runner.app/Runner"
      echo "ok  $target: release device build, engine linked"
      ;;
    ios-sim)
      set_spm true
      run ios-sim flutter build ios --simulator --debug
      echo "ok  ios-sim: debug simulator build"
      ;;
  esac
done

echo "OK: clean consumer app builds for: ${TARGETS[*]} (build only; nothing was run)"
