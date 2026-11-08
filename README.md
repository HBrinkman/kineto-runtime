# Kineto Runtime

Playback engine and integration SDKs for the **Kineto animation runtime**.

Kineto is an iPad-first animation editor. It exports runtime documents
(`.kvk`) that this runtime loads, animates, runs state machines for and
renders inside a host application. The editor is a separate, private product;
this repository contains only the runtime.

## Status

**Pre-release, source only.** No version is released or tagged, and nothing
is published to CocoaPods, Swift Package registries, Maven, npm or pub.dev.
Use a pinned commit SHA and build from source. Interfaces may still change.

No platform has been tested on physical hardware. See
[docs/PLATFORM_SUPPORT.md](docs/PLATFORM_SUPPORT.md) for what is verified, and
how.

## What is here

| Path | What |
| --- | --- |
| `engine/kineto_engine` | Rust engine: document loading, animation, bones/IK, constraints, state machines, listeners, audio events, vello rendering; C ABI, JNI and wasm bindings |
| `schema/kineto_runtime.fbs` | `.kvk` document format (FlatBuffers) |
| `runtimes/swift/KinetoRuntime` | Swift package for iOS (Metal) |
| `runtimes/android` | Kotlin library for Android, and a sample host |
| `runtimes/web` | ES module + wasm (Canvas 2D rendering) |
| `packages/kineto_runtime` | Flutter plugin (iOS, Android, web) and example app |
| `runtimes/parity` | fixture table every runtime is checked against |

## Getting started

Prerequisites, per-platform builds and installation:
**[docs/BUILDING.md](docs/BUILDING.md)**.

```sh
git clone https://github.com/HBrinkman/kineto-runtime.git
cd kineto-runtime
scripts/bootstrap.sh check   # which prerequisites are installed
scripts/bootstrap.sh test    # engine, web and Flutter host checks
```

Install paths, in short:

- **iOS (Swift):** build the libraries (`scripts/bootstrap.sh ios`), then add
  `runtimes/swift/KinetoRuntime` as a *local* package. It cannot be added by
  repository URL: the manifest is in a subdirectory and the binaries are built
  locally.
- **Android:** build the libraries (`scripts/bootstrap.sh android`), then
  include `runtimes/android/KinetoRuntime` as a Gradle module.
- **Flutter:** path dependency on `packages/kineto_runtime` in a clone with
  native libraries built. A Git dependency (`path: packages/kineto_runtime`)
  works for the Dart API and web only.
- **Web:** the committed `runtimes/web` package, via a file dependency or a
  copy.

## License

The runtime source is MIT-licensed ([LICENSE](LICENSE)). Third-party
components keep their own licenses: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
Where this code came from: [PROVENANCE.md](PROVENANCE.md).

Contributions: [CONTRIBUTING.md](CONTRIBUTING.md). Security reports:
[SECURITY.md](SECURITY.md).

## Documentation

- [Kineto website](https://Kineto.keyvotion.com)
- [Kineto documentation](https://Kineto.keyvotion.com/docs/)

## Contact

[hello@keyvotion.com](mailto:hello@keyvotion.com)
