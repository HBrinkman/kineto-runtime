# kineto_runtime

Flutter player for **Kineto** runtime documents (`.kvk`): loads a document,
runs its animations and state machines, and draws it in a platform view.
The engine is written in Rust; this package calls its C ABI through
`dart:ffi` on iOS and Android, and a wasm build of it on the web.

**Pre-release.** The API may still change. See the
[platform support matrix](https://github.com/HBrinkman/kineto-runtime/blob/main/docs/PLATFORM_SUPPORT.md)
for what has been verified on which platform, and how. No platform has been
tested on physical hardware yet.

## Platforms

| Platform | View | Engine delivery |
| --- | --- | --- |
| iOS 15+ | `UiKitView` drawing with Metal | `ios/kineto_runtime/KinetoRuntimeC.xcframework` (device arm64; simulator arm64 + x86_64), linked statically. Swift Package Manager and CocoaPods both work |
| Android (minSdk 24) | `AndroidView` with a `SurfaceView` | `android/src/main/jniLibs/{arm64-v8a,x86_64}/libkineto_engine.so`. No `armeabi-v7a` |
| Web | `HtmlElementView` canvas, Canvas 2D | `web/pkg/kineto_engine_bg.wasm`, loaded as a package asset |
| macOS / Linux desktop | none (`unsupported` view) | not bundled; FFI logic only, with a library you build |

The prebuilt libraries are part of the published package: no Rust toolchain
is needed to use it. They are built from the repository commit named in the
release notes with `scripts/package_flutter.sh`.

## Install

```yaml
dependencies:
  kineto_runtime: ^0.1.0
```

Until a version is published, depend on a built checkout instead; see
[Building and installing from source](https://github.com/HBrinkman/kineto-runtime/blob/main/docs/BUILDING.md).

## Use

```dart
import 'package:kineto_runtime/kineto_runtime.dart';

final player = await KinetoPlayer.loadAsset('assets/hero.kvk');

// In a build method:
player.asWidget();

// Drive a state machine:
for (final machine in player.stateMachines()) {
  for (final input in player.inputs(machine.id)) {
    debugPrint('${machine.name}.${input.name}');
  }
}
player.setBool('machine-id', 'isHovered', true);
player.setTrigger('machine-id', 'tap');

// When done:
player.dispose();
```

`KinetoPlayer.loadBytesAsync(bytes)` loads from memory on every platform.
The `example/` app plays a bundled document.

## Known limits

- The iOS simulator cannot draw with this renderer (it lacks the indirect
  execution vello needs): logic, touch and lifecycle run, pixels do not.
- Android: a transparent document shows black (`SurfaceView` is opaque).
- Web: rendering is a Canvas 2D fallback; `pointerMoved`, state-machine and
  input listing, `selectAnimation` and `pause` are not implemented yet.
- Audio: a `.kvk` carries no audio bytes. The player reports which sound
  fired; playing it is up to the app.

## Licenses

The package source is MIT (`LICENSE`). The bundled native and wasm libraries
statically link third-party Rust crates under permissive licenses; their
license and notice texts are in `licenses/RUST_LICENSE_TEXTS.txt`, listed in
`THIRD_PARTY_NOTICES.md`. **If you ship an app with this package, include
those notices** with the app's other open-source notices.
