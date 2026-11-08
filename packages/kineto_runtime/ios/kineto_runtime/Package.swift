// swift-tools-version: 5.10

import PackageDescription

// Every `#[no_mangle] pub extern "C"` in engine/kineto_engine/src/ffi/c.rs
// except the Android-only surface constructor, which is not in the iOS slice.
//
// dart:ffi resolves these through `DynamicLibrary.process()`, which the static
// linker cannot see, so release builds dead-strip anything not named here.
// Keep in sync with the podspec and tool/check_runtime_symbols.sh.
let ffiLinkRoots = [
    "kineto_active_audio_clips",
    "kineto_advance",
    "kineto_animations",
    "kineto_audio_assets",
    "kineto_audio_events",
    "kineto_destroy",
    "kineto_override_scalar",
    "kineto_retain",
    "kineto_get_active_state",
    "kineto_get_active_transition",
    "kineto_get_transition_progress",
    "kineto_hit_test_document_point",
    "kineto_last_error_message",
    "kineto_load",
    "kineto_machine_inputs",
    "kineto_pause",
    "kineto_play",
    "kineto_document_point",
    "kineto_background_color",
    "kineto_pointer_event",
    "kineto_pointer_moved",
    "kineto_render",
    "kineto_select_animation",
    "kineto_set_bool",
    "kineto_set_number",
    "kineto_set_trigger",
    "kineto_state_machines",
    "kineto_surface_create_from_ca_metal_layer",
    "kineto_surface_destroy",
    "kineto_surface_last_error_message",
    "kineto_surface_resize",
    "kineto_take_fired_audio_events",
]

let package = Package(
    name: "kineto_runtime",
    platforms: [
        // Must not exceed what consumes this package. Flutter generates
        // `FlutterGeneratedPluginSwiftPackage` from Runner's
        // IPHONEOS_DEPLOYMENT_TARGET, and a *stale* copy of that generated
        // file — it is gitignored and only rewritten when Flutter reruns —
        // stopped a device build at package resolution, before anything
        // compiled:
        //
        //   kineto-runtime requires minimum platform version 18.0 for iOS,
        //   but FlutterGeneratedPluginSwiftPackage and Runner support 15.0
        //
        // Declaring the minimum this target actually needs, rather than
        // mirroring the app's, makes that class of mismatch impossible in one
        // direction: nothing here has an availability guard, and the
        // KinetoRuntime package it wraps already targets v15.
        //
        // The app's own IPHONEOS_DEPLOYMENT_TARGET stays 18.0. A package with
        // a lower minimum is consumable by a target with a higher one, so
        // which devices the app runs on does not change.
        .iOS("15.0")
    ],
    products: [
        .library(
            name: "kineto-runtime",
            targets: ["kineto_runtime"]
        )
    ],
    dependencies: [
        .package(
            name: "FlutterFramework",
            path: "../FlutterFramework"
        ),
        .package(
            path: "KinetoRuntime"
        )
    ],
    targets: [
        .target(
            name: "kineto_runtime",
            dependencies: [
                .product(
                    name: "FlutterFramework",
                    package: "FlutterFramework"
                ),
                .product(
                    name: "KinetoRuntimeC",
                    package: "KinetoRuntime"
                )
            ],
            linkerSettings: ffiLinkRoots.flatMap { symbol in
                [.unsafeFlags(["-Xlinker", "-u", "-Xlinker", "_\(symbol)"])]
            }
        )
    ]
)
