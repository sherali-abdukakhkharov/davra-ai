//! One-line installers for the `davra` client, served by the hub itself:
//! `irm https://hub/install.ps1 | iex` and `curl -fsSL https://hub/install.sh | sh`.
//! Binaries come from `DAVRA_DOWNLOADS_DIR` with a `SHA256SUMS` file; the
//! scripts verify the checksum before installing.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue};
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
Invoke-WebRequest -UseBasicParsing "$hub/downloads/davra-windows-x86_64.exe" -OutFile $tmp
$r = Invoke-WebRequest -UseBasicParsing "$hub/downloads/SHA256SUMS"
# Windows PowerShell 5.1 returns bytes for non-text content types.
$sums = if ($r.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($r.Content) } else { [string]$r.Content }
$want = ($sums -split "`n" | Where-Object { $_ -match 'davra-windows-x86_64\.exe' } | ForEach-Object { ($_.Trim() -split '\s+')[0] }) | Select-Object -First 1
$got = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToLower()
if (-not $want -or $got -ne $want.ToLower()) { Remove-Item $tmp -Force; throw "davra: checksum mismatch, not installing" }
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
curl -fsSL "$HUB/downloads/$f" -o "$dir/davra.download"
want=$(curl -fsSL "$HUB/downloads/SHA256SUMS" | awk -v f="$f" '$2 == f { print $1 }')
got=$( (sha256sum "$dir/davra.download" 2>/dev/null || shasum -a 256 "$dir/davra.download") | awk '{ print $1 }')
if [ -z "$want" ] || [ "$want" != "$got" ]; then
  rm -f "$dir/davra.download"; echo "davra: checksum mismatch, not installing" >&2; exit 1
fi
chmod +x "$dir/davra.download"
mv "$dir/davra.download" "$dir/davra"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "export PATH=\"$dir:\$PATH\"" >> "$HOME/.profile"
     echo "Added $dir to PATH in ~/.profile (open a new shell)." ;;
esac
"$dir/davra" --version
echo
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_have_the_hub_and_verify_checksums() {
        for s in [PS1, SH] {
            let t = s.replace("{{HUB}}", "https://davra.qitmir.uz");
            assert!(t.contains("https://davra.qitmir.uz"));
            assert!(!t.contains("{{HUB}}"));
            assert!(t.contains("SHA256SUMS"));
            assert!(t.contains("checksum mismatch"));
        }
    }
}
