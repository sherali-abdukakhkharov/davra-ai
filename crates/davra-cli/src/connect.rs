//! What connecting Davra changes on this machine, as a plan a person can
//! review first: file edits (shown as diffs, backed up before writing) and
//! commands. `davra setup` prints and applies plans; Davra Desktop shows them.
//!
//! - Claude Code: MCP server (`claude mcp add --scope user`) plus hooks in
//!   `~/.claude/settings.json`.
//! - Cursor: MCP server in `~/.cursor/mcp.json`.
//! - Codex: MCP server in `~/.codex/config.toml` (`$CODEX_HOME`).
//! - A repository: `.davra.toml`, and Claude Code usage reporting in the
//!   repository's `.claude/settings.local.json` (kept out of git).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::config;

/// One change.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Write {
        title: String,
        path: PathBuf,
        /// None: the file does not exist yet.
        before: Option<String>,
        after: String,
    },
    Run {
        title: String,
        program: String,
        args: Vec<String>,
    },
}

impl Step {
    pub fn title(&self) -> &str {
        match self {
            Step::Write { title, .. } | Step::Run { title, .. } => title,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Step::Write { path, .. } => Some(path),
            Step::Run { .. } => None,
        }
    }

    pub fn is_new_file(&self) -> bool {
        matches!(self, Step::Write { before: None, .. })
    }

    /// A unified diff for a file, the command line for a command.
    pub fn diff(&self) -> String {
        match self {
            Step::Write {
                path,
                before,
                after,
                ..
            } => {
                let name = path.display().to_string();
                let old = before.as_deref().unwrap_or("");
                similar::TextDiff::from_lines(old, after.as_str())
                    .unified_diff()
                    .context_radius(3)
                    .header(if before.is_some() { &name } else { "/dev/null" }, &name)
                    .to_string()
            }
            Step::Run { program, args, .. } => {
                let mut line = program.clone();
                for a in args {
                    line.push(' ');
                    if a.contains(' ') {
                        line.push_str(&format!("\"{a}\""));
                    } else {
                        line.push_str(a);
                    }
                }
                line
            }
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub notes: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    fn write(&mut self, title: &str, path: PathBuf, before: Option<String>, after: String) {
        if before.as_deref() != Some(after.as_str()) {
            self.steps.push(Step::Write {
                title: title.into(),
                path,
                before,
                after,
            });
        }
    }

    fn run(&mut self, title: &str, program: &str, args: &[&str]) {
        self.steps.push(Step::Run {
            title: title.into(),
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        });
    }

    fn note(&mut self, n: &str) {
        self.notes.push(n.into());
    }

    fn extend(&mut self, other: Plan) {
        self.steps.extend(other.steps);
        self.notes.extend(other.notes);
    }
}

/// A command that never flashes a console window (Davra Desktop is a GUI app).
/// On Windows `claude`, `codex` and friends are .cmd shims, so they run via cmd.
pub fn command(program: &str) -> Command {
    let c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", program]);
        c
    } else {
        Command::new(program)
    };
    #[cfg(windows)]
    let mut c = c;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Applies a plan: refuses files that changed since it was made, backs every
/// existing file up to `~/.davra/backups/`, writes, runs. Returns the backups.
pub fn apply(plan: &Plan) -> Result<Vec<PathBuf>> {
    let mut backups = Vec::new();
    for step in &plan.steps {
        if let Step::Write { path, before, .. } = step {
            if read_opt(path)? != *before {
                bail!(
                    "{} changed since the plan was made; review it again",
                    path.display()
                );
            }
        }
    }
    for step in &plan.steps {
        match step {
            Step::Write { path, after, .. } => {
                if let Some(b) = backup(path)? {
                    backups.push(b);
                }
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)
                        .with_context(|| format!("creating {}", dir.display()))?;
                }
                std::fs::write(path, after)
                    .with_context(|| format!("writing {}", path.display()))?;
            }
            Step::Run {
                program,
                args,
                title,
            } => {
                let out = command(program)
                    .args(args)
                    .output()
                    .with_context(|| format!("{program} was not found on PATH"))?;
                if !out.status.success() {
                    bail!(
                        "{title}: `{}` failed: {}",
                        step.diff(),
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                }
            }
        }
    }
    Ok(backups)
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
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
    let to = dir.join(format!("{name}.{stamp}"));
    std::fs::copy(path, &to).with_context(|| format!("backing up {}", path.display()))?;
    Ok(Some(to))
}

pub fn read_opt(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn parse_json(path: &Path, text: Option<&str>) -> Result<Value> {
    match text.map(str::trim) {
        None | Some("") => Ok(json!({})),
        Some(t) => serde_json::from_str(t)
            .with_context(|| format!("{} is not valid JSON; fix it first", path.display())),
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).expect("JSON values serialize") + "\n"
}

/// Plans a JSON settings edit; no step when the content means the same.
fn json_edit(
    plan: &mut Plan,
    title: &str,
    path: PathBuf,
    edit: impl FnOnce(&mut Value),
) -> Result<()> {
    let text = read_opt(&path)?;
    let old = parse_json(&path, text.as_deref())?;
    let mut new = old.clone();
    edit(&mut new);
    if new != old {
        plan.write(title, path, text, pretty(&new));
    }
    Ok(())
}

/// Forward slashes work for every client on every OS and need no escaping.
pub fn exe_string(exe: &Path) -> String {
    exe.display().to_string().replace('\\', "/")
}

fn home() -> Result<PathBuf> {
    dirs::home_dir().context("no home directory")
}

// ---- Clients ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientId {
    Claude,
    Cursor,
    Codex,
}

impl ClientId {
    pub const ALL: [ClientId; 3] = [ClientId::Claude, ClientId::Cursor, ClientId::Codex];

