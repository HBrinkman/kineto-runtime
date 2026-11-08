# KinetoRuntime

Swift Package wrapper for the M7 Rust runtime.

```swift
import KinetoRuntime

let player = try KinetoPlayer(contentsOf: kvkURL)
let controller = KinetoPlayerViewController(player: player)
```

The package links `KinetoRuntimeC.xcframework`, generated from
`engine/kineto_engine` with `metal-render` enabled for iOS device and
simulator. Its Headers and `Info.plist` are committed; its two `.a` libs are
not (they're rebuilt bytes-differ-every-time binaries — see
`docs/BUILDING.md` at the repo root), so run this once before the package will link:

```sh
tool/refresh_runtime_xcframework.sh   # from the repo root
```
