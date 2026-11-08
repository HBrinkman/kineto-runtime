# Kineto Runtime Example

Minimal Flutter sample for the Kineto runtime package.

The app loads `assets/sample.kvk` through `KinetoPlayer.loadAsset` and mounts it
with `KinetoPlayer.asWidget()`.

Current platform behavior:

- iOS: `UiKitView` -> `CAMetalLayer` -> Rust/vello/wgpu Metal.
- Android: `AndroidView` -> the native runtime's `KinetoPlayerView` -> `SurfaceView`
  -> Rust/vello/wgpu. Build the engine first: `scripts/build_runtime_android.sh`.
- Web: Flutter `HtmlElementView` canvas -> wasm-bindgen Rust player ->
  Canvas2D render fallback.

Touch is read on the Flutter side and shared by Android and iOS. What is proven on
which platform: `docs/PLATFORM_SUPPORT.md` at the repo root.

For direct bytes, prefer `await KinetoPlayer.loadBytesAsync(bytes)` in
cross-platform code. Native code can still use sync `KinetoPlayer.loadBytes`.

Run on a connected device, simulator, emulator, or Chrome:

```sh
flutter run -d <device-id>
```

Android checks, on an emulator only:

```sh
flutter test integration_test/runtime_device_test.dart -d emulator-5554
ANDROID_SERIAL=emulator-5554 ../../../scripts/runtime_android_flutter_smoke.sh
```
