//! Local client state: `~/.davra/config.toml` (no secrets) and the token in
//! the OS keychain. `DAVRA_TOKEN` / `DAVRA_HUB` override both, for CI and
//! containers where no keychain exists.

use anyhow::{bail, Context, Result};
use davra_core::project_file;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const KEYRING_SERVICE: &str = "davra";

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Config {
    pub hub: Option<String>,
    /// Used when the current directory has no `.davra.toml`.
    pub default_project: Option<String>,
}

pub fn dir() -> Result<PathBuf> {
    if let Ok(d) = std::env::var("DAVRA_HOME") {
        return Ok(PathBuf::from(d));
    }
    Ok(dirs::home_dir()
        .context("no home directory")?
        .join(".davra"))
}

fn config_path() -> Result<PathBuf> {
    Ok(dir()?.join("config.toml"))
}

impl Config {
    pub fn load() -> Result<Self> {
        let p = config_path()?;
        if !p.exists() {
            return Ok(Config::default());
        }
        let text =
            std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", p.display()))
    }

    pub fn save(&self) -> Result<()> {
        let p = config_path()?;
        std::fs::create_dir_all(p.parent().expect("config has a parent"))?;
        std::fs::write(&p, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

pub fn normalize_hub(url: &str) -> Result<String> {
    let u = url.trim().trim_end_matches('/');
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"];
    if !(u.starts_with("https://") || local.iter().any(|l| u.starts_with(l))) {
        bail!("hub URL must use https:// (plain http is only allowed for localhost)");
    }
    Ok(u.to_string())
}

fn entry(hub: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, hub).context("OS keychain is not available")
}

pub fn store_token(hub: &str, token: &str) -> Result<()> {
    entry(hub)?
        .set_password(token)
        .context("could not write the token to the OS keychain (set DAVRA_TOKEN instead)")
}

pub fn delete_token(hub: &str) -> Result<()> {
    match entry(hub)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("could not remove the token from the OS keychain"),
    }
}

pub fn load_token(hub: &str) -> Result<String> {
    if let Ok(t) = std::env::var("DAVRA_TOKEN") {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }
    match entry(hub)?.get_password() {
        Ok(t) => Ok(t),
        Err(keyring::Error::NoEntry) => {
            bail!("not logged in to {hub}: run `davra login --hub {hub}`")
        }
        Err(e) => Err(e).context("could not read the token from the OS keychain"),
    }
}

/// Where requests go and which project they are about.
#[derive(Clone, Debug)]
pub struct Target {
    pub hub: String,
    /// `None` means "ask the hub": used when the member has one project.
    pub project: Option<String>,
}

/// Resolution order: `DAVRA_HUB` env, `.davra.toml` (walking up from `cwd`),
/// then `~/.davra/config.toml`.
pub fn target(cwd: &Path) -> Result<Target> {
    let cfg = Config::load()?;
    let pf = project_file::find(cwd).map(|(_, pf)| pf);
    let hub = std::env::var("DAVRA_HUB")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .or_else(|| pf.as_ref().and_then(|p| p.hub.clone()))
        .or(cfg.hub.clone())
        .context("no hub configured: run `davra login --hub <url>`")?;
    let project = pf.map(|p| p.project).or(cfg.default_project);
    Ok(Target {
        hub: normalize_hub(&hub)?,
        project,
    })
}