    pub fn as_str(self) -> &'static str {
        match self {
            ClientId::Claude => "claude",
            ClientId::Cursor => "cursor",
            ClientId::Codex => "codex",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ClientId::Claude => "Claude Code",
            ClientId::Cursor => "Cursor",
            ClientId::Codex => "Codex",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "claude" => Ok(ClientId::Claude),
            "cursor" => Ok(ClientId::Cursor),
            "codex" => Ok(ClientId::Codex),
            _ => bail!("unknown client '{s}' (claude, cursor, codex)"),
        }
    }
}

/// Where a client stands on this machine.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientState {
    pub id: ClientId,
    pub detected: bool,
    pub connected: bool,
    pub partial: bool,
    pub detail: String,
}

pub fn connect(client: ClientId, exe: &str) -> Result<Plan> {
    match client {
        ClientId::Claude => claude_connect(exe),
        ClientId::Cursor => cursor_connect(exe),
        ClientId::Codex => codex_connect(exe),
    }
}

pub fn disconnect(client: ClientId) -> Result<Plan> {
    match client {
        ClientId::Claude => claude_disconnect(),
        ClientId::Cursor => cursor_disconnect(),
        ClientId::Codex => codex_disconnect(),
    }
}

pub fn state(client: ClientId, exe: &str) -> ClientState {
    let s = |detected, connected, partial, detail: &str| ClientState {
        id: client,
        detected,
        connected,
        partial,
        detail: detail.into(),
    };
    match client {
        ClientId::Claude => {
            let mcp = match mcp_command() {
                Ok(m) => m,
                Err(_) => {
                    return s(
                        claude_settings_path().is_ok_and(|p| p.parent().is_some_and(Path::exists)),
                        false,
                        false,
                        "`claude` is not on PATH",
                    )
                }
            };
            let hooks = claude_settings_path()
                .and_then(|p| parse_json(&p, read_opt(&p)?.as_deref()))
                .map(|v| HOOKS.iter().filter(|h| has_davra_hook(&v, h.0)).count())
                .unwrap_or(0);
            let mut plan = Plan::default();
            if let Err(e) = claude_plan(&mut plan, exe, mcp.as_deref()) {
                return s(true, false, false, &format!("{e:#}"));
            }
            let stale = mcp.as_deref().is_some_and(|c| !same_exe(c, exe))
                || (hooks == HOOKS.len() && !plan.is_empty());
            match (mcp.is_some(), hooks) {
                _ if plan.is_empty() => s(true, true, false, "MCP server and hooks"),
                _ if stale => s(true, false, true, "set up for another davra; update it"),
                (true, _) => s(true, false, true, "MCP server; hooks missing"),
                (false, 0) => s(true, false, false, "not connected"),
                (false, _) => s(true, false, true, "hooks; MCP server missing"),
            }
        }
        ClientId::Cursor | ClientId::Codex => {
            let dir = match client {
                ClientId::Cursor => home().map(|h| h.join(".cursor")),
                _ => codex_config_path()
                    .map(|p| p.parent().map(Path::to_path_buf).unwrap_or_default()),
            };
            let detected = dir.is_ok_and(|d| d.exists());
            match connect(client, exe) {
                Ok(p) if p.is_empty() => s(true, true, false, "MCP server"),
                Ok(_) if detected => s(true, false, false, "not connected"),
                Ok(_) => s(false, false, false, "not found on this machine"),
                Err(e) => s(detected, false, false, &format!("{e:#}")),
            }
        }
    }
}

