#!/usr/bin/env bash
# Build Davra Desktop installers for Linux (.deb, .AppImage) and Windows (NSIS) in Docker.
#
#   scripts/build-desktop.sh [linux] [windows]      # default: both
#
# Steps:
#   1. builds the build image (desktop/Dockerfile.build; cached after the first run);
#   2. builds the UI on the host (`npm --prefix web run build:desktop` -> web/dist-desktop).
#      If web has no build:desktop script yet, a placeholder page is generated into
#      desktop/dist/.placeholder-ui and the build is pointed at it;
#   3. in the container: builds the `davra` CLI from crates/davra-cli for each target and
#      copies it to desktop/src-tauri/binaries/davra-<triple>[.exe] (Tauri sidecar naming);
#   4. bundles: Linux natively, Windows with cargo-xwin + x86_64-pc-windows-msvc.
#
# Output: desktop/dist/{linux,windows}/ (gitignored). desktop/dist/windows/app/ also holds
# the raw app exe with its sidecar, for running without installing.
#
# Caches live in named Docker volumes: davra-cargo (cargo registry, shared with davra-dev),
# davra-tauri-target (target dirs), davra-tauri-cache (cargo-xwin's MSVC CRT + Windows SDK,
# linuxdeploy, NSIS plugins).
#
# Env: DAVRA_DESKTOP_IMAGE (image tag), DAVRA_SKIP_IMAGE=1 (reuse the image as is),
# DAVRA_SKIP_UI=1 (use web/dist-desktop as it is), DAVRA_PLACEHOLDER_UI=1 (shell-only build),
# CARGO_BUILD_JOBS (default 4: a release build with LTO needs a lot of memory per job).
set -euo pipefail

LINUX_TRIPLE=x86_64-unknown-linux-gnu
WINDOWS_TRIPLE=x86_64-pc-windows-msvc

