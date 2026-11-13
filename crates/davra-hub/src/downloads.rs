//! One-line installers for the `davra` client, served by the hub itself:
//! `irm https://hub/install.ps1 | iex` and `curl -fsSL https://hub/install.sh | sh`.
//! Binaries come from `DAVRA_DOWNLOADS_DIR` with a `SHA256SUMS` file; the
//! scripts verify the checksum before installing.

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::AppState;

const PS1: &str = r#"# Davra client installer for Windows (PowerShell).
$ErrorActionPreference = 'Stop'
$hub = '{{HUB}}'
$dir = Join-Path $env:LOCALAPPDATA 'Davra\bin'
New-Item -ItemType Directory -Force $dir | Out-Null
$exe = Join-Path $dir 'davra.exe'
$tmp = "$exe.download"
Write-Host "Downloading davra from $hub ..."
$r = Invoke-WebRequest -UseBasicParsing "$hub/downloads/SHA256SUMS"
# Windows PowerShell 5.1 returns bytes for non-text content types.
$sums = if ($r.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($r.Content) } else { [string]$r.Content }
$want = ($sums -split "`n" | Where-Object { $_ -match 'davra-windows-x86_64\.exe' } | ForEach-Object { ($_.Trim() -split '\s+')[0] }) | Select-Object -First 1
if (-not $want) { throw "davra: no checksum for davra-windows-x86_64.exe on $hub" }
# The checksum in the URL gives every build its own cache key, so a CDN can never serve an older binary.
Invoke-WebRequest -UseBasicParsing "$hub/downloads/davra-windows-x86_64.exe?sha=$want" -OutFile $tmp
$got = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToLower()
if (-not $want -or $got -ne $want.ToLower()) { Remove-Item $tmp -Force; throw "davra: checksum mismatch, not installing" }
# A running davra (an open `davra claude` or MCP server) can't be overwritten, but it can be
# renamed: move it aside, and the next session starts the new one.
if (Test-Path $exe) {
  Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
  Move-Item -Force $exe "$exe.old"
  Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
}
Move-Item -Force $tmp $exe
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ((($userPath -split ';') | Where-Object { $_ -eq $dir }).Count -eq 0) {
  [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ';' + $dir), 'User')
  $env:Path += ";$dir"
  Write-Host "Added $dir to your PATH (new terminals pick it up)."
}
& $exe --version
Write-Host ""
Write-Host "Next:"
Write-Host "  davra login --hub $hub"
Write-Host "  davra setup claude"
"#;

const SH: &str = r#"#!/bin/sh
# Davra client installer for Linux.
set -eu
HUB='{{HUB}}'
os=$(uname -s); arch=$(uname -m)
case "$os-$arch" in
  Linux-x86_64) f=davra-linux-x86_64 ;;
  *) echo "davra: no prebuilt binary for $os/$arch yet." >&2; exit 1 ;;
esac
dir="$HOME/.davra/bin"
mkdir -p "$dir"
echo "Downloading davra from $HUB ..."
want=$(curl -fsSL "$HUB/downloads/SHA256SUMS" | awk -v f="$f" '$2 == f { print $1 }')
[ -n "$want" ] || { echo "davra: no checksum for $f on $HUB" >&2; exit 1; }
# The checksum in the URL gives every build its own cache key, so a CDN can never serve an older binary.
curl -fsSL "$HUB/downloads/$f?sha=$want" -o "$dir/davra.download"
got=$( (sha256sum "$dir/davra.download" 2>/dev/null || shasum -a 256 "$dir/davra.download") | awk '{ print $1 }')
if [ -z "$want" ] || [ "$want" != "$got" ]; then
  rm -f "$dir/davra.download"; echo "davra: checksum mismatch, not installing" >&2; exit 1
fi
chmod +x "$dir/davra.download"
mv "$dir/davra.download" "$dir/davra"
# Put ~/.davra/bin on PATH for every shell this user may open: ~/.profile
# alone is read only by login shells, and zsh never reads it.
added=""
add_line() { # file line
  if ! grep -qs '\.davra/bin' "$1"; then
    mkdir -p "$(dirname "$1")"
    printf '\n# Added by the Davra installer\n%s\n' "$2" >> "$1"
    added="$added ~/${1#"$HOME/"}"
  fi
}
posix='export PATH="$HOME/.davra/bin:$PATH"'
login_shell=$(basename "${SHELL:-sh}")
add_line "$HOME/.profile" "$posix"
if [ "$login_shell" = bash ] || [ -f "$HOME/.bashrc" ]; then add_line "$HOME/.bashrc" "$posix"; fi
if [ "$login_shell" = zsh ] || [ -f "${ZDOTDIR:-$HOME}/.zshrc" ]; then add_line "${ZDOTDIR:-$HOME}/.zshrc" "$posix"; fi
if [ "$login_shell" = fish ] || [ -d "$HOME/.config/fish" ]; then
  add_line "$HOME/.config/fish/conf.d/davra.fish" 'fish_add_path -g $HOME/.davra/bin'
