// swift-tools-version: 5.10

import PackageDescription

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
        // The Flutter wrapper owns its player handle in Dart and therefore
        // calls the C ABI directly. Keep this product separate from the
        // high-level native Swift API.
        .library(
            name: "KinetoRuntimeC",
            targets: ["KinetoRuntimeC"]
        )
    ],
    targets: [
        .binaryTarget(
            name: "KinetoRuntimeC",
            path: "Artifacts/KinetoRuntimeC.xcframework"
        ),
        .target(
            name: "KinetoRuntime",
            dependencies: ["KinetoRuntimeC"],
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
            // A real `.kvk`, so the introspection tests parse what the engine
            // actually emits rather than a hand-written string (issue #111).
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
