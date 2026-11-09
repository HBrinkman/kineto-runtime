# iOS example (SwiftUI)

A SwiftUI app that plays `sample.kvk` with `KinetoPlayerViewController` and
lists the document's state machines and inputs. It depends on the repository's
root `Package.swift` as a local package (`../..`), and bundles
`packages/kineto_runtime/example/assets/sample.kvk` by reference.

```sh
scripts/bootstrap.sh ios                 # from the repository root, once
open examples/ios-swiftui/KinetoExample.xcodeproj
```

Pick your own team under *Signing & Capabilities* to run it on a device. The
iOS simulator runs the app but cannot draw with this renderer (it lacks the
indirect execution vello needs), so the player area stays empty there.

Command-line build, no signing:

```sh
cd examples/ios-swiftui
xcodebuild build -project KinetoExample.xcodeproj -scheme KinetoExample \
  -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO
```

iOS 17+ (the app); the runtime package itself supports iOS 15+.