fi
[ -n "$added" ] && echo "Added ~/.davra/bin to PATH in:$added"
"$dir/davra" --version
echo
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "New terminals find davra. For this one, run first:"
     if [ "$login_shell" = fish ]; then echo "  fish_add_path -g ~/.davra/bin"; else echo "  export PATH=\"\$HOME/.davra/bin:\$PATH\""; fi
     echo ;;
esac
echo "Next:"
echo "  davra login --hub $HUB"
echo "  davra setup claude"
echo "(The token goes to the Secret Service keyring, or ~/.davra/credentials.toml (0600) where there is none.)"
"#;

/// The hub's own public URL: configured, else derived from the request.
pub fn hub_url(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(u) = &state.public_url {
        return u.clone();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .filter(|h| {
            h.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
        })
        .unwrap_or("localhost");
    let local =
        host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");
    format!("{}://{host}", if local { "http" } else { "https" })
}

fn script(body: &str, hub: &str) -> Response {
    let text = body.replace("{{HUB}}", hub);
    let mut r = text.into_response();
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    r
}

pub async fn install_ps1(State(state): State<AppState>, headers: HeaderMap) -> Response {
    script(PS1, &hub_url(&state, &headers))
}

pub async fn install_sh(State(state): State<AppState>, headers: HeaderMap) -> Response {
    script(SH, &hub_url(&state, &headers))
}

#[derive(serde::Deserialize)]
pub struct UpdateQuery {
    target: String,
    arch: String,
    current: String,
}

/// "0.2.10" > "0.2.9"; a pre-release suffix ("-beta.1") is ignored.
fn newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split('-')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(candidate) > parts(current)
}

/// What Davra Desktop's updater asks the hub it is connected to (Tauri's dynamic
/// update format): 204 when there is nothing newer for that platform, else the
/// signed installer to fetch. The signature is checked by the app against the
/// public key it was built with, so the hub can only offer, never forge.
pub async fn desktop_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<UpdateQuery>,
) -> Response {
    let none = || StatusCode::NO_CONTENT.into_response();
    let Some(dir) = &state.desktop_dir else {
        return none();
    };
    let Some(latest) = tokio::fs::read_to_string(dir.join("latest.json"))
        .await
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return none();
    };
    let Some(version) = latest["version"].as_str() else {
        return none();
    };
    if !newer(version, &q.current) {
        return none();
    }
    // In-place updates: the NSIS installer on Windows, the AppImage on Linux
    // (a .deb is updated by installing the new .deb).
    let kind = match (q.target.as_str(), q.arch.as_str()) {
        ("windows", "x86_64") => "nsis",
        ("linux", "x86_64") => "appimage",
        _ => return none(),
    };
    let Some(file) = latest["files"].as_array().and_then(|fs| {
        fs.iter()
            .find(|f| f["kind"] == kind && f["signature"].is_string())
    }) else {
        return none();
    };
    let hub = hub_url(&state, &headers);
    let mut body = serde_json::json!({
        "version": version,
        "notes": format!("Davra Desktop {version}"),
        "url": format!(
            "{hub}/desktop/{}?sha={}",
            file["name"].as_str().unwrap_or_default(),
            file["sha256"].as_str().unwrap_or_default()
        ),
        "signature": file["signature"],
    });
    if let Some(d) = latest["pub_date"].as_str() {
        body["pub_date"] = d.into();
    }
    let mut r = axum::Json(body).into_response();
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_versions_compare_numerically() {
        assert!(newer("0.2.10", "0.2.9"));
        assert!(newer("0.3.0", "0.2.99"));
        assert!(newer("1.0.0", "0.9.0-beta.1"));
        assert!(!newer("0.2.1", "0.2.1"));
        assert!(!newer("0.2.0", "0.2.1"));
    }
    #[test]
    fn scripts_have_the_hub_and_verify_checksums() {
        for s in [PS1, SH] {
            let t = s.replace("{{HUB}}", "https://davra.qitmir.uz");
            assert!(t.contains("https://davra.qitmir.uz"));
            assert!(!t.contains("{{HUB}}"));
            assert!(t.contains("SHA256SUMS"));
            assert!(t.contains("checksum mismatch"));
        }
        // Login shells, interactive bash, zsh and fish each read a different file.
        for rc in [".profile", ".bashrc", ".zshrc", "conf.d/davra.fish"] {
            assert!(SH.contains(rc), "install.sh misses {rc}");
        }
    }
}
