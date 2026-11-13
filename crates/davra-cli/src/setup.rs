//! `davra setup claude`: connects Claude Code to Davra on this machine.
//!
//! 1. Registers the MCP server for all projects (`claude mcp add --scope user`).
//! 2. Adds the SessionStart / UserPromptSubmit / Stop hooks to `~/.claude/settings.json`.
//!
//! Safe to run again: nothing is added twice. Every file is backed up to
//! `~/.davra/backups/` before it changes, and the changes are listed first.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::config;

pub struct Plan {
    pub settings_path: PathBuf,
    /// Hook events that have no Davra hook yet.
    pub missing: Vec<&'static str>,
    pub mcp_present: bool,
    exe: String,
}

/// Forward slashes work for Claude Code on every OS and need no escaping.
fn exe_string(exe: &Path) -> String {
    exe.display().to_string().replace('\\', "/")
}

fn has_davra_hook(settings: &Value, event: &str) -> bool {
    settings["hooks"][event]
        .as_array()
        .map(|groups| {
            groups.iter().any(|g| {
                g["hooks"].as_array().is_some_and(|hs| {
                    hs.iter().any(|h| {
                        h["command"].as_str().is_some_and(|c| {
                            c.contains("davra") && (c.contains(" brief") || c.contains(" inbox"))
                        })
                    })
                })
            })
        })
        .unwrap_or(false)
}

/// The hooks Davra installs: event, `davra` arguments, timeout in seconds.
/// The Stop hook keeps a finished agent going when new messages are waiting.
const HOOKS: [(&str, &str, u32); 3] = [
    ("SessionStart", "brief --hook", 20),
    ("UserPromptSubmit", "inbox --hook", 10),
    ("Stop", "inbox --hook --stop", 10),
];

/// Adds the hooks to a settings document, leaving everything else alone.
pub fn merge_hooks(settings: &mut Value, exe: &str) {
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    let add = |s: &mut Value, event: &str, cmd: String, timeout: u32| {
        if has_davra_hook(s, event) {
            return;
        }
        let list = s["hooks"][event].as_array().cloned().unwrap_or_default();
        let mut list = list;
        list.push(json!({ "hooks": [{ "type": "command", "command": cmd, "timeout": timeout }] }));
        s["hooks"][event] = Value::Array(list);
    };
    for (event, args, timeout) in HOOKS {
        add(settings, event, format!("{exe} {args}"), timeout);
    }
}

fn claude_settings_path() -> Result<PathBuf> {
    if let Ok(d) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(d).join("settings.json"));
    }
    Ok(dirs::home_dir()
        .context("no home directory")?
        .join(".claude")
        .join("settings.json"))
}

fn read_settings(p: &Path) -> Result<Value> {
    if !p.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON; fix it first", p.display()))
}

pub(crate) fn claude_cmd() -> Command {
    // On Windows `claude` is a .cmd shim, which needs the shell to run.
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "claude"]);
        c
    } else {
        Command::new("claude")
    }
}

fn mcp_registered() -> Result<bool> {
    let out = claude_cmd()
        .args(["mcp", "get", "davra"])
        .output()
        .context("Claude Code (`claude`) was not found on PATH; install it first")?;
    Ok(out.status.success())
}

pub fn plan() -> Result<Plan> {
    let exe = exe_string(&std::env::current_exe()?);
    let settings_path = claude_settings_path()?;
    let settings = read_settings(&settings_path)?;
    Ok(Plan {
        missing: HOOKS
            .iter()
            .map(|h| h.0)
            .filter(|e| !has_davra_hook(&settings, e))
            .collect(),
        mcp_present: mcp_registered()?,
        settings_path,
        exe,
    })
}

fn backup(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let dir = config::dir()?.join("backups");
    std::fs::create_dir_all(&dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let to = dir.join(format!("{name}.{stamp}"));
    std::fs::copy(path, &to).with_context(|| format!("backing up {}", path.display()))?;
    Ok(Some(to))
}

pub fn run(dry_run: bool) -> Result<()> {
    let p = plan()?;
    println!("Davra setup for Claude Code");
    println!("  davra:    {}", p.exe);
    println!(
        "  MCP:      {}",
        if p.mcp_present {
            "already registered (kept)"
        } else {
            "+ claude mcp add --scope user davra -- <davra> mcp"
        }
    );
    let hooks = if p.missing.is_empty() {
        "already present (kept)".to_string()
    } else {
        let parts: Vec<String> = HOOKS
            .iter()
            .filter(|h| p.missing.contains(&h.0))
            .map(|(event, args, _)| format!("{event}: davra {args}"))
            .collect();
        format!("+ {} in {}", parts.join(", "), p.settings_path.display())
    };
    println!("  hooks:    {hooks}");
    println!("  Hooks only speak inside repositories with a .davra.toml.");
    let cwd = std::env::current_dir()?;
    if davra_core::project_file::find(&cwd).is_none() {
        println!(
            "\n  Note: {} has no .davra.toml, so Davra stays silent here.\n  \
             Commit one at the repository root (no secrets in it):\n    \
             hub     = \"https://your-hub\"\n    org     = \"your-org\"\n    project = \"your-project\"",
            cwd.display()
        );
    }
    if dry_run {
        println!("\n--dry-run: nothing changed.");
        return Ok(());
    }

    if !p.mcp_present {
        let status = claude_cmd()
            .args([
                "mcp", "add", "--scope", "user", "davra", "--", &p.exe, "mcp",
            ])
            .status()
            .context("running `claude mcp add`")?;
        if !status.success() {
            bail!("`claude mcp add` failed");
        }
    }
    if !p.missing.is_empty() {
        let mut settings = read_settings(&p.settings_path)?;
        if let Some(b) = backup(&p.settings_path)? {
            println!("  backup:   {}", b.display());
        }
        merge_hooks(&mut settings, &p.exe);
        if let Some(dir) = p.settings_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            &p.settings_path,
            serde_json::to_string_pretty(&settings)? + "\n",
        )?;
    }
    println!("\nDone. Open a new Claude Code session in a repo with a .davra.toml.");
    println!("For live messages (they reach the agent even while it waits), start it with:");
    println!("  davra claude");
    Ok(())
}

/// Removes what `run` added (hooks; MCP via `claude mcp remove`).
pub fn undo() -> Result<()> {
    let path = claude_settings_path()?;
    let mut settings = read_settings(&path)?;
    let mut changed = false;
    if let Some(hooks) = settings["hooks"].as_object_mut() {
        for event in HOOKS.map(|h| h.0) {
            if let Some(groups) = hooks.get_mut(event).and_then(Value::as_array_mut) {
                let before = groups.len();
                groups.retain(|g| {
                    !g["hooks"].as_array().is_some_and(|hs| {
                        hs.iter().all(|h| {
                            h["command"]
                                .as_str()
                                .is_some_and(|c| c.contains("davra") && c.contains("--hook"))
                        })
                    })
                });
                changed |= groups.len() != before;
            }
        }
        hooks.retain(|_, v| !v.as_array().is_some_and(|a| a.is_empty()));
    }
    if changed {
        if let Some(b) = backup(&path)? {
            println!("backup: {}", b.display());
        }
        std::fs::write(&path, serde_json::to_string_pretty(&settings)? + "\n")?;
        println!("removed Davra hooks from {}", path.display());
    }
    if mcp_registered().unwrap_or(false) {
        let _ = claude_cmd()
            .args(["mcp", "remove", "--scope", "user", "davra"])
            .status();
        println!("removed the davra MCP server");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_without_touching_other_hooks() {
        let mut s = json!({
            "model": "x",
            "hooks": { "StopFailure": [{ "matcher": "rate_limit", "hooks": [{ "type": "command", "command": "bash x.sh" }] }],
                       "SessionStart": [{ "hooks": [{ "type": "command", "command": "node brief.mjs" }] }] }
        });
        merge_hooks(&mut s, "C:/u/.davra/bin/davra.exe");
        assert_eq!(s["model"], "x");
        assert_eq!(s["hooks"]["StopFailure"].as_array().unwrap().len(), 1);
        assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert_eq!(
            s["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "C:/u/.davra/bin/davra.exe inbox --hook"
        );
        assert_eq!(
            s["hooks"]["Stop"][0]["hooks"][0]["command"],
            "C:/u/.davra/bin/davra.exe inbox --hook --stop"
        );
        // Idempotent.
        let once = s.clone();
        merge_hooks(&mut s, "C:/u/.davra/bin/davra.exe");
        assert_eq!(s, once);
    }

    #[test]
    fn empty_or_missing_settings() {
        let mut s = json!(null);
        merge_hooks(&mut s, "davra");
        assert!(has_davra_hook(&s, "SessionStart"));
        assert!(has_davra_hook(&s, "UserPromptSubmit"));
    }
}
