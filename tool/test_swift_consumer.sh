#!/usr/bin/env bash
# Build fresh, throwaway Swift packages that consume the Kineto runtime the
# way an app would, for iOS device and iOS simulator. Build and link only:
# nothing is installed on or launched in a device or simulator.
#
#   tool/test_swift_consumer.sh [--zip path/to/KinetoRuntimeC.xcframework.zip]
#                               [--keep] [--device-only | --simulator-only]
#
# Consumers (each in a new temporary directory, outside the repository):
#   source  `.package(path: <repo>)` on the root Package.swift, product
#           KinetoRuntime, Swift API (KinetoPlayer).
#   zip     `.binaryTarget(path: KinetoRuntimeC.xcframework.zip)` with a
#           copy of the distribution zip (default build/dist/swift), C ABI
#           only. This is what a `binaryTarget(url:checksum:)` resolves to
#           after download, so it checks the zip's layout and contents.
#
# Each builds its test bundle with `xcodebuild build-for-testing`, which links
# the static libraries into a loadable binary, for generic/platform=iOS and
# generic/platform=iOS Simulator. The script then checks the linked binary
# defines the C ABI the Flutter plugin resolves with dlsym.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ZIP="$ROOT/build/dist/swift/KinetoRuntimeC.xcframework.zip"
KEEP=0
DESTS=("iOS" "iOS Simulator")
while (($#)); do
  case "$1" in
    --keep) KEEP=1 ;;
    --device-only) DESTS=("iOS") ;;
    --simulator-only) DESTS=("iOS Simulator") ;;
    --zip) ZIP="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"; shift ;;
    -h | --help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "error: unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/kineto-swift-consumer.XXXXXX")"
# Kept on failure (and with --keep) so the logs and products can be read.
cleanup() { if (($? == 0 && KEEP == 0)); then rm -rf "$WORK"; else echo "work directory kept: $WORK" >&2; fi; }
trap cleanup EXIT
FIXTURE="$ROOT/runtimes/swift/KinetoRuntime/Tests/KinetoRuntimeTests/Fixtures/pointer_binding.kvk"

build() {
  local dir="$1" scheme="$2" dest="$3"
  local derived="$WORK/derived-$scheme-${dest//[^A-Za-z]/}"
  echo "==> $scheme: build-for-testing ($dest)"
  (cd "$dir" && xcodebuild -quiet build-for-testing \
    -scheme "$scheme" \
    -destination "generic/platform=$dest" \
    -derivedDataPath "$derived" \
    CODE_SIGNING_ALLOWED=NO) >"$derived.log" 2>&1 || {
    tail -40 "$derived.log" >&2
    echo "FAIL: $scheme ($dest); full log $derived.log" >&2
    exit 1
  }
  local bin
  bin="$(find "$derived/Build/Products" -path '*.xctest/*' -type f -perm -u+x ! -name '*.dylib' | head -n 1)"
  [[ -n "$bin" ]] || { echo "FAIL: no linked test bundle in $derived" >&2; exit 1; }
  local defined
  defined="$(nm -gU "$bin" 2>/dev/null)" # not piped: grep -q would SIGPIPE nm under pipefail
  for symbol in _kineto_load _kineto_advance _kineto_render _kineto_destroy; do
    grep -q " $symbol\$" <<<"$defined" || {
      echo "FAIL: $symbol not linked into $bin" >&2
      exit 1
    }
  done
  echo "ok  $scheme  $dest  $(lipo -archs "$bin")  $(basename "$bin")"
}

# --- source: root manifest by local path -----------------------------------
mkdir -p "$WORK/source/Sources/Consumer" "$WORK/source/Tests/ConsumerTests"
cp "$FIXTURE" "$WORK/source/Tests/ConsumerTests/pointer_binding.kvk"
cat >"$WORK/source/Package.swift" <<EOF
// swift-tools-version: 5.10
import PackageDescription
let package = Package(
    name: "Consumer",
    platforms: [.iOS(.v15)],
    products: [.library(name: "Consumer", targets: ["Consumer"])],
    dependencies: [.package(name: "KinetoRuntime", path: "$ROOT")],
    targets: [
        .target(name: "Consumer", dependencies: [.product(name: "KinetoRuntime", package: "KinetoRuntime")]),
        .testTarget(name: "ConsumerTests", dependencies: ["Consumer"], resources: [.copy("pointer_binding.kvk")]),
    ]
)
EOF
cat >"$WORK/source/Sources/Consumer/Consumer.swift" <<'EOF'
import Foundation
import KinetoRuntime
import UIKit

