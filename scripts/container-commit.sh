#!/usr/bin/env bash

set -euo pipefail

git config user.name "nwn900"
git config user.email "nwn900@users.noreply.github.com"
git add README.md icon.ico main.js package-lock.json package.json assets docker scripts

if git diff --cached --quiet; then
  echo "Nothing to commit."
  exit 0
fi

git commit -m "${1:-Rebrand app for Grok}"
