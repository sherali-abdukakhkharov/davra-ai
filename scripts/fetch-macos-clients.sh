#!/usr/bin/env bash
# Takes the macOS builds of a GitHub Release (made on a Mac with
# scripts/build-macos.sh --upload; see docs/build-macos.md) into the hub:
#   - the davra CLI into deploy/clients/, which the hub image serves for install.sh;
#   - Davra Desktop (.dmg, and the signed .app update bundle) into desktop/dist/publish/,
#     which the hub serves under /desktop, with latest.json rewritten.
# Then rebuild the hub (needed for the CLI; Desktop files are served live):
#   scripts/fetch-macos-clients.sh v0.2.4
#   docker compose -f deploy/docker-compose.yml --profile tunnel-config up -d --build hub
set -euo pipefail
tag=${1:?usage: scripts/fetch-macos-clients.sh <tag>}
cd "$(dirname "$0")/.."
check() { if command -v sha256sum >/dev/null; then sha256sum -c --ignore-missing "$@"; else shasum -a 256 -c --ignore-missing "$@"; fi; }

# Inside the repo: a native gh.exe (Windows) cannot see Git Bash's /tmp.
tmp=".fetch-macos.$$"
mkdir "$tmp"
trap 'rm -rf "$tmp"' EXIT
gh release download "$tag" -D "$tmp" -p 'davra-macos-*' -p 'Davra_*_universal.*' -p SHA256SUMS-macos
(cd "$tmp" && check SHA256SUMS-macos)

shopt -s nullglob
cli=("$tmp"/davra-macos-*)
if [[ ${#cli[@]} -gt 0 ]]; then
  mkdir -p deploy/clients
  cp "${cli[@]}" deploy/clients/
  ls -l deploy/clients/davra-macos-*
fi

desktop=("$tmp"/Davra_*_universal.*)
if [[ ${#desktop[@]} -gt 0 ]]; then
  pub=desktop/dist/publish
  mkdir -p "$pub"
  # One macOS version at a time; Windows and Linux files stay.
  rm -f "$pub"/Davra_*_universal.*
  cp "${desktop[@]}" "$pub/"
  scripts/desktop-manifest.sh "$pub"
fi
