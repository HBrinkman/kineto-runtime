# Example apps

| Platform | Where | Uses | Build |
| --- | --- | --- | --- |
| iOS (SwiftUI) | [`examples/ios-swiftui`](ios-swiftui) | root `Package.swift` as a local package, `KinetoPlayerViewController` | `scripts/bootstrap.sh ios`, then open `KinetoExample.xcodeproj` |
| Web | [`examples/web`](web) | `runtimes/web` ES module, Canvas 2D | `node examples/web/serve.mjs` |
| Android (Kotlin) | [`runtimes/android/sample`](../runtimes/android/sample) | `KinetoRuntime` Gradle module, `KinetoPlayerView` | `scripts/bootstrap.sh android`, then `cd runtimes/android && ./gradlew :sample:assembleDebug` |
| Flutter (iOS, Android, web) | [`packages/kineto_runtime/example`](../packages/kineto_runtime/example) | `kineto_runtime` by path | native libraries as above, then `flutter run` |

Every example plays `packages/kineto_runtime/example/assets/sample.kvk`, a
synthetic test document. None contains signing settings, a development team
or app icons; set your own before running on a device.

CI builds all four on every pull request (`.github/workflows/packaging.yml`,
`.github/workflows/runtime-tests.yml`); the web example is also loaded in
headless Chromium. Nothing is run on a device.
