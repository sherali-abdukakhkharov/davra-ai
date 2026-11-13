//! Local client state: `~/.davra/config.toml` (no secrets) and the token in
//! the OS keychain (or `~/.davra/credentials.toml`, mode 0600, where there is
//! no keychain). `DAVRA_TOKEN` / `DAVRA_HUB` override both, for CI.

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
    /// Repositories linked from Davra Desktop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<PathBuf>,
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

/// Fallback for machines without a keychain (headless Linux servers):
/// `~/.davra/credentials.toml`, readable only by the user.
fn credentials_path() -> Result<PathBuf> {
    Ok(dir()?.join("credentials.toml"))
}

#[derive(Serialize, Deserialize, Default)]
struct Credentials {
    #[serde(default)]
    tokens: std::collections::BTreeMap<String, String>,
}

fn read_credentials() -> Result<Credentials> {
    let p = credentials_path()?;
    if !p.exists() {
        return Ok(Credentials::default());
    }
    let text = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", p.display()))
}

fn write_credentials(c: &Credentials) -> Result<()> {
    let p = credentials_path()?;
    if c.tokens.is_empty() {
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
        return Ok(());
    }
    std::fs::create_dir_all(p.parent().expect("credentials have a parent"))?;
    let text = toml::to_string_pretty(c)?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&p)?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(text.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(&p, text)?;
    Ok(())
}

/// Where `store_token` put the token.
pub enum Stored {
    Keychain,
    File(PathBuf),
}

pub fn store_token(hub: &str, token: &str) -> Result<Stored> {
    let keychain = entry(hub).and_then(|e| e.set_password(token).map_err(Into::into));
    let mut creds = read_credentials().unwrap_or_default();
    match keychain {
        Ok(()) => {
            // Never leave an older copy on disk next to the keychain one.
            if creds.tokens.remove(hub).is_some() {
                write_credentials(&creds)?;
            }
            Ok(Stored::Keychain)
        }
        Err(_) => {
            creds.tokens.insert(hub.to_string(), token.to_string());
            write_credentials(&creds)?;
            Ok(Stored::File(credentials_path()?))
        }
    }
}

pub fn delete_token(hub: &str) -> Result<()> {
    if let Ok(e) = entry(hub) {
        match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            // No keychain on this machine: the file below is all there is.
            Err(keyring::Error::PlatformFailure(_)) | Err(keyring::Error::NoStorageAccess(_)) => {}
            Err(e) => return Err(e).context("could not remove the token from the OS keychain"),
        }
    }
    let mut creds = read_credentials()?;
    if creds.tokens.remove(hub).is_some() {
        write_credentials(&creds)?;
    }
    Ok(())
}

pub fn load_token(hub: &str) -> Result<String> {
    if let Ok(t) = std::env::var("DAVRA_TOKEN") {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }
    let keychain = entry(hub).and_then(|e| match e.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
    });
    if let Ok(Some(t)) = keychain {
        return Ok(t);
    }
    if let Some(t) = read_credentials()?.tokens.get(hub) {
        return Ok(t.clone());
    }
    match keychain {
        Err(e) => Err(e).context("could not read the token from the OS keychain"),
        _ => bail!("not logged in to {hub}: run `davra login --hub {hub}`"),
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
