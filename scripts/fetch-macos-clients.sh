#!/usr/bin/env bash
# Downloads the macOS davra binaries of a GitHub Release (built on a Mac with
# scripts/build-macos.sh --upload; see docs/build-macos.md) into deploy/clients/,
# where the hub image picks them up for install.sh. Then rebuild the hub:
#   scripts/fetch-macos-clients.sh v0.2.4
#   docker compose -f deploy/docker-compose.yml up -d --build hub
set -euo pipefail
tag=${1:?usage: scripts/fetch-macos-clients.sh <tag>}
cd "$(dirname "$0")/../deploy/clients"
gh release download "$tag" --clobber -p 'davra-macos-*' -p SHA256SUMS-macos
if command -v sha256sum >/dev/null; then sha256sum -c --ignore-missing SHA256SUMS-macos; else shasum -a 256 -c --ignore-missing SHA256SUMS-macos; fi
rm SHA256SUMS-macos
ls -l davra-macos-*
