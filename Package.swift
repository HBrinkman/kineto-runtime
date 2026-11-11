// swift-tools-version: 5.10
//
// Root Swift package for the Kineto runtime, so the repository can be added
// with Xcode's "Add Package Dependencies…" or `.package(path:)` / `.package(url:)`.
//
// Sources and tests live in runtimes/swift/KinetoRuntime (which keeps its own
// manifest for the Flutter plugin). This manifest points at the same files.
//
// The C ABI (`KinetoRuntimeC`) is a static-library XCFramework built from the
// Rust engine. Where it comes from depends on `releasedBinary`:
//
//   nil (default, every commit that is not a release):
//     the locally built XCFramework in runtimes/swift/KinetoRuntime/Artifacts.
//     Its .a files are not committed; build them first with
//     `scripts/bootstrap.sh ios`. A `.package(url:)` dependency on such a commit
//     resolves but cannot link, because the libraries are absent from Git.
//
//   (url, checksum):
//     a release commit sets this to the uploaded KinetoRuntimeC.xcframework.zip
//     and the checksum `scripts/package_swift.sh` prints. SwiftPM then downloads
//     and verifies the binary, and `.package(url:, exact:)` works without Rust.
//     See docs/RELEASING.md. Never set it to a URL that is not published.

import PackageDescription

let releasedBinary: (url: String, checksum: String)? = nil

let swiftRoot = "runtimes/swift/KinetoRuntime"

let cRuntime: Target = {
    if let releasedBinary {
        return .binaryTarget(
            name: "KinetoRuntimeC",
            url: releasedBinary.url,
            checksum: releasedBinary.checksum
        )
    }
    return .binaryTarget(
        name: "KinetoRuntimeC",
        path: "\(swiftRoot)/Artifacts/KinetoRuntimeC.xcframework"
    )
}()

let package = Package(
    name: "KinetoRuntime",
    platforms: [
        .iOS(.v15)
    ],
    products: [
        .library(
            name: "KinetoRuntime",
            targets: ["KinetoRuntime"]
        ),
        // The raw C ABI, for hosts (such as the Flutter plugin) that own the
        // player handle themselves.
        .library(
            name: "KinetoRuntimeC",
            targets: ["KinetoRuntimeC"]
        )
    ],
    targets: [
        cRuntime,
        .target(
            name: "KinetoRuntime",
            dependencies: ["KinetoRuntimeC"],
            path: "\(swiftRoot)/Sources/KinetoRuntime",
            linkerSettings: [
                .linkedFramework("Foundation"),
                .linkedFramework("Metal"),
                .linkedFramework("QuartzCore"),
                .linkedFramework("UIKit")
            ]
        ),
        .testTarget(
            name: "KinetoRuntimeTests",
            dependencies: ["KinetoRuntime"],
            path: "\(swiftRoot)/Tests/KinetoRuntimeTests",
            resources: [
                .copy("Fixtures/pointer_binding.kvk"),
                .copy("Fixtures/background_half.kvk"),
                .copy("Fixtures/background_opaque.kvk"),
                .copy("Fixtures/background_off.kvk"),
                .copy("Fixtures/background_legacy.kvk")
            ]
        )
    ]
)