// Claude Code

/// Event, `davra` arguments, timeout in seconds. The Stop hook keeps a
/// finished agent going when new messages are waiting.
pub const HOOKS: [(&str, &str, u32); 3] = [
    ("SessionStart", "brief --hook", 20),
    ("UserPromptSubmit", "inbox --hook", 10),
    ("Stop", "inbox --hook --stop", 10),
];

pub fn has_davra_hook(settings: &Value, event: &str) -> bool {
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

/// Adds the hooks to a settings document, leaving everything else alone.
pub fn merge_hooks(settings: &mut Value, exe: &str) {
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    // Davra hooks left pointing at another davra (an older install elsewhere,
    // or one that is gone) move to this one.
    for (event, args, _) in HOOKS {
        let suffix = format!(" {args}");
        for g in settings["hooks"]
            .get_mut(event)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            for h in g["hooks"].as_array_mut().into_iter().flatten() {
                let stale = h["command"]
                    .as_str()
                    .and_then(|c| c.strip_suffix(&suffix))
                    .is_some_and(|prefix| {
                        prefix.to_lowercase().contains("davra") && !same_exe(prefix, exe)
                    });
                if stale {
                    h["command"] = json!(format!("{exe} {args}"));
                }
            }
        }
    }
    for (event, args, timeout) in HOOKS {
        if has_davra_hook(settings, event) {
            continue;
        }
        let mut list = settings["hooks"][event]
            .as_array()
            .cloned()
            .unwrap_or_default();
        list.push(json!({ "hooks": [{ "type": "command", "command": format!("{exe} {args}"), "timeout": timeout }] }));
        settings["hooks"][event] = Value::Array(list);
    }
}

pub fn remove_hooks(settings: &mut Value) {
    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        for (event, _, _) in HOOKS {
            if let Some(groups) = hooks.get_mut(event).and_then(Value::as_array_mut) {
                groups.retain(|g| {
                    !g["hooks"].as_array().is_some_and(|hs| {
                        hs.iter().all(|h| {
                            h["command"]
                                .as_str()
                                .is_some_and(|c| c.contains("davra") && c.contains("--hook"))
                        })
                    })
                });
            }
        }
        hooks.retain(|_, v| !v.as_array().is_some_and(|a| a.is_empty()));
    }
    if settings["hooks"].as_object().is_some_and(|h| h.is_empty()) {
        settings.as_object_mut().map(|o| o.remove("hooks"));
    }
}

pub fn claude_settings_path() -> Result<PathBuf> {
    if let Ok(d) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(d).join("settings.json"));
    }
    Ok(home()?.join(".claude").join("settings.json"))
}

/// Errors when Claude Code is not installed.
pub fn mcp_registered() -> Result<bool> {
    Ok(mcp_command()?.is_some())
}

/// The command Claude Code runs for the davra MCP server; None when it is not
/// registered. Errors when Claude Code is not installed.
pub fn mcp_command() -> Result<Option<String>> {
    let out = command("claude")
        .args(["mcp", "get", "davra"])
        .output()
        .context("Claude Code (`claude`) was not found on PATH; install it first")?;
    // cmd /C reports a missing program as exit code 9009 / "not recognized".
    let err = String::from_utf8_lossy(&out.stderr);
    if out.status.code() == Some(9009)
        || err.contains("not recognized")
        || out.status.code() == Some(127)
    {
        bail!("Claude Code (`claude`) was not found on PATH; install it first");
    }
    if !out.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(Some(
        text.lines()
            .find_map(|l| l.trim().strip_prefix("Command:"))
            .map(|c| c.trim().to_string())
            .unwrap_or_default(),
    ))
}

/// Whether two spellings name the same davra executable.
pub fn same_exe(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let s = s.trim().trim_matches('"').replace('\\', "/");
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    norm(a) == norm(b)
}

