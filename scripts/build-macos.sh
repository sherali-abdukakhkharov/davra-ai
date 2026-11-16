#!/usr/bin/env bash
# Builds the davra CLI (and optionally Davra Desktop) for macOS, on a Mac.
# The hub's Docker image can't target macOS, so these come from a Mac and
# reach the hub through a GitHub Release. Guide: docs/build-macos.md.
#
#   scripts/build-macos.sh                  # CLI for Apple Silicon and Intel
#   scripts/build-macos.sh cli desktop      # also Davra Desktop (.app, .dmg; universal)
#   scripts/build-macos.sh cli --upload     # then attach the files to the release v<version>
#
# Output: dist-macos/ (gitignored)
#   davra-macos-aarch64, davra-macos-x86_64      CLI, ad-hoc signed
#   Davra_<version>_universal.dmg                Desktop (with `desktop`)
#   Davra_<version>_universal.app.tar.gz(.sig)   Desktop update bundle (with TAURI_SIGNING_PRIVATE_KEY)
#   SHA256SUMS-macos
#
# Desktop updater key: ~/.davra-release/updater.key (or DAVRA_UPDATER_KEY, or
# TAURI_SIGNING_PRIVATE_KEY [+ _PASSWORD]); without it no update bundle is made.
# CARGO_BUILD_JOBS to limit parallel jobs. DMG_LAYOUT=1 to arrange the .dmg window
# through Finder (needs permission to control Finder).
set -euo pipefail

die() { echo "build-macos: $*" >&2; exit 1; }
[[ "$(uname -s)" == Darwin ]] || die "run this on macOS"
cd "$(dirname "$0")/.."

what=() upload=false
for a in "$@"; do
  case "$a" in
    cli | desktop) what+=("$a") ;;
    --upload) upload=true ;;
    *) die "unknown argument: $a (expected cli, desktop, --upload)" ;;
  esac
done
[[ ${#what[@]} -eq 0 ]] && what=(cli)
[[ " ${what[*]} " == *" desktop "* && " ${what[*]} " != *" cli "* ]] && what=(cli "${what[@]}")

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[[ -n "$version" ]] || die "no version in Cargo.toml"
out=dist-macos
targets=(aarch64-apple-darwin x86_64-apple-darwin)

command -v rustup >/dev/null ||
  die "needs rustup (Homebrew's rust builds only for this Mac's own CPU): brew install rustup && rustup-init -y"
rustup target add "${targets[@]}" >/dev/null

rm -rf "$out" && mkdir -p "$out"
echo "==> davra $version for macOS"

# --- CLI -------------------------------------------------------------------------------
for t in "${targets[@]}"; do
  echo "==> davra CLI ($t)"
  cargo build --locked --release -p davra-cli --bin davra --target "$t"
done
cp target/aarch64-apple-darwin/release/davra "$out/davra-macos-aarch64"
cp target/x86_64-apple-darwin/release/davra "$out/davra-macos-x86_64"
for f in "$out"/davra-macos-*; do
  # Apple Silicon runs only signed code; the linker's ad-hoc signature is enough.
  codesign --verify "$f" 2>/dev/null || codesign --force --sign - "$f"
done
[[ "$(lipo -archs "$out/davra-macos-aarch64")" == arm64 ]] || die "davra-macos-aarch64 is not arm64"
[[ "$(lipo -archs "$out/davra-macos-x86_64")" == x86_64 ]] || die "davra-macos-x86_64 is not x86_64"
native="$out/davra-macos-$([[ "$(uname -m)" == arm64 ]] && echo aarch64 || echo x86_64)"
got=$("$native" --version)
[[ "$got" == "davra $version" ]] || die "$native --version says '$got', expected 'davra $version'"
echo "    $got"

# --- Desktop ---------------------------------------------------------------------------
if [[ " ${what[*]} " == *" desktop "* ]]; then
  command -v npm >/dev/null || die "Davra Desktop needs Node.js and npm (brew install node)"
  echo "==> Desktop UI (web/dist-desktop)"
  [[ -d web/node_modules ]] || npm --prefix web ci
  npm --prefix web run build:desktop

  # Tauri's sidecar naming: binaries/davra-<target triple>.
  bins=desktop/src-tauri/binaries
  mkdir -p "$bins"
  cp "$out/davra-macos-aarch64" "$bins/davra-aarch64-apple-darwin"
  cp "$out/davra-macos-x86_64" "$bins/davra-x86_64-apple-darwin"
  lipo -create -output "$bins/davra-universal-apple-darwin" "$out/davra-macos-aarch64" "$out/davra-macos-x86_64"

  if ! cargo tauri --version 2>/dev/null | grep -q '^tauri-cli 2\.'; then
    echo "==> installing tauri-cli 2.12.4"
    cargo install tauri-cli --version 2.12.4 --locked
  fi
  # Same place as scripts/build-desktop.sh: the key the user copied from the hub machine.
  key="${DAVRA_UPDATER_KEY:-$HOME/.davra-release/updater.key}"
  if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -f "$key" ]]; then
    export TAURI_SIGNING_PRIVATE_KEY="$key"
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${DAVRA_UPDATER_KEY_PASSWORD:-}"
  fi
  sign=false
  [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] && sign=true
  if $sign; then echo "==> the update bundle will be signed"; else echo "==> no updater key: .dmg only, no auto-update bundle"; fi
  # The UI is built above; "-" signs the app ad hoc (no Apple Developer ID yet).
  config="{\"build\":{\"beforeBuildCommand\":\"\"},\"bundle\":{\"createUpdaterArtifacts\":$sign,\"macOS\":{\"signingIdentity\":\"-\"}}}"
  # Tauri arranges the .dmg window by driving Finder with AppleScript. Without permission to
  # control Finder (an agent's shell, SSH) that times out with -1712 and the build fails.
  # CI=true makes Tauri skip it; the .dmg works the same, its icons just aren't arranged.
  ci=true
  [[ "${DMG_LAYOUT:-}" == 1 ]] && ci=false
  echo "==> Davra Desktop (universal .app, .dmg)"
  (cd desktop/src-tauri && CI=$ci cargo tauri build --ci --target universal-apple-darwin --bundles app,dmg --config "$config")
  bundle=desktop/src-tauri/target/universal-apple-darwin/release/bundle
  cp "$bundle"/dmg/*.dmg "$out/Davra_${version}_universal.dmg"
  if $sign; then
    cp "$bundle"/macos/*.app.tar.gz "$out/Davra_${version}_universal.app.tar.gz"
    cp "$bundle"/macos/*.app.tar.gz.sig "$out/Davra_${version}_universal.app.tar.gz.sig"
  fi
fi

(cd "$out" && shasum -a 256 $(ls | grep -v '\.sig$') > SHA256SUMS-macos && cat SHA256SUMS-macos)

# --- Upload ----------------------------------------------------------------------------
if $upload; then
  command -v gh >/dev/null || die "--upload needs the GitHub CLI (brew install gh; gh auth login)"
  tag="v$version"
  head=$(git rev-parse HEAD)
  git fetch -q origin
  git branch -r --contains "$head" | grep -q . || die "commit $head is not pushed; push it first"
  echo "==> GitHub Release $tag"
  gh release view "$tag" >/dev/null 2>&1 ||
    gh release create "$tag" --target "$head" --title "Davra $version" --notes "Davra $version"
  gh release upload "$tag" "$out"/* --clobber
  gh release view "$tag" --json url --jq .url
fi
echo "==> done: $out/"