# ---------------------------------------------------------------------------------------
# Inside the container (repo at /src).
# ---------------------------------------------------------------------------------------
if [[ "${1:-}" == "--inside" ]]; then
  shift
  ui_override="${UI_DIST_OVERRIDE:-}"
  # The UI is built on the host, so the container skips beforeBuildCommand.
  if [[ -n "$ui_override" ]]; then
    config="{\"build\":{\"beforeBuildCommand\":\"\",\"frontendDist\":\"$ui_override\"}}"
  else
    config='{"build":{"beforeBuildCommand":""}}'
  fi
  bins=/src/desktop/src-tauri/binaries
  dist=/src/desktop/dist
  mkdir -p "$bins"

  for platform in "$@"; do
    start=$SECONDS
    case "$platform" in
      linux)
        echo "==> davra CLI ($LINUX_TRIPLE)"
        (cd /src && CARGO_TARGET_DIR=/target/cli cargo build --locked --release -p davra-cli --bin davra)
        install -m 0755 /target/cli/release/davra "$bins/davra-$LINUX_TRIPLE"

        echo "==> Davra Desktop (deb, appimage)"
        (cd /src/desktop/src-tauri && CARGO_TARGET_DIR=/target/desktop \
          cargo tauri build --ci --bundles deb,appimage --config "$config")
        rm -rf "$dist/linux" && mkdir -p "$dist/linux"
        cp /target/desktop/release/bundle/deb/*.deb /target/desktop/release/bundle/appimage/*.AppImage "$dist/linux/"
        ;;
      windows)
        echo "==> davra CLI ($WINDOWS_TRIPLE, cargo-xwin)"
        (cd /src && CARGO_TARGET_DIR=/target/cli \
          cargo xwin build --locked --release --target "$WINDOWS_TRIPLE" -p davra-cli --bin davra)
        install -m 0755 "/target/cli/$WINDOWS_TRIPLE/release/davra.exe" "$bins/davra-$WINDOWS_TRIPLE.exe"

        echo "==> Davra Desktop (nsis, cargo-xwin)"
        (cd /src/desktop/src-tauri && CARGO_TARGET_DIR=/target/desktop \
          cargo tauri build --ci --runner cargo-xwin --target "$WINDOWS_TRIPLE" --bundles nsis --config "$config")
        out="/target/desktop/$WINDOWS_TRIPLE/release"
        rm -rf "$dist/windows" && mkdir -p "$dist/windows/app"
        cp "$out"/bundle/nsis/*.exe "$dist/windows/"
        cp "$out/davra-desktop.exe" "$dist/windows/app/"
        cp "$bins/davra-$WINDOWS_TRIPLE.exe" "$dist/windows/app/davra.exe"
        ;;
      *) echo "unknown platform: $platform (expected linux or windows)" >&2; exit 2 ;;
    esac
    echo "==> $platform done in $((SECONDS - start))s"
  done
  exit 0
fi

# ---------------------------------------------------------------------------------------
# On the host.
# ---------------------------------------------------------------------------------------
cd "$(dirname "$0")/.."
repo="$(pwd)"
# Docker Desktop on Windows wants a Windows path for bind mounts; Git Bash must not rewrite
# the container-side paths.
if command -v cygpath >/dev/null 2>&1; then
  repo_mount="$(pwd -W)"
  export MSYS_NO_PATHCONV=1
else
  repo_mount="$repo"
fi

platforms=("$@")
[[ ${#platforms[@]} -eq 0 ]] && platforms=(linux windows)
image="${DAVRA_DESKTOP_IMAGE:-davra-desktop-build}"
total_start=$SECONDS

if [[ "${DAVRA_SKIP_IMAGE:-}" != "1" ]]; then
  echo "==> build image ($image)"
  docker build -t "$image" -f desktop/Dockerfile.build desktop
fi

ui_override=""
if [[ "${DAVRA_SKIP_UI:-}" != "1" && "${DAVRA_PLACEHOLDER_UI:-}" != "1" ]] && grep -q '"build:desktop"' web/package.json; then
  echo "==> UI (npm --prefix web run build:desktop)"
  [[ -d web/node_modules ]] || npm --prefix web ci
  npm --prefix web run build:desktop
fi
if [[ "${DAVRA_PLACEHOLDER_UI:-}" == "1" || ! -f web/dist-desktop/index.html ]]; then
  echo "==> using a placeholder UI (desktop/dist/.placeholder-ui)"
  mkdir -p desktop/dist/.placeholder-ui
  cat > desktop/dist/.placeholder-ui/index.html <<'HTML'
<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Davra</title></head>
<body style="font-family: system-ui, sans-serif; display: grid; place-items: center; height: 100vh; margin: 0; background: #1d2f8f; color: #f3f6fa">
  <p>Davra Desktop: placeholder UI</p>
</body>
</html>
HTML
  # Relative to desktop/src-tauri, like frontendDist in tauri.conf.json.
  ui_override="../dist/.placeholder-ui"
fi

docker run --rm \
  -v "$repo_mount:/src" \
  -v davra-cargo:/usr/local/cargo/registry \
  -v davra-tauri-target:/target \
  -v davra-tauri-cache:/root/.cache \
  -e UI_DIST_OVERRIDE="$ui_override" \
  -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" \
  -w /src \
  "$image" bash /src/scripts/build-desktop.sh --inside "${platforms[@]}"

# What the hub serves under /desktop (DAVRA_DESKTOP_DIR): this version's installers, their
# checksums and latest.json, which the web app reads to offer the download.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' desktop/src-tauri/Cargo.toml | head -1)
pub=desktop/dist/publish
rm -rf "$pub" && mkdir -p "$pub"
entries=()
for spec in "windows:nsis:desktop/dist/windows/Davra_${version}_x64-setup.exe" \
            "linux:deb:desktop/dist/linux/Davra_${version}_amd64.deb" \
            "linux:appimage:desktop/dist/linux/Davra_${version}_amd64.AppImage"; do
  IFS=: read -r os kind file <<<"$spec"
  [[ -f "$file" ]] || continue
  cp "$file" "$pub/"
  name=$(basename "$file")
  size=$(wc -c <"$file" | tr -d ' ')
  sum=$(sha256sum "$file" | cut -d' ' -f1)
  entries+=("{\"os\":\"$os\",\"kind\":\"$kind\",\"name\":\"$name\",\"size\":$size,\"sha256\":\"$sum\"}")
done
if [[ ${#entries[@]} -gt 0 ]]; then
  (cd "$pub" && sha256sum -- *.exe *.deb *.AppImage 2>/dev/null >SHA256SUMS || true)
  printf '{"version":"%s","files":[%s]}\n' "$version" "$(IFS=,; echo "${entries[*]}")" >"$pub/latest.json"
  echo "==> publish: $pub ($(cat "$pub/latest.json" | head -c 200)...)"
fi

echo "==> done in $((SECONDS - total_start))s"
find desktop/dist -maxdepth 2 -type f \( -name '*.deb' -o -name '*.AppImage' -o -name '*-setup.exe' \) -exec ls -l {} +
