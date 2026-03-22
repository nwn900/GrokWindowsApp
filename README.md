# Grok Windows App

A lightweight Windows desktop wrapper for Grok.

## Features

- Direct access to `https://grok.com/`
- Single-instance app with tray access
- Minimize-to-tray behavior with optional launch at Windows startup
- Strict in-app navigation allowlist for Grok, X, Google, and Microsoft auth flows
- Windows NSIS packaging through Electron Builder

## Development

This project is meant to be built from a disposable Docker container so the host machine stays clean.

### Container Builder Image

Build the reusable builder image:

```bash
docker build -t grok-electron-builder -f docker/builder.Dockerfile docker
```

### Clone And Build In A Disposable Container

```bash
docker run --rm -it \
  -v "$PWD:/workspace" \
  -w /workspace \
  grok-electron-builder \
  bash -lc "npm ci && npm run audit:branding && npm run build"
```

### Smoke Test

```bash
docker run --rm -it \
  -v "$PWD:/workspace" \
  -w /workspace \
  grok-electron-builder \
  bash -lc "npm ci && xvfb-run -a env ELECTRON_DISABLE_SANDBOX=1 SMOKE_TEST=1 npm start"
```

The smoke test exits automatically after the main window loads `grok.com`.

## Output

`npm run build` produces the Windows installer in `dist/`.

## License

ISC