/// Hooks and MCP server for `exe`, given what `claude mcp get davra` reported.
fn claude_plan(plan: &mut Plan, exe: &str, mcp: Option<&str>) -> Result<()> {
    json_edit(
        plan,
        "Claude Code hooks: brief at session start, inbox on each prompt and when a turn ends",
        claude_settings_path()?,
        |s| merge_hooks(s, exe),
    )?;
    match mcp {
        None => plan.run(
            "Register the davra MCP server for all projects",
            "claude",
            &["mcp", "add", "--scope", "user", "davra", "--", exe, "mcp"],
        ),
        Some(current) if !same_exe(current, exe) => {
            plan.run(
                &format!("Remove the davra MCP server that runs {current}"),
                "claude",
                &["mcp", "remove", "--scope", "user", "davra"],
            );
            plan.run(
                "Register it again with this davra",
                "claude",
                &["mcp", "add", "--scope", "user", "davra", "--", exe, "mcp"],
            );
        }
        Some(_) => {}
    }
    Ok(())
}

fn claude_connect(exe: &str) -> Result<Plan> {
    let mut plan = Plan::default();
    claude_plan(&mut plan, exe, mcp_command()?.as_deref())?;
    plan.note("Hooks only speak inside repositories with a .davra.toml.");
    plan.note("For live messages while the agent works, start Claude Code with `davra claude`.");
    Ok(plan)
}

fn claude_disconnect() -> Result<Plan> {
    let mut plan = Plan::default();
    json_edit(
        &mut plan,
        "Remove the Davra hooks",
        claude_settings_path()?,
        remove_hooks,
    )?;
    if mcp_registered().unwrap_or(false) {
        plan.run(
            "Remove the davra MCP server",
            "claude",
            &["mcp", "remove", "--scope", "user", "davra"],
        );
    }
    Ok(plan)
}

// Cursor

pub fn cursor_mcp_path() -> Result<PathBuf> {
    Ok(home()?.join(".cursor").join("mcp.json"))
}

fn mcp_entry(exe: &str) -> Value {
    json!({ "command": exe, "args": ["mcp"] })
}

fn cursor_connect(exe: &str) -> Result<Plan> {
    let mut plan = Plan::default();
    json_edit(
        &mut plan,
        "Add the davra MCP server to Cursor",
        cursor_mcp_path()?,
        |s| {
            if !s.is_object() {
                *s = json!({});
            }
            if !s["mcpServers"].is_object() {
                s["mcpServers"] = json!({});
            }
            s["mcpServers"]["davra"] = mcp_entry(exe);
        },
    )?;
    plan.note(
        "Cursor gets the Davra tools; ask it to call team_brief and inbox at the start of a task.",
    );
    Ok(plan)
}

fn cursor_disconnect() -> Result<Plan> {
    let mut plan = Plan::default();
    json_edit(
        &mut plan,
        "Remove the davra MCP server from Cursor",
        cursor_mcp_path()?,
        |s| {
            if let Some(m) = s.get_mut("mcpServers").and_then(Value::as_object_mut) {
                m.remove("davra");
            }
        },
    )?;
    Ok(plan)
}

// Codex

pub fn codex_config_path() -> Result<PathBuf> {
    if let Ok(d) = std::env::var("CODEX_HOME") {
        return Ok(PathBuf::from(d).join("config.toml"));
    }
    Ok(home()?.join(".codex").join("config.toml"))
}

fn toml_doc(path: &Path, text: Option<&str>) -> Result<toml_edit::DocumentMut> {
    text.unwrap_or("")
        .parse()
        .with_context(|| format!("{} is not valid TOML; fix it first", path.display()))
}

/// `[mcp_servers.davra]` with this exe, keeping the rest of the file as it is.
pub fn codex_with_davra(doc: &mut toml_edit::DocumentMut, exe: &str) -> Result<()> {
    use toml_edit::{value, Array, Item, Table};
    if doc.get("mcp_servers").is_none() {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert("mcp_servers", Item::Table(t));
    }
    let servers = doc["mcp_servers"]
        .as_table_mut()
        .context("mcp_servers in the Codex config is not a table")?;
    let current = servers.get("davra").and_then(Item::as_table);
    let same = current.is_some_and(|t| {
        t.get("command").and_then(Item::as_str) == Some(exe)
            && t.get("args")
                .and_then(Item::as_array)
                .is_some_and(|a| a.len() == 1 && a.get(0).and_then(|v| v.as_str()) == Some("mcp"))
    });
    if !same {
        let mut t = Table::new();
        t.insert("command", value(exe));
        let mut args = Array::new();
        args.push("mcp");
        t.insert("args", value(args));
        servers.insert("davra", Item::Table(t));
    }
    Ok(())
}

