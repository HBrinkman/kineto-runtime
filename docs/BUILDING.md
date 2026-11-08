# Building and installing from source

Nothing in this repository is published to a package registry. There is no
CocoaPods trunk, Swift Package release, Maven, npm or pub.dev package, and no
release tag. Every integration below builds from a clone.

Use a specific commit SHA when you depend on this repository; `main` moves.

## Prerequisites

| Tool | Version tested | Needed for |
| --- | --- | --- |
| Rust (`rustup`, `cargo`) | 1.95.0 | everything |
| `flatc` | exactly **25.12.19** (`flatbuffers-build` refuses others) | everything: `build.rs` compiles `schema/kineto_runtime.fbs` |
| Python 3 | 3.10+ | scripts and checks |
| Node.js | 24+ | web parity runner |
| `wasm-bindgen-cli` | exactly **0.2.122** (matches `engine/Cargo.lock`) | rebuilding the web package only |
| Flutter | 3.47.2 (pubspec floor 3.44) | the Flutter package |
| Xcode | Swift 5.10+ (tested with Xcode 27.1); Rust targets `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios` | iOS |
| Android SDK 36, NDK (r29 tested by the build script), JDK 17; Rust targets `aarch64-linux-android`, `x86_64-linux-android` | | Android |

`flatc` can come from your package manager if the version matches, or from the
[v25.12.19 release](https://github.com/google/flatbuffers/releases/tag/v25.12.19).
Point `FLATC_PATH` at it if it is not on `PATH`.

```sh
scripts/bootstrap.sh check   # what is installed
scripts/bootstrap.sh host    # build the engine for this machine
scripts/bootstrap.sh test    # every host-side check (what CI runs)
```

## Where build output goes

Cargo run from `engine/` writes to `<repo>/build/cargo` (set in
`engine/.cargo/config.toml`; `CARGO_TARGET_DIR` overrides it). Cargo run
elsewhere with `--manifest-path engine/Cargo.toml` uses `engine/target`. The
scripts ask cargo where it wrote rather than assuming either.

## Generated files

| File | Committed? | Regenerate with |
| --- | --- | --- |
| `engine/kineto_engine/kineto_engine.h` | yes | any cargo build (`build.rs`, cbindgen) |
| Rust FlatBuffers bindings | no, in cargo's `OUT_DIR` | any cargo build |
| `runtimes/web/pkg/*`, mirrored to `packages/kineto_runtime/web/pkg/*` | yes | `scripts/bootstrap.sh web` |
| `KinetoRuntimeC.xcframework/*/*.a` (iOS) | **no** | `scripts/bootstrap.sh ios` |
| `**/src/main/jniLibs/*/libkineto_engine.so` (Android) | **no** | `scripts/bootstrap.sh android` |
| Host `libkineto_engine.dylib` / `.so` | no | `scripts/bootstrap.sh host` |

The native libraries are not committed: each rebuild produces different bytes
(about 45 MB device / 88 MB simulator for iOS), and committed copies go stale
silently. The iOS headers, module map and `Info.plist` are committed.

## Swift Package (iOS)

`runtimes/swift/KinetoRuntime` is a local Swift package. It is **not usable as
a remote SwiftPM URL dependency**: its `Package.swift` is in a subdirectory, not
at the repository root, and its binary target points at libraries that are
built locally and ignored by Git. There is deliberately no root `Package.swift`
and no `binaryTarget(url:)` until binaries are published with checksums.

```sh
git clone https://github.com/HBrinkman/kineto-runtime.git
cd kineto-runtime
git checkout <commit-sha>
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
scripts/bootstrap.sh ios
```

Then add the local package: in Xcode, *File > Add Package Dependencies… > Add
Local…* and choose `runtimes/swift/KinetoRuntime`; or from another package:

```swift
.package(path: "../kineto-runtime/runtimes/swift/KinetoRuntime")
```

Platform: iOS 15+ (package manifest). No macOS, tvOS, visionOS or Mac Catalyst
slice is built.

## Flutter package

`packages/kineto_runtime` (`publish_to: none`). It contains symlinks into
`runtimes/` (the Kotlin sources and the Swift package), so it must be used from
a full checkout of this repository.

**Recommended:** a path dependency on a clone where you have built the native
libraries for your targets:

```yaml
dependencies:
  kineto_runtime:
    path: ../kineto-runtime/packages/kineto_runtime
```

**Git dependency:** pub can fetch the package from Git:

```yaml
dependencies:
  kineto_runtime:
    git:
      url: https://github.com/HBrinkman/kineto-runtime.git
      path: packages/kineto_runtime
      ref: <commit-sha>
```

That gives you the Dart API, and the committed web runtime. It does **not**
give you iOS or Android native libraries: they are not committed, and pub's
checkout in its cache has not been built. For iOS or Android, use the path
dependency above.

Host (macOS/Linux) Dart tests load the engine through `dart:ffi`. Run
`scripts/bootstrap.sh host`; the loader finds `build/cargo/debug` or
`engine/target/debug`, or set `KINETO_ENGINE_DYLIB` to the library path.

## Android (Kotlin)

`runtimes/android/KinetoRuntime` is a Gradle library module, not a Maven
artifact.

```sh
export ANDROID_NDK_HOME=/path/to/ndk
scripts/bootstrap.sh android     # builds arm64-v8a and x86_64
```

```kotlin
// settings.gradle.kts
include(":KinetoRuntime")
project(":KinetoRuntime").projectDir = file("<clone>/runtimes/android/KinetoRuntime")

// app/build.gradle.kts
dependencies { implementation(project(":KinetoRuntime")) }
```

`minSdk` 24, compile SDK 36, Java 17. `armeabi-v7a` is not built. The module's
`preBuild` fails with a pointer to the script if the libraries are missing.

## Web

`runtimes/web` is a private ES module package (`"private": true`, not on
npm). The wasm and its TypeScript types are committed under `runtimes/web/pkg`.

```sh
cd runtimes/web && node ../parity/run_wasm_parity.mjs   # check the committed wasm
```

Consume it by copying `runtimes/web/pkg` and `runtimes/web/src` from a known
commit, or with a file dependency on a clone:

```json
{ "dependencies": { "@kineto/runtime": "file:../kineto-runtime/runtimes/web" } }
```

Rendering on the web is a Canvas 2D fallback; see `runtimes/web/README.md`
for the API and what the web runtime does not support yet.

To rebuild the wasm after changing `engine/kineto_engine/src/wasm.rs`:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122
scripts/bootstrap.sh web
```
