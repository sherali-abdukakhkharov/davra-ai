#!/usr/bin/env bash
# Writes latest.json and SHA256SUMS for the Davra Desktop files in a publish
# directory (default desktop/dist/publish), from the files themselves:
#
#   Davra_<v>_x64-setup.exe          windows  nsis      (updates in place)
#   Davra_<v>_amd64.deb              linux    deb
#   Davra_<v>_amd64.AppImage         linux    appimage  (updates in place)
#   Davra_<v>_universal.dmg          macos    dmg
#   Davra_<v>_universal.app.tar.gz   macos    app       (updates in place)
#
# A `<file>.sig` next to a file is its updater signature. Each file keeps its own
# version, so Windows/Linux (built here) and macOS (built on a Mac) can be
# published at different times; the top-level version is the newest one.
# Used by scripts/build-desktop.sh and scripts/fetch-macos-clients.sh.
set -euo pipefail
pub=${1:-desktop/dist/publish}
cd "$pub"

sum() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
newest=""
newer() { # a b: is a > b (dotted numbers)
  [[ -z "$2" ]] && return 0
  [[ "$1" == "$2" ]] && return 1
  [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)" == "$1" ]]
}

entries=() names=()
shopt -s nullglob
for f in Davra_*_x64-setup.exe Davra_*_amd64.deb Davra_*_amd64.AppImage Davra_*_universal.dmg Davra_*_universal.app.tar.gz; do
  case "$f" in
    *_x64-setup.exe) os=windows kind=nsis ;;
    *_amd64.deb) os=linux kind=deb ;;
    *_amd64.AppImage) os=linux kind=appimage ;;
    *_universal.dmg) os=macos kind=dmg ;;
    *_universal.app.tar.gz) os=macos kind=app ;;
  esac
  v=${f#Davra_}; v=${v%%_*}
  newer "$v" "$newest" && newest=$v
  size=$(wc -c <"$f" | tr -d ' ')
  sha=$(sum "$f" | cut -d' ' -f1)
  sig=""
  [[ -f "$f.sig" ]] && sig=",\"signature\":\"$(tr -d '\r\n' <"$f.sig")\""
  entries+=("{\"os\":\"$os\",\"kind\":\"$kind\",\"version\":\"$v\",\"name\":\"$f\",\"size\":$size,\"sha256\":\"$sha\"$sig}")
  names+=("$f")
done
if [[ ${#entries[@]} -eq 0 ]]; then
  echo "desktop-manifest: no Davra Desktop files in $pub" >&2
  exit 1
fi
# Git Bash's sha256sum marks binary mode with " *"; the plain two-space form reads everywhere.
sum "${names[@]}" | sed 's/ \*/  /' >SHA256SUMS
printf '{"version":"%s","pub_date":"%s","files":[%s]}\n' "$newest" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  "$(IFS=,; echo "${entries[*]}")" >latest.json
echo "==> $pub/latest.json: ${names[*]}"
