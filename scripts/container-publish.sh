#!/usr/bin/env bash

set -euo pipefail

TOKEN_FILE="${1:?token file path is required}"

export GH_TOKEN="$(cat "$TOKEN_FILE")"

git config user.name "nwn900"
git config user.email "nwn900@users.noreply.github.com"

if git remote get-url origin >/dev/null 2>&1; then
  git remote rename origin upstream
fi

git add README.md icon.ico main.js package-lock.json package.json assets docker scripts
git commit -m "Rebrand app for Grok"
gh repo create nwn900/GrokWindowsApp --public --source . --remote origin --push
