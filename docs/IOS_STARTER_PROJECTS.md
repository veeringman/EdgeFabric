# iOS Starter Projects

EdgeFabric can now generate a complete local iOS starter project scaffold from the repository using:

```bash
./scripts/create-ios-starter.sh --name EdgeStarter
```

## Available Options

- `--name`: app display name and project seed
- `--organization-id`: reverse-DNS prefix used for default bundle identifiers
- `--bundle-id`: explicit bundle identifier override
- `--team-id`: Apple Developer Team ID for automatic signing assignment
- `--interface`: `swiftui` or `uikit`
- `--deployment-target`: iOS deployment target version
- `--output-dir`: destination root for generated projects
- `--archive`: export a zip archive alongside the generated project

## Example

```bash
./scripts/create-ios-starter.sh \
  --name FieldOps \
  --organization-id com.edgefabric.samples \
  --interface swiftui \
  --team-id ABC123XYZ9 \
  --archive
```

That command produces:

- a complete Xcode project under `generated-ios/FieldOps`
- a shared scheme ready for Xcode or VS Code workflows
- developer team assignment baked into build settings when `--team-id` is provided
- an optional zip archive users can move or download elsewhere

## Current Scope

This is the foundation for dashboard and VS Code driven starter-project creation. The current implementation is script-based and produces a complete scaffold locally. The next step would be exposing these options through the API, dashboard, and extension UI.