public enum Consumer {
    public static func load(_ data: Data) throws -> (machines: Int, controller: UIViewController) {
        let player = try KinetoPlayer(data: data)
        try player.advance(1.0 / 60.0)
        return (player.stateMachines().count, KinetoPlayerViewController(player: player))
    }
}
EOF
cat >"$WORK/source/Tests/ConsumerTests/ConsumerTests.swift" <<'EOF'
import Consumer
import XCTest

final class ConsumerTests: XCTestCase {
    func testLoadsFixture() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "pointer_binding", withExtension: "kvk"))
        let result = try Consumer.load(Data(contentsOf: url))
        XCTAssertGreaterThan(result.machines, 0)
    }
}
EOF

# --- zip: the distribution artifact as a binary target ----------------------
if [[ -f "$ZIP" ]]; then
  mkdir -p "$WORK/zip/Sources/ZipConsumer" "$WORK/zip/Tests/ZipConsumerTests"
  cp "$ZIP" "$WORK/zip/KinetoRuntimeC.xcframework.zip"
  cat >"$WORK/zip/Package.swift" <<'EOF'
// swift-tools-version: 5.10
import PackageDescription
let package = Package(
    name: "ZipConsumer",
    platforms: [.iOS(.v15)],
    products: [.library(name: "ZipConsumer", targets: ["ZipConsumer"])],
    targets: [
        .binaryTarget(name: "KinetoRuntimeC", path: "KinetoRuntimeC.xcframework.zip"),
        .target(
            name: "ZipConsumer",
            dependencies: ["KinetoRuntimeC"],
            linkerSettings: [.linkedFramework("Metal"), .linkedFramework("QuartzCore")]
        ),
        .testTarget(name: "ZipConsumerTests", dependencies: ["ZipConsumer"]),
    ]
)
EOF
  cat >"$WORK/zip/Sources/ZipConsumer/ZipConsumer.swift" <<'EOF'
import KinetoRuntimeC

public enum ZipConsumer {
    /// Loads bytes through the raw C ABI; false when the engine rejects them.
    public static func load(_ bytes: [UInt8]) -> Bool {
        var status = KinetoStatus_Ok
        let handle = bytes.withUnsafeBufferPointer { buffer in
            kineto_load(buffer.baseAddress, UInt(buffer.count), &status)
        }
        guard let handle else { return false }
        _ = kineto_advance(handle, 0.016)
        kineto_destroy(handle)
        return status == KinetoStatus_Ok
    }

    public static func render(_ handle: OpaquePointer, _ surface: UnsafeMutableRawPointer) -> KinetoStatus {
        kineto_render(handle, surface)
    }
}
EOF
  cat >"$WORK/zip/Tests/ZipConsumerTests/ZipConsumerTests.swift" <<'EOF'
import XCTest
import ZipConsumer

final class ZipConsumerTests: XCTestCase {
    func testRejectsGarbage() { XCTAssertFalse(ZipConsumer.load([0, 1, 2])) }
}
EOF
else
  echo "warning: $ZIP not found; run scripts/package_swift.sh first. Skipping the zip consumer." >&2
fi

for dest in "${DESTS[@]}"; do
  build "$WORK/source" Consumer "$dest"
  [[ -d "$WORK/zip" ]] && build "$WORK/zip" ZipConsumer "$dest"
done

if [[ -d "$WORK/zip" ]]; then
  # The zip's notices must survive extraction next to the XCFramework.
  extracted="$(find "$WORK" -path '*/artifacts/*' -name THIRD_PARTY_NOTICES.md | head -n 1)"
  if [[ -n "$extracted" ]]; then
    echo "ok  notices extracted with the binary: $(ls "$(dirname "$extracted")" | tr '\n' ' ')"
  else
    echo "note: SwiftPM did not keep the zip's notice files in its artifacts directory" >&2
  fi
fi

echo "OK: Swift consumers build and link for device and simulator (build only; nothing was run)"
