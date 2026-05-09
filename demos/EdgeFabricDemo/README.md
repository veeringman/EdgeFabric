# EdgeFabricDemo

Minimal SwiftUI iOS app used to validate local simulator build and deployment flows through EdgeFabric.

## Build, Install, Launch

```bash
./scripts/run-ios-demo.sh
```

Pass a simulator UDID explicitly if you do not want to use the currently booted one:

```bash
./scripts/run-ios-demo.sh <simulator-udid>
```

## Build Locally

```bash
cd demos/EdgeFabricDemo
xcodebuild -project EdgeFabricDemo.xcodeproj -scheme EdgeFabricDemo -destination 'id=<simulator-udid>' build
```