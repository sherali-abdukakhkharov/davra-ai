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

// ---- Usage (OpenTelemetry) -------------------------------------------------

/// Claude Code settings that send its metrics (counts only: tokens, cost,
/// lines, commits, active time) to the hub. They go in the repository's
/// `.claude/settings.local.json`, so only Davra repositories report, each to
/// its own hub. The token comes from the keychain via `otelHeadersHelper`.
pub struct UsagePlan {
    pub path: PathBuf,
    pub root: PathBuf,
    pub hub: String,
    pub project: String,
    pub present: bool,
}

const USAGE_ENV: [(&str, &str); 5] = [
    ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
    ("OTEL_METRICS_EXPORTER", "otlp"),
    ("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL", "http/json"),
    ("OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE", "delta"),
    ("OTEL_METRICS_INCLUDE_ACCOUNT_UUID", "false"),
];
const ENDPOINT_ENV: &str = "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT";

pub fn metrics_endpoint(hub: &str) -> String {
    format!("{}/otlp/v1/metrics", hub.trim_end_matches('/'))
}

fn helper_cmd(exe: &str, hub: &str, project: &str) -> String {
    let exe = if exe.contains(' ') {
        format!("\"{exe}\"")
    } else {
        exe.to_string()
    };
    format!("{exe} otel-headers --hub {hub} --project {project}")
}

fn has_usage(settings: &Value, hub: &str) -> bool {
    settings["env"][ENDPOINT_ENV] == metrics_endpoint(hub).as_str()
        && settings["otelHeadersHelper"]
            .as_str()
            .is_some_and(|c| c.contains("otel-headers"))
}

pub fn merge_usage(settings: &mut Value, exe: &str, hub: &str, project: &str) {
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["env"].is_object() {
        settings["env"] = json!({});
    }
    for (k, v) in USAGE_ENV {
        settings["env"][k] = json!(v);
    }
    settings["env"][ENDPOINT_ENV] = json!(metrics_endpoint(hub));
    settings["otelHeadersHelper"] = json!(helper_cmd(exe, hub, project));
}

/// Removes what `merge_usage` added; true if anything changed.
pub fn remove_usage(settings: &mut Value) -> bool {
    let ours = settings["otelHeadersHelper"]
        .as_str()
        .is_some_and(|c| c.contains("otel-headers"));
    if !ours {
        return false;
    }
    if let Some(o) = settings.as_object_mut() {
        o.remove("otelHeadersHelper");
    }
    if let Some(env) = settings["env"].as_object_mut() {
        for (k, _) in USAGE_ENV {
            env.remove(k);
        }
        env.remove(ENDPOINT_ENV);
        if env.is_empty() {
            settings.as_object_mut().map(|o| o.remove("env"));
        }
    }
    true
}

fn usage_plan(cwd: &Path) -> Result<Option<UsagePlan>> {
    let Some((file, pf)) = davra_core::project_file::find(cwd) else {
        return Ok(None);
    };
    let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let hub = config::target(cwd)?.hub;
    let path = root.join(".claude").join("settings.local.json");
    let present = has_usage(&read_settings(&path)?, &hub);
    Ok(Some(UsagePlan {
        path,
        root,
        hub,
        project: pf.project,
        present,
    }))
}

/// Keeps the local settings file out of commits (Claude Code does the same
/// when it creates the file itself).
fn exclude_from_git(root: &Path) {
    let rel = ".claude/settings.local.json";
    let ignored = Command::new("git")
        .args(["check-ignore", "-q", rel])
        .current_dir(root)
        .status();
    if !matches!(ignored, Ok(s) if s.code() == Some(1)) {
        return; // ignored already, or not a git repository
    }
    let Ok(out) = Command::new("git")
        .args(["rev-parse", "--git-path", "info/exclude"])
        .current_dir(root)
        .output()
    else {
        return;
    };
    let p = root.join(String::from_utf8_lossy(&out.stdout).trim());
    let mut text = std::fs::read_to_string(&p).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(rel);
    text.push('\n');
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&p, text);
}

fn write_settings(path: &Path, settings: &Value) -> Result<()> {
    if let Some(b) = backup(path)? {
        println!("  backup:   {}", b.display());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(settings)? + "\n")
        .with_context(|| format!("writing {}", path.display()))
}

pub fn run(dry_run: bool, usage: bool) -> Result<()> {
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
    let usage_plan = if usage { usage_plan(&cwd)? } else { None };
    match &usage_plan {
        Some(u) if u.present => println!("  usage:    already reporting to {} (kept)", u.hub),
        Some(u) => println!(
            "  usage:    + Claude Code metrics for {} -> {}\n            \
             in {} (counts only: tokens, cost, lines, commits;\n            \
             no prompts or code). Skip with --no-usage.",
            u.project,
            metrics_endpoint(&u.hub),
            u.path.display()
        ),
        None if usage => {}
        None => println!("  usage:    off (--no-usage)"),
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
        merge_hooks(&mut settings, &p.exe);
        write_settings(&p.settings_path, &settings)?;
    }
    if let Some(u) = usage_plan.filter(|u| !u.present) {
        let mut settings = read_settings(&u.path)?;
        merge_usage(&mut settings, &p.exe, &u.hub, &u.project);
        write_settings(&u.path, &settings)?;
        exclude_from_git(&u.root);
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
    if let Some((file, _)) = davra_core::project_file::find(&std::env::current_dir()?) {
        let local = file
            .parent()
            .unwrap_or(Path::new("."))
            .join(".claude")
            .join("settings.local.json");
        let mut s = read_settings(&local)?;
        if remove_usage(&mut s) {
            write_settings(&local, &s)?;
            println!("removed usage reporting from {}", local.display());
        }
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
    fn usage_settings_merge_and_undo() {
        let mut s = json!({ "env": { "FOO": "1" }, "permissions": { "allow": [] } });
        let hub = "https://davra.example.uz";
        merge_usage(&mut s, "C:/Program Files/davra.exe", hub, "mosla");
        assert!(has_usage(&s, hub));
        assert_eq!(
            s["env"]["OTEL_EXPORTER_OTLP_METRICS_ENDPOINT"],
            "https://davra.example.uz/otlp/v1/metrics"
        );
        assert_eq!(
            s["otelHeadersHelper"],
            "\"C:/Program Files/davra.exe\" otel-headers --hub https://davra.example.uz --project mosla"
        );
        assert!(!has_usage(&s, "https://other.example"));
        assert!(remove_usage(&mut s));
        assert_eq!(
            s,
            json!({ "env": { "FOO": "1" }, "permissions": { "allow": [] } })
        );
        assert!(!remove_usage(&mut s));
    }

    #[test]
    fn empty_or_missing_settings() {
        let mut s = json!(null);
        merge_hooks(&mut s, "davra");
        assert!(has_davra_hook(&s, "SessionStart"));
        assert!(has_davra_hook(&s, "UserPromptSubmit"));
    }
}