fn codex_connect(exe: &str) -> Result<Plan> {
    let mut plan = Plan::default();
    let path = codex_config_path()?;
    let text = read_opt(&path)?;
    let mut doc = toml_doc(&path, text.as_deref())?;
    codex_with_davra(&mut doc, exe)?;
    plan.write(
        "Add the davra MCP server to Codex",
        path,
        text,
        doc.to_string(),
    );
    plan.note(
        "Codex gets the Davra tools; ask it to call team_brief and inbox at the start of a task.",
    );
    Ok(plan)
}

fn codex_disconnect() -> Result<Plan> {
    let mut plan = Plan::default();
    let path = codex_config_path()?;
    let Some(text) = read_opt(&path)? else {
        return Ok(plan);
    };
    let mut doc = toml_doc(&path, Some(&text))?;
    if let Some(t) = doc.get_mut("mcp_servers").and_then(|i| i.as_table_mut()) {
        t.remove("davra");
        if t.is_empty() {
            doc.remove("mcp_servers");
        }
    }
    plan.write(
        "Remove the davra MCP server from Codex",
        path,
        Some(text),
        doc.to_string(),
    );
    Ok(plan)
}

// ---- Repositories and usage ------------------------------------------------

const USAGE_ENV: [(&str, &str); 5] = [
    ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
    ("OTEL_METRICS_EXPORTER", "otlp"),
    ("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL", "http/json"),
    ("OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE", "delta"),
    ("OTEL_METRICS_INCLUDE_ACCOUNT_UUID", "false"),
];
const ENDPOINT_ENV: &str = "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT";
/// Where davra 0.2.0 wrote usage settings; Claude Code ignores them there.
const LOCAL_SETTINGS: &str = ".claude/settings.local.json";

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

/// The settings `davra claude` hands Claude Code (`--settings <file>`) in a
/// Davra repository: send its metrics (counts only) to the hub, with the
/// token read from the keychain by `otelHeadersHelper`. They can't live in
/// the repository's own .claude/settings*.json: Claude Code ignores telemetry
/// variables there, so that no repository can send your usage anywhere.
pub fn usage_settings(exe: &str, hub: &str, project: &str) -> Value {
    let mut env = serde_json::Map::new();
    for (k, v) in USAGE_ENV {
        env.insert(k.into(), json!(v));
    }
    env.insert(ENDPOINT_ENV.into(), json!(metrics_endpoint(hub)));
    json!({ "env": env, "otelHeadersHelper": helper_cmd(exe, hub, project) })
}

