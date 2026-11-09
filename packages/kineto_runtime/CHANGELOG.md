# Changelog

## 0.1.0-dev.1 (unreleased)

First development pre-release, for testing on real projects and devices.

- Dart API over the Kineto engine C ABI (`KinetoPlayer`): load `.kvk`
  documents, advance, play/pause, select animations, state-machine inputs
  (bool, number, trigger), pointer events and hit testing, audio event
  reporting, diagnostics.
- `KinetoPlayer.asWidget()`: iOS `UiKitView` (Metal), Android `AndroidView`
  (`SurfaceView`), web `HtmlElementView` (Canvas 2D fallback).
- Published package bundles the prebuilt engine: `KinetoRuntimeC.xcframework`
  (iOS device arm64, simulator arm64 + x86_64), `libkineto_engine.so`
  (Android arm64-v8a, x86_64) and the wasm web runtime, with their
  third-party license texts.
