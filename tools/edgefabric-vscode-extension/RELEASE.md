# Release Guide

## Local Package

1. Install dependencies:

```bash
npm install
```

2. Compile extension:

```bash
npm run compile
```

3. Build VSIX:

```bash
npm run package:vsix
```

4. Install in VS Code:

- Open Command Palette
- Run Extensions: Install from VSIX
- Select generated edgefabric-v<version>.vsix

## Marketplace Publish

1. Create publisher PAT with Marketplace publish scope.
2. Login to vsce:

```bash
npx vsce login veeringman
```

3. Publish:

```bash
npm run publish:vsce
```

## CI Packaging

Workflow file:

- .github/workflows/package-extension.yml

Run options:

- Manual trigger from Actions tab
- Push tag matching edgefabric-extension-v*
