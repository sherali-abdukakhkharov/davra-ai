//! Installing the `davra` CLI into `~/.davra/bin` and putting it on PATH, the
//! same way the hub's install.ps1 / install.sh do. Used by Davra Desktop,
//! which ships the CLI inside its installer.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where install.ps1 / install.sh put the CLI, so every installer replaces
/// the same copy: `%LOCALAPPDATA%\Davra\bin` on Windows, `~/.davra/bin`
/// elsewhere (and wherever `DAVRA_HOME` points, for tests).
pub fn bin_dir() -> Result<PathBuf> {
    if cfg!(windows) && std::env::var_os("DAVRA_HOME").is_none() {
        if let Some(local) = dirs::data_local_dir() {
            return Ok(local.join("Davra").join("bin"));
        }
    }
    Ok(config::dir()?.join("bin"))
}

pub fn cli_path() -> Result<PathBuf> {
    Ok(bin_dir()?.join(if cfg!(windows) { "davra.exe" } else { "davra" }))
}

/// `davra --version` of the binary at `path` ("0.2.0"), if it runs.
pub fn version_of(path: &Path) -> Option<String> {
    let mut c = std::process::Command::new(path);
    c.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().nth(1).map(str::to_string)
}

/// Copies `from` to `~/.davra/bin`. A running old copy is moved aside first
/// (Windows cannot overwrite a running exe, but it can rename one).
pub fn install_from(from: &Path) -> Result<PathBuf> {
    let to = cli_path()?;
    std::fs::create_dir_all(to.parent().expect("bin has a parent"))?;
    if to.exists() {
        let old = to.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&to, &old)
            .with_context(|| format!("moving the old {} aside", to.display()))?;
        let _ = std::fs::remove_file(&old);
    }
    std::fs::copy(from, &to)
        .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o755))?;
    }
    if version_of(&to).is_none() {
        bail!("the installed {} does not run", to.display());
    }
    Ok(to)
}

/// Whether new terminals will find `dir`: the user PATH on Windows (the
/// registry, not this process's possibly stale copy), the shell files on Unix.
pub fn on_path(dir: &Path) -> bool {
    #[cfg(windows)]
    {
        user_path_windows()
            .map(|p| p.split(';').any(|e| same_dir(Path::new(e.trim()), dir)))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let in_env = std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|e| same_dir(&e, dir)));
        in_env
            || rc_files()
                .iter()
                .any(|(f, _, _)| std::fs::read_to_string(f).is_ok_and(|t| t.contains(".davra/bin")))
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.display()
            .to_string()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    norm(a) == norm(b)
}

#[cfg(windows)]
fn powershell(script: &str, dir: &Path) -> Result<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DAVRA_BIN_DIR", dir)
        .creation_flags(0x0800_0000)
        .output()
        .context("running PowerShell")?;
    if !out.status.success() {
        bail!(
            "PowerShell: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(windows)]
fn user_path_windows() -> Result<String> {
    powershell(
        "[Environment]::GetEnvironmentVariable('Path', 'User')",
        Path::new(""),
    )
}

/// Adds `dir` to PATH for new terminals; false when it was there already.
pub fn add_to_path(dir: &Path) -> Result<bool> {
    if on_path(dir) {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        // SetEnvironmentVariable also tells running programs (Explorer) to reload.
        powershell(
            "$d = $env:DAVRA_BIN_DIR; $p = [Environment]::GetEnvironmentVariable('Path', 'User'); \
             if (-not $p) { $p = '' }; \
             [Environment]::SetEnvironmentVariable('Path', (($p.TrimEnd(';') + ';' + $d).TrimStart(';')), 'User')",
            dir,
        )?;
    }
    #[cfg(not(windows))]
    {
        for (file, line, wanted) in rc_files() {
            if !wanted && !file.exists() {
                continue;
            }
            let mut text = std::fs::read_to_string(&file).unwrap_or_default();
            if text.contains(".davra/bin") {
                continue;
            }
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!("{line}\n"));
            if let Some(d) = file.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
        }
    }
    Ok(true)
}

/// Login shells, interactive bash, zsh and fish each read a different file.
#[cfg(not(windows))]
fn rc_files() -> Vec<(PathBuf, &'static str, bool)> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };
    let posix = "export PATH=\"$HOME/.davra/bin:$PATH\"";
    let zdot = std::env::var_os("ZDOTDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.clone());
    vec![
        (home.join(".profile"), posix, true),
        (home.join(".bashrc"), posix, false),
        (zdot.join(".zshrc"), posix, false),
        (
            home.join(".config/fish/conf.d/davra.fish"),
            "fish_add_path -g $HOME/.davra/bin",
            home.join(".config/fish").is_dir(),
        ),
    ]
}