/// Removes the usage settings davra 0.2.0 wrote into settings.local.json.
pub fn remove_usage(settings: &mut Value) {
    let ours = settings["otelHeadersHelper"]
        .as_str()
        .is_some_and(|c| c.contains("otel-headers"));
    if !ours {
        return;
    }
    if let Some(o) = settings.as_object_mut() {
        o.remove("otelHeadersHelper");
    }
    if let Some(env) = settings.get_mut("env").and_then(Value::as_object_mut) {
        for (k, _) in USAGE_ENV {
            env.remove(k);
        }
        env.remove(ENDPOINT_ENV);
        if env.is_empty() {
            settings.as_object_mut().map(|o| o.remove("env"));
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    same_exe(&a.display().to_string(), &b.display().to_string())
}

/// Whether `davra claude` reports usage in `root` (on unless turned off).
pub fn usage_on(root: &Path) -> bool {
    config::Config::load()
        .map(|c| !c.usage_off.iter().any(|p| same_path(p, root)))
        .unwrap_or(true)
}

/// Turns usage reporting for `root` on or off (kept in ~/.davra/config.toml).
pub fn set_usage(root: &Path, on: bool) -> Result<()> {
    let mut cfg = config::Config::load()?;
    let before = cfg.usage_off.len();
    cfg.usage_off.retain(|p| !same_path(p, root));
    if !on {
        cfg.usage_off.push(root.to_path_buf());
    }
    if on && cfg.usage_off.len() == before {
        return Ok(());
    }
    cfg.save()
}

/// A linked repository as Davra Desktop lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct RepoState {
    pub path: PathBuf,
    pub project: Option<String>,
    pub hub: Option<String>,
    pub usage: bool,
    pub has_file: bool,
}

pub fn repo_state(path: &Path, fallback_hub: Option<&str>) -> RepoState {
    let file = path.join(davra_core::project_file::FILE_NAME);
    let pf = read_opt(&file)
        .ok()
        .flatten()
        .and_then(|t| davra_core::project_file::ProjectFile::parse(&t).ok());
    let hub = pf
        .as_ref()
        .and_then(|p| p.hub.clone())
        .or(fallback_hub.map(String::from));
    RepoState {
        path: path.to_path_buf(),
        has_file: pf.is_some(),
        project: pf.map(|p| p.project),
        hub,
        usage: usage_on(path),
    }
}

/// Links `root` to a project: `.davra.toml` if missing. Also removes usage
/// settings an older davra left in `.claude/settings.local.json`.
pub fn repo(root: &Path, hub: &str, org: Option<&str>, project: &str) -> Result<Plan> {
    use davra_core::project_file::{ProjectFile, FILE_NAME};
    let mut plan = Plan::default();
    if !root.is_dir() {
        bail!("{} is not a folder", root.display());
    }
    let file = root.join(FILE_NAME);
    match read_opt(&file)? {
        Some(text) => {
            let pf = ProjectFile::parse(&text)
                .with_context(|| format!("{} is not valid; fix it first", file.display()))?;
            if pf.project != project {
                bail!(
                    "{} already links this repository to '{}'",
                    file.display(),
                    pf.project
                );
            }
        }
        None => {
            let mut text = format!("# Davra: which project this repository belongs to (no secrets).\nhub     = \"{hub}\"\n");
            if let Some(o) = org {
                text.push_str(&format!("org     = \"{o}\"\n"));
            }
            text.push_str(&format!("project = \"{project}\"\n"));
            plan.write("Link this repository to the project", file, None, text);
            plan.note("Commit .davra.toml so your teammates' agents find the project too.");
        }
    }
    let local = root.join(LOCAL_SETTINGS);
    if local.exists() {
        json_edit(
            &mut plan,
            "Remove old usage settings (Claude Code ignores them in a repository)",
            local,
            remove_usage,
        )?;
    }
    Ok(plan)
}

/// Everything `davra setup <client>` does in one plan: the client, plus (for
/// Claude Code) tidying the current repository when it has a .davra.toml.
pub fn setup_plan(client: ClientId, exe: &str, cwd: &Path) -> Result<Plan> {
    let mut plan = connect(client, exe)?;
    if client == ClientId::Claude {
        if let Some((file, pf)) = davra_core::project_file::find(cwd) {
            let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
            let hub = config::target(cwd)?.hub;
            plan.extend(repo(&root, &hub, pf.org.as_deref(), &pf.project)?);
        }
    }
    Ok(plan)
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
            s["hooks"]["Stop"][0]["hooks"][0]["command"],
            "C:/u/.davra/bin/davra.exe inbox --hook --stop"
        );
        let once = s.clone();
        merge_hooks(&mut s, "C:/u/.davra/bin/davra.exe");
        assert_eq!(s, once, "idempotent");
        // A reinstall elsewhere moves Davra's hooks along, and only Davra's.
        merge_hooks(&mut s, "C:/u/AppData/Local/Davra/bin/davra.exe");
        assert_eq!(
            s["hooks"]["Stop"][0]["hooks"][0]["command"],
            "C:/u/AppData/Local/Davra/bin/davra.exe inbox --hook --stop"
        );
        assert_eq!(
            s["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "node brief.mjs"
        );
        assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert!(same_exe("C:\\u\\davra.exe", "\"C:/u/davra.exe\""));
        // Removing from a file that has nothing of ours leaves it exactly as it was
        // (no `"hooks": null` from indexing a missing key).
        let mut plain = json!({ "model": "x" });
        remove_hooks(&mut plain);
        remove_usage(&mut plain);
        assert_eq!(plain, json!({ "model": "x" }));
        remove_hooks(&mut s);
        assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert!(s["hooks"].get("Stop").is_none());
    }

    #[test]
    fn usage_settings_shape_and_old_ones_removed() {
        let s = usage_settings(
            "C:/Program Files/davra.exe",
            "https://davra.example.uz",
            "atlas",
        );
        assert_eq!(s["env"]["CLAUDE_CODE_ENABLE_TELEMETRY"], "1");
        assert_eq!(
            s["env"]["OTEL_EXPORTER_OTLP_METRICS_ENDPOINT"],
            "https://davra.example.uz/otlp/v1/metrics"
        );
        assert_eq!(
            s["otelHeadersHelper"],
            "\"C:/Program Files/davra.exe\" otel-headers --hub https://davra.example.uz --project atlas"
        );
        // What davra 0.2.0 merged into settings.local.json comes out cleanly.
        let original = json!({ "env": { "FOO": "1" }, "permissions": { "allow": [] } });
        let mut local = original.clone();
        for (k, v) in s["env"].as_object().unwrap() {
            local["env"][k] = v.clone();
        }
        local["otelHeadersHelper"] = s["otelHeadersHelper"].clone();
        remove_usage(&mut local);
        assert_eq!(local, original);
    }

    #[test]
    fn codex_toml_keeps_the_rest_of_the_file() {
        let text = "# my codex config\nmodel = \"gpt-x\"\n\n[mcp_servers.other]\ncommand = \"npx\"\nargs = [\"-y\", \"other\"]\n";
        let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
        codex_with_davra(&mut doc, "/home/g/.davra/bin/davra").unwrap();
        let out = doc.to_string();
        assert!(out.starts_with(text), "existing content untouched:\n{out}");
        assert!(
            out.contains(
                "[mcp_servers.davra]\ncommand = \"/home/g/.davra/bin/davra\"\nargs = [\"mcp\"]\n"
            ),
            "{out}"
        );
        let again = out.clone();
        let mut doc: toml_edit::DocumentMut = again.parse().unwrap();
        codex_with_davra(&mut doc, "/home/g/.davra/bin/davra").unwrap();
        assert_eq!(doc.to_string(), out, "idempotent");
        let mut empty = toml_edit::DocumentMut::new();
        codex_with_davra(&mut empty, "davra").unwrap();
        assert_eq!(
            empty.to_string(),
            "[mcp_servers.davra]\ncommand = \"davra\"\nargs = [\"mcp\"]\n"
        );
    }

    #[test]
    fn plans_apply_with_backups_and_refuse_stale_files() {
        let dir = std::env::temp_dir().join(format!("davra-connect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::env::set_var("DAVRA_HOME", dir.join("home"));
        // Old usage settings in settings.local.json, next to the person's own.
        let old = json!({ "permissions": { "allow": ["Bash(ls)"] }, "otelHeadersHelper": "davra otel-headers --hub h --project atlas" });
        std::fs::write(dir.join(LOCAL_SETTINGS), pretty(&old)).unwrap();
        let plan = repo(&dir, "https://h.example", Some("o"), "atlas").unwrap();
        let titles: Vec<&str> = plan.steps.iter().map(Step::title).collect();
        assert_eq!(titles.len(), 2, "{titles:?}");
        assert!(plan.steps[0].is_new_file());
        assert!(plan.steps[0].diff().contains("+project = \"atlas\""));
        assert!(plan.steps[1].diff().contains("-  \"otelHeadersHelper\""));
        // A file changed after the plan was made is refused.
        std::fs::write(dir.join(LOCAL_SETTINGS), "{\"changed\": true}\n").unwrap();
        assert!(apply(&plan)
            .unwrap_err()
            .to_string()
            .contains("changed since"));
        std::fs::write(dir.join(LOCAL_SETTINGS), pretty(&old)).unwrap();
        let plan = repo(&dir, "https://h.example", Some("o"), "atlas").unwrap();
        assert_eq!(
            apply(&plan).unwrap().len(),
            1,
            "settings.local.json backed up"
        );
        let left: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(LOCAL_SETTINGS)).unwrap())
                .unwrap();
        assert_eq!(left, json!({ "permissions": { "allow": ["Bash(ls)"] } }));
        // Linked already: nothing more to do; another project is refused.
        assert!(repo(&dir, "https://h.example", Some("o"), "atlas")
            .unwrap()
            .is_empty());
        assert!(repo(&dir, "https://h.example", None, "other").is_err());
        // Usage is on unless turned off for this repository.
        let st = repo_state(&dir, None);
        assert_eq!(st.project.as_deref(), Some("atlas"));
        assert!(st.usage && st.has_file);
        set_usage(&dir, false).unwrap();
        assert!(!repo_state(&dir, None).usage);
        set_usage(&dir, true).unwrap();
        assert!(repo_state(&dir, None).usage);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
