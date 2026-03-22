#!/usr/bin/env bash

set -euo pipefail

export HOME="${HOME:-/tmp/home}"
export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-/tmp/npm-cache}"
export ELECTRON_CACHE=/tmp/electron-cache
export ELECTRON_BUILDER_CACHE=/tmp/electron-builder-cache

mkdir -p "$HOME" "$NPM_CONFIG_CACHE" "$ELECTRON_CACHE" "$ELECTRON_BUILDER_CACHE"

case "${1:-all}" in
  install)
    npm ci
    npm run audit:branding
    ;;
  smoke)
    timeout 180s xvfb-run -a env ELECTRON_DISABLE_SANDBOX=1 SMOKE_TEST=1 npm start
    ;;
  build)
    npm run build
    ;;
  all)
    npm ci
    npm run audit:branding
    timeout 180s xvfb-run -a env ELECTRON_DISABLE_SANDBOX=1 SMOKE_TEST=1 npm start
    npm run build
    ;;
  *)
    echo "Unknown step: $1" >&2
    exit 1
    ;;
esac
