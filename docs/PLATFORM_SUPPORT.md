# Platform support

**Status: pre-release.** No version has been released or tagged, no package is
published, and no platform has been tested on physical hardware.

## Evidence levels

| Level | Meaning |
| --- | --- |
| **CI** | Run by the `runtime-tests` workflow on GitHub-hosted Ubuntu. The workflow is in place but has not run yet: nothing has been pushed. |
| **Host** | Ran on a macOS host through the C ABI / wasm / Dart FFI. |
| **Compile** | Builds. Says nothing about behaviour. |
| **Package** | The staged distributable (`scripts/package_swift.sh`, `scripts/package_flutter.sh`) was built, verified and consumed by a new, throwaway app or package that compiled and linked against it (`tool/test_swift_consumer.sh`, `tool/test_flutter_consumer.sh`). Build only: nothing was launched. |
| **Upstream emulator / simulator** | Recorded by the private source repository at the extraction commit on an Android 16 (API 36) arm64 emulator or the iOS simulator. Not re-run in this repository. |
| **Device** | Physical hardware. **None.** |

The iOS simulator cannot draw with this renderer (it lacks the indirect
execution vello needs), so a simulator run proves logic, touch and lifecycle,
not pixels. An emulator render says nothing about Adreno, Mali or other real
GPU drivers.

## Matrix

| Target | Package | Rendering | Evidence | Install |
| --- | --- | --- | --- | --- |
| Engine (Rust) | `engine/kineto_engine` | vello scene building (`vello-render`) | Host; CI workflow | source |
| Web | `runtimes/web` | Canvas 2D fallback (no WebGPU yet) | Host (wasm parity in Node); `examples/web` plays and draws in headless Chromium with no console errors; CI workflow | committed wasm, file dependency |
| Flutter web | `packages/kineto_runtime` | Canvas 2D via the web runtime | Package: a new app on the staged package builds for web with the wasm bundled unchanged. Rendering in a browser not checked through Flutter here (`runtimes/parity/run_flutter_web_smoke.mjs` not run); the plain web example draws in headless Chromium | path or git dependency; staged pub package |
| iOS (Swift) | `Package.swift` (root), `runtimes/swift/KinetoRuntime` | Metal via wgpu/vello | Package: the distribution zip and the root package link into new consumer packages for device and simulator; the package's own tests and `examples/ios-swiftui` build for both. XCTests not run here (CI runs them on a simulator). Upstream simulator for logic | local Swift package after `scripts/bootstrap.sh ios`; zip binary target after a release |
| Flutter iOS | `packages/kineto_runtime` | Metal platform view | Package: a new app on the staged package builds for device (release, Swift Package Manager and CocoaPods; all FFI symbols linked) and simulator. Upstream simulator for logic; rendering **not proven** | path dependency on a built clone; staged pub package |
| Android (Kotlin) | `runtimes/android/KinetoRuntime` | Vulkan/GLES via wgpu/vello | Compile: library (AAR) and `sample` APK build. Upstream emulator | Gradle module after `scripts/bootstrap.sh android` |
| Flutter Android | `packages/kineto_runtime` | native `SurfaceView` | Package: a new app on the staged package builds an APK with both ABIs. Upstream emulator | path dependency on a built clone; staged pub package |
| Flutter desktop (macOS/Linux) | `packages/kineto_runtime` | none: FFI logic only, `unsupported` for views | Host (macOS) Dart tests; CI workflow (Linux) | path dependency |
| Windows, tvOS, visionOS, Mac Catalyst, macOS native | — | — | none | not supported |

Android ABIs: `arm64-v8a` and `x86_64` only. iOS: device `arm64`, simulator
`arm64` + `x86_64`.

## Known limits

- Audio: a `.kvk` carries no audio bytes. Every runtime reports which sound
  fired; the iOS Swift package includes a player for it (`KinetoAudioDirector`),
  Android, Flutter and web leave playback to the host.
- Android: a transparent document shows black (`SurfaceView` is opaque). The
  first surface compiles GPU pipelines on the main thread; the cost on real
  devices is unmeasured.
- Web: no `pointerMoved`, state-machine/input listing, `selectAnimation` or
  `pause` yet (see `runtimes/web/README.md`).
- A view on any platform reports a renderer failure rather than crashing; there
  is no fallback renderer.
- The staged Swift zip and Flutter package carry the Rust crates' license
  texts next to the binaries. An app's in-app license screen does not pick
  them up automatically; the app must include them (see the package README).
