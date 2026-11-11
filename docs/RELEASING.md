# Packaging and releasing

**Nothing has been released.** There is no tag, no uploaded binary and no
registry entry. This file describes how the distributable packages are built
and checked, and the steps a release would take. Every step that publishes
something is manual, needs the accounts listed at the end, and cannot be done
by CI.

| Package | Distribution form | Staged by | Staged at (ignored by Git) |
| --- | --- | --- | --- |
| Swift (`KinetoRuntime`, `KinetoRuntimeC`) | root `Package.swift` + `KinetoRuntimeC.xcframework.zip` binary target | `scripts/package_swift.sh` | `build/dist/swift/` |
| Flutter (`kineto_runtime`) | pub.dev package with the engine bundled for iOS, Android and web | `scripts/package_flutter.sh` | `build/dist/flutter/kineto_runtime/` |

Android (Maven) and npm packaging are out of scope here.

## Swift

### What is built

`scripts/package_swift.sh`:

1. builds the engine (`metal-render`) for `aarch64-apple-ios`,
   `aarch64-apple-ios-sim` and `x86_64-apple-ios` with
   `tool/refresh_runtime_xcframework.sh`, and lipos the two simulator
   architectures together;
2. checks every slice with `tool/verify_xcframework.sh`: architectures match
   `Info.plist`, every object is built for the slice's platform (iOS vs iOS
   simulator) and for iOS 15.0, every C ABI function is defined, the headers
   match `engine/kineto_engine/kineto_engine.h`;
3. stages a copy with debug symbols stripped (`strip -S`), checks it again,
   and adds `LICENSE`, `THIRD_PARTY_NOTICES.md` and `licenses/`;
4. writes `KinetoRuntimeC.xcframework.zip` with `tool/deterministic_zip.py`
   (sorted entries, fixed timestamps and permissions), its
   `swift package compute-checksum` value, `SHA256SUMS` and `build-info.json`.

Layout of the zip:

```
KinetoRuntimeC.xcframework/
  Info.plist
  ios-arm64/                    libkineto_engine.a      (arm64, iOS)
  ios-arm64_x86_64-simulator/   libkineto_engine_sim.a  (arm64 + x86_64, iOS simulator)
  */Headers/                    kineto_engine.h, module.modulemap
LICENSE
THIRD_PARTY_NOTICES.md
licenses/RUST_LICENSE_TEXTS.txt, licenses/rust-crates.tsv
```

### Reproducibility

Inputs are pinned: `engine/Cargo.lock` (`--locked`), Rust 1.95.0, iOS
deployment target 15.0. Absolute paths (checkout, cargo home, target
directory) are remapped out of the binaries, and `ZERO_AR_DATE=1` zeroes
archive timestamps. The target directory is remapped after the checkout
(rustc applies the last matching prefix), in both spellings cargo uses for it.
`tool/check_binary_paths.py` fails packaging if any bundled `.a`, `.so` or
`.wasm` still names a path on the build machine (the Android NDK linker's own
build-path ident is the one allowed exception).
`scripts/package_swift.sh --check-reproducible` rebuilds
everything in an empty target directory and fails unless the zip is
byte-identical. Builds on different Xcode versions have not been compared.

### Consumer checks

`tool/test_swift_consumer.sh` creates two packages in a temporary directory
and runs `xcodebuild build-for-testing` for `generic/platform=iOS` and
`generic/platform=iOS Simulator`:

- a `.package(path:)` dependency on the repository root, using the Swift API;
- a `.binaryTarget(path:)` on a copy of the zip, using the C ABI: the same
  bytes a `binaryTarget(url:checksum:)` resolves to after download.

It links the test bundles and checks the C ABI is in the linked binary. It
does not run anything.

### Release steps (not yet done)

1. From a clean checkout of the release commit:
   `scripts/package_swift.sh --check-reproducible`, then
   `tool/test_swift_consumer.sh`.
2. Create the tag and a release in the GitHub repository, and upload
   `build/dist/swift/KinetoRuntimeC.xcframework.zip` unchanged as a release
   asset. SwiftPM needs a semantic-version tag (`0.1.0`, or `v0.1.0`).
