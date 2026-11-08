# Kineto Runtime for Android

Plays an exported `.kvk` in a native Android app. The Rust engine evaluates and
renders; this library is the Kotlin surface over it.

**Status:** tested on an Android 16 (API 36) arm64 emulator using the Mac's GPU. Not
yet tested on a physical device, and not yet published. See
[`docs/PLATFORM_SUPPORT.md`](../../docs/PLATFORM_SUPPORT.md)
for what is and is not proven.

## Add it to an app

No Maven release exists yet. Build the engine, then include the module:

```bash
scripts/build_runtime_android.sh   # from the repo root; needs Rust and the NDK
```

```kotlin
// settings.gradle.kts
include(":KinetoRuntime")
project(":KinetoRuntime").projectDir = file("<repo>/runtimes/android/KinetoRuntime")

// app/build.gradle.kts
dependencies { implementation(project(":KinetoRuntime")) }
```

Requires `minSdk` 24, compile SDK 36, AGP 9, and Java 17 bytecode. Ships
`arm64-v8a` and `x86_64`. `armeabi-v7a` is not built.

`runtimes/android/sample` is a complete host using only the public API.

## Use it

```kotlin
val player = KinetoPlayer.loadAsset(context, "button.kvk")
val view = KinetoPlayerView(context).apply { this.player = player }

player.setTrigger("machine-button", "press")
player.setBool("machine-button", "pressed", true)
player.setNumber("machine-button", "pointerX", 0.5)
val state = player.activeState("machine-button")

// When the host is done: the view does not own the player.
view.player = null
player.close()
```

Everything throws `KinetoRuntimeException`, with a `status` and the engine's message,
for a document that will not load, an unknown name or a closed player.

`KinetoPlayer` may be called from any thread. `KinetoPlayerView` is main-thread only,
like any view.

## Touch

`KinetoPlayerView` delivers touch itself: the engine maps the point through the fit
and camera it renders with, and whatever listens there drives its input. To drive a
player without the view, call `documentPoint`, then `pointerEvent` or `pointerMoved`.

## Audio

A `.kvk` names its audio and carries none of it, and this runtime does not make a
sound. The engine reports; the app plays.

```kotlin
view.onFrame = {
    for (id in player.takeFiredAudioEvents()) playSoundFor(id)   // one-shots
    syncTimeline(player.activeAudioClips())                      // the clip's track
}
view.onPointerEvent = { _, _ ->
    for (id in player.takeFiredAudioEvents()) playSoundFor(id)   // a tap fires too
}
```

`audioEvents()` and `audioAssets()` list what the document names. iOS has a ready-made
player for this (`KinetoAudioDirector`). Android does not yet.

## When something fails

A failure stops playback and is shown over the surface and in `view.lastError`. The
app keeps running. Engine details go to logcat under the tag `kineto`.

A device whose Vulkan driver cannot run the renderer's shaders reports this way
rather than crashing. There is no fallback renderer.

## Known limits

- A transparent document shows black: `SurfaceView` is opaque here.
- The first surface builds its GPU pipelines on the main thread. The cost on real
  devices is unmeasured.
- No stale-library check: rerun `build_runtime_android.sh` after engine changes.
