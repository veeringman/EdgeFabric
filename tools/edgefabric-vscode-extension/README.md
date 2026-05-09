# EdgeFabric VS Code Extension

This extension lets developers authenticate with EdgeFabric, submit iOS, IoT, and Quantum build/deploy workloads, and inspect build logs directly from VS Code.

## Features

- Login and logout with EdgeFabric API key
- Submit IoT build + deploy jobs (ESP32C6/RPi workflows)
- Submit iOS build + deploy jobs on macOS runners
- Submit Quantum build + run jobs (virtual or real backend)
- Create complete iOS starter projects with selectable interface, bundle, team, and export options
- Build monitor tree in the EdgeFabric activity bar
- Open build logs in an output channel
- Download artifacts from completed builds
- Download all artifacts for a build in one step
- Filter build list by status, device, and source

## Commands

- `EdgeFabric: Login`
- `EdgeFabric: Logout`
- `EdgeFabric: Submit IoT Build + Deploy`
- `EdgeFabric: Submit iOS Build + Deploy`
- `EdgeFabric: Submit Quantum Build + Run`
- `EdgeFabric: Create iOS Starter Project`
- `EdgeFabric: Refresh Builds`
- `EdgeFabric: View Build Logs`
- `EdgeFabric: Download Build Artifact`
- `EdgeFabric: Download All Build Artifacts`
- `EdgeFabric: Set Build Filters`
- `EdgeFabric: Clear Build Filters`

## Configuration

- `edgefabric.apiBaseUrl` (default: `http://localhost:6060/api/v1`)
- `edgefabric.defaultRegion`
- `edgefabric.defaultSshUser` (default: current OS username, falls back to `builder`)

## Build

Prerequisites:

- Node.js 20+
- npm 10+

```bash
npm install
npm run compile
```

## Run In VS Code

1. Open this extension folder in VS Code.
2. Run `npm install`.
3. Press `F5` to launch Extension Development Host.
4. Execute `EdgeFabric: Login` from Command Palette.
5. Execute `EdgeFabric: Create iOS Starter Project` to scaffold a local Xcode project from the extension UI.

## End-To-End Flow

1. Login with `EdgeFabric: Login`.
2. Submit IoT workloads with `EdgeFabric: Submit IoT Build + Deploy`.
3. Submit iOS workloads with `EdgeFabric: Submit iOS Build + Deploy`.
4. Submit Quantum workloads with `EdgeFabric: Submit Quantum Build + Run`.
5. Create starter projects with `EdgeFabric: Create iOS Starter Project`.
6. Monitor statuses in the `EdgeFabric` Activity Bar under `Builds`.
7. Open per-build logs with `EdgeFabric: View Build Logs`.
8. Download artifacts with `EdgeFabric: Download Build Artifact`.
9. Use `EdgeFabric: Download All Build Artifacts` to export all artifacts into a selected folder.

## Artifact Picker UX

- Optional type filter before item selection
- Sorted by size (largest first)
- Type-aware icons and human-readable sizes

## Package VSIX

```bash
npm run package:vsix
```

This produces a versioned `.vsix` file in the extension root, which can be installed from VS Code using Extensions: Install from VSIX.