3. Commit the `releasedBinary` value in `Package.swift` with the asset's https
   download URL and the checksum (`scripts/package_swift.sh --asset-url <url>`
   prints the exact lines). The tag must point at a commit whose manifest has
   both: a tag created before step 3 resolves to the locally built path and
   will not link for consumers.
4. From a machine without this checkout, add
   `.package(url: "https://github.com/HBrinkman/kineto-runtime.git", exact: "<version>")`
   and run `tool/test_swift_consumer.sh`-style builds against it.

Until then, `.package(url:)` on this repository resolves but does not link:
the static libraries are not in Git. Use a clone and `.package(path:)`
(see `docs/BUILDING.md`).

## Flutter

### What is staged

`scripts/package_flutter.sh` (add `--build` to build the iOS and Android
libraries first) copies `packages/kineto_runtime` into
`build/dist/flutter/kineto_runtime` and makes it self-contained:

- the Kotlin sources linked from `runtimes/android` become real files;
- `ios/kineto_runtime/KinetoRuntimeC.xcframework` is the stripped, verified
  XCFramework from the Swift step. Both the plugin's `Package.swift` (Swift
  Package Manager) and `kineto_runtime.podspec` (CocoaPods) link it;
- `android/src/main/jniLibs/{arm64-v8a,x86_64}/libkineto_engine.so`, debug info
  stripped, after checking each is the right ELF machine and exports every JNI
  and C ABI symbol;
- `web/pkg` (wasm, glue, types), checked identical to `runtimes/web/pkg`;
- `LICENSE`, `THIRD_PARTY_NOTICES.md`, `licenses/` (Rust crate texts, Flutter
  BSD, Gradle Apache-2.0/NOTICE), `CHANGELOG.md`, `README.md`, `example/`;
- `BUILD_INFO.json`: source commit, toolchain and the sha256 of each binary;
- `pubspec.yaml` without `publish_to: none`.

It then runs `flutter pub get` and `dart pub publish --dry-run` in the staged
directory and keeps the output in `build/dist/flutter/pub-dry-run.txt`. The
dry run uploads nothing. The checkout's own `pubspec.yaml` keeps
`publish_to: none`, so publishing from the checkout is refused.

### Consumer checks

`tool/test_flutter_consumer.sh` runs `flutter create` in a temporary
directory, depends on a pristine copy of the staged package, and builds:
`web` (release), `apk` (debug), `ios-spm` and `ios-pods` (release, no code
signing, then `tool/check_runtime_symbols.sh` on `Runner`), and `ios-sim`
(debug simulator). Build only.

### Release steps (not yet done)

1. Bump `version` in `packages/kineto_runtime/pubspec.yaml` and
   `CHANGELOG.md`; commit.
2. `scripts/package_flutter.sh --build` and `tool/test_flutter_consumer.sh` on
   a clean checkout of that commit. Read `pub-dry-run.txt`.
3. Record device evidence for each platform (see
   `docs/PLATFORM_SUPPORT.md`): nothing has been run on hardware yet.
4. `cd build/dist/flutter/kineto_runtime && GIT_CEILING_DIRECTORIES="$PWD/.." dart pub publish`
   (without the variable, pub sees the staged copy as ignored by this
   repository's Git and finds no files). **A published
   pub.dev version cannot be deleted or replaced**; it can only be retracted
   within seven days. Publish from the staged directory only.

## Accounts and setup a release needs

- **GitHub:** write access to `HBrinkman/kineto-runtime` to push the tag and
  create the release; the Swift zip is a release asset.
- **pub.dev:** a Google account that owns the `kineto_runtime` package name
  (unclaimed until the first upload), ideally a verified publisher for the
  project's domain, which needs DNS verification of that domain.
- **Apple:** none for packaging. Static libraries are not code-signed; apps
  sign them with their own identity. Device testing needs a development team.
- **Android:** none for packaging (no Maven publication here).
- **CI:** `.github/workflows/packaging.yml` runs on the `macos-26` GitHub-hosted
  image (Xcode 26.6 by default). It has not run yet: nothing has been pushed.
