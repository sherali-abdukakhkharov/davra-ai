use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use davra_core::api::PostRequest;
use davra_core::format;
use davra_core::project_file;

use davra_cli::client::Client;
use davra_cli::{config, handoffs, mcp, setup};

#[derive(Parser)]
#[command(
    name = "davra",
    version,
    about = "Connects your AI coding agent to your team's Davra hub"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Save the hub URL and store a token in the OS keychain.
    Login {
        #[arg(long)]
        hub: String,
        /// Read the token from stdin instead of prompting (for scripts).
        #[arg(long)]
        token_stdin: bool,
        /// Default project when a repository has no .davra.toml.
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove the stored token for the configured hub.
    Logout,
    /// Show who you are on the hub and which project this directory maps to.
    Whoami,
    /// Run the MCP server on stdio (started by Claude Code, Cursor, Codex).
    Mcp,
    /// Print the session brief. Used by the SessionStart hook.
    Brief {
        /// Hook mode: read the hook's JSON from stdin, never fail the session.
        #[arg(long)]
        hook: bool,
        #[arg(long)]
        project: Option<String>,
    },
    /// Print unread messages addressed to you.
    Inbox {
        /// Hook mode: print nothing when empty, mark printed messages as read.
        #[arg(long)]
        hook: bool,
        /// With --hook, as a Stop hook: new messages keep the agent going
        /// instead of waiting for the next prompt.
        #[arg(long, requires = "hook")]
        stop: bool,
        #[arg(long)]
        project: Option<String>,
    },
    /// Start Claude Code with live Davra messages (loads davra as a development channel).
    Claude {
        /// Passed on to `claude`, e.g. --resume.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Post a message to the room.
    Post {
        text: String,
        #[arg(long)]
        reply_to: Option<i64>,
        #[arg(long)]
        project: Option<String>,
    },
    /// Search the room's history.
    Search {
        query: String,
        #[arg(short, long, default_value_t = 5)]
        k: i64,
        #[arg(long)]
        project: Option<String>,
    },
    /// Print the commands that connect Claude Code to Davra.
    /// Answer a question one of your agents asked you (ask_owner).
    Answer { id: i64, answer: String },
    /// Handoff files in this repository (see handoffs in .davra.toml).
    #[command(subcommand)]
    Handoffs(HandoffsCmd),
    /// Connect Claude Code, Cursor or Codex on this machine (reviewable, with backups).
    Setup {
        #[arg(value_parser = ["claude", "cursor", "codex"])]
        client: String,
        /// Show what would change, change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Remove the Davra hooks and MCP server again.
        #[arg(long, conflicts_with = "dry_run")]
        undo: bool,
        /// Don't report Claude Code usage (tokens, cost, lines) for this repository.
        #[arg(long)]
        no_usage: bool,
    },
    /// Print the OpenTelemetry headers (agent token, project) for Claude Code's
    /// otelHeadersHelper. Prints {} when not logged in, so a session never breaks.
    #[command(hide = true)]
    OtelHeaders {
        #[arg(long)]
        hub: String,
        #[arg(long)]
        project: String,
    },
}

#[derive(Subcommand)]
enum HandoffsCmd {
    /// Send changed handoff files to the hub. The first sync imports quietly.
    Sync {
        /// Announce new and closed handoffs in the room even on a first sync.
        #[arg(long)]
        announce: bool,
        /// Never announce (re-import).
        #[arg(long, conflicts_with = "announce")]
        quiet: bool,
    },
    /// List handoffs (open ones for you by default).
    List {
        #[arg(long, default_value = "open")]
        status: String,
        /// Show everyone's, not just yours.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        project: Option<String>,
    },
}

/// Hooks receive JSON on stdin; we need `cwd` from it.
fn hook_cwd() -> PathBuf {
    hook_input().0
}

/// The hook's working directory and its whole stdin document (Null if none).
fn hook_input() -> (PathBuf, serde_json::Value) {
    let fallback = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if std::io::stdin().is_terminal() {
        return (fallback, serde_json::Value::Null);
    }
    let mut buf = String::new();
    if std::io::stdin()
        .take(1 << 20)
        .read_to_string(&mut buf)
        .is_err()
    {
        return (fallback, serde_json::Value::Null);
    }
    let v = serde_json::from_str::<serde_json::Value>(&buf).unwrap_or_default();
    let cwd = v["cwd"].as_str().map(PathBuf::from).unwrap_or(fallback);
    (cwd, v)
}

fn client_for(cwd: &std::path::Path, timeout: Duration) -> Result<(Client, config::Target)> {
    let target = config::target(cwd)?;
    let token = config::load_token(&target.hub)?;
    Ok((Client::new(&target.hub, token, timeout)?, target))
}

async fn brief(cwd: PathBuf, project: Option<String>, timeout: Duration) -> Result<String> {
    let (client, target) = client_for(&cwd, timeout)?;
    // Keep the hub's view of the repo's handoff files current before briefing.
    if let Some(repo) = handoffs::repo(&cwd) {
        if let Err(e) = handoffs::sync(&client, &target.hub, &repo, None).await {
            eprintln!("davra: handoff sync skipped: {e:#}");
        }
    }
    let p = client
        .resolve_project(project.as_deref().or(target.project.as_deref()))
        .await?;
    Ok(format::brief_text(&client.brief(&p).await?))
}

async fn inbox(
    cwd: PathBuf,
    project: Option<String>,
    hook: bool,
    timeout: Duration,
) -> Result<String> {
    let (client, target) = client_for(&cwd, timeout)?;
    let p = client
        .resolve_project(project.as_deref().or(target.project.as_deref()))
        .await?;
    let i = client.inbox(&p, 20).await?;
    let text = format::inbox_text(&i, hook);
    // In hook mode the text goes straight into the agent's context, so the
    // messages count as delivered.
    if hook {
        if let Some(last) = i.messages.last() {
            client.ack(&p, last.id).await?;
        }
    }
    Ok(text)
}

fn read_token(token_stdin: bool) -> Result<String> {
    let mut token = String::new();
    if token_stdin || !std::io::stdin().is_terminal() {
        std::io::stdin().read_to_string(&mut token)?;
    } else {
        eprint!("Paste your Davra token (input is not hidden yet): ");
        std::io::stdin().read_line(&mut token)?;
    }
    let token = token.trim().to_string();
    if !token.starts_with("dvr_") {
        bail!("that does not look like a Davra token (expected dvr_...)");
    }
    Ok(token)
}

async fn run(cli: Cli) -> Result<()> {
    let cwd = std::env::current_dir().context("no current directory")?;
    match cli.cmd {
        Cmd::Login {
            hub,
            token_stdin,
            project,
        } => {
            let hub = config::normalize_hub(&hub)?;
            let token = read_token(token_stdin)?;
            let client = Client::new(&hub, token.clone(), Duration::from_secs(15))?;
            let me = client.me().await.context("the hub rejected this token")?;
            let stored = config::store_token(&hub, &token)?;
            let mut cfg = config::Config::load()?;
            cfg.hub = Some(hub.clone());
            if project.is_some() {
                cfg.default_project = project;
            }
            cfg.save()?;
            let place = match &stored {
                config::Stored::Keychain => "the OS keychain".to_string(),
                config::Stored::File(p) => format!("{} (no OS keychain here)", p.display()),
            };
            println!(
                "Logged in to {hub} as @{} (org {}). Token stored in {place}.",
                me.member.handle, me.org
            );
            let projects: Vec<&str> = me.projects.iter().map(|p| p.slug.as_str()).collect();
            println!(
                "Projects: {}",
                if projects.is_empty() {
                    "none yet".into()
                } else {
                    projects.join(", ")
                }
            );
        }
        Cmd::Logout => {
            let cfg = config::Config::load()?;
            let hub = cfg.hub.context("not logged in")?;
            config::delete_token(&hub)?;
            println!("Token for {hub} removed.");
        }
        Cmd::Whoami => {
            let (client, target) = client_for(&cwd, Duration::from_secs(15))?;
            let me = client.me().await?;
            println!(
                "@{} ({}) in org {} on {}",
                me.member.handle,
                me.member.kind.as_str(),
                me.org,
                target.hub
            );
            if let Some(o) = me.member.owner {
                println!("owner: @{o}");
            }
            for p in &me.projects {
                println!("- {} as {}", p.slug, p.role);
            }
            match target.project {
                Some(p) => println!("this directory -> {p}"),
                None => println!(
                    "this directory -> no .davra.toml (the only project is used, if there is one)"
                ),
            }
        }
        Cmd::Mcp => mcp::serve(cwd).await?,
        Cmd::Brief { hook, project } => {
            if hook {
                // A hook must never break or slow down the session, and it only
                // speaks inside repositories that opted in with a .davra.toml.
                let cwd = hook_cwd();
                if project.is_none() && project_file::find(&cwd).is_none() {
                    return Ok(());
                }
                match brief(cwd, project, Duration::from_secs(4)).await {
                    Ok(t) => print!("{t}"),
                    Err(e) => eprintln!("davra: brief unavailable: {e:#}"),
                }
            } else {
                print!("{}", brief(cwd, project, Duration::from_secs(15)).await?);
            }
        }
        Cmd::Inbox {
            hook,
            stop,
            project,
        } => {
            if hook {
                let (cwd, input) = hook_input();
                if project.is_none() && project_file::find(&cwd).is_none() {
                    return Ok(());
                }
                // A live channel already delivered everything; and a Stop hook
                // that fired once in a row lets the agent rest (no endless turns).
                if stop && (mcp::channel_mode() || input["stop_hook_active"] == true) {
                    return Ok(());
                }
                match inbox(cwd, project, true, Duration::from_secs(3)).await {
                    Ok(t) if stop && !t.is_empty() => println!(
                        "{}",
                        serde_json::json!({ "decision": "block", "reason": t })
                    ),
                    Ok(t) => print!("{t}"),
                    Err(e) => eprintln!("davra: inbox unavailable: {e:#}"),
                }
            } else {
                println!(
                    "{}",
                    inbox(cwd, project, false, Duration::from_secs(15)).await?
                );
            }
        }
        Cmd::Post {
            text,
            reply_to,
            project,
        } => {
            let (client, target) = client_for(&cwd, Duration::from_secs(15))?;
            let p = client
                .resolve_project(project.as_deref().or(target.project.as_deref()))
                .await?;
            let r = client
                .post_message(
                    &p,
                    &PostRequest {
                        text,
                        to: vec![],
                        topic: None,
                        reply_to,
                    },
                )
                .await?;
            println!(
                "posted #{}{}",
                r.id,
                if r.relayed { " (shown in chat)" } else { "" }
            );
            if r.redacted {
                eprintln!("note: something that looked like a secret was removed");
            }
        }
        Cmd::Search { query, k, project } => {
            let (client, target) = client_for(&cwd, Duration::from_secs(15))?;
            let p = client
                .resolve_project(project.as_deref().or(target.project.as_deref()))
                .await?;
            print!(
                "{}",
                format::search_text(&client.search(&p, &query, k).await?)
            );
        }
        Cmd::Answer { id, answer } => {
            let (client, _) = client_for(&cwd, Duration::from_secs(15))?;
            client.answer_question(id, &answer).await?;
            println!("answered question #{id}");
        }
        Cmd::Handoffs(HandoffsCmd::Sync { announce, quiet }) => {
            let (client, target) = client_for(&cwd, Duration::from_secs(60))?;
            let repo = handoffs::repo(&cwd).context(
                "no handoffs folder: add handoffs = \"docs/memory/handoffs\" to .davra.toml",
            )?;
            let mode = if announce {
                Some(true)
            } else if quiet {
                Some(false)
            } else {
                None
            };
            let r = handoffs::sync(&client, &target.hub, &repo, mode).await?;
            println!(
                "{}: {} new, {} updated, {} closed, {} reopened, {} unchanged",
                repo.project, r.created, r.updated, r.closed, r.reopened, r.unchanged
            );
        }
        Cmd::Handoffs(HandoffsCmd::List {
            status,
            all,
            project,
        }) => {
            let (client, target) = client_for(&cwd, Duration::from_secs(15))?;
            let p = client
                .resolve_project(project.as_deref().or(target.project.as_deref()))
                .await?;
            let l = client
                .handoffs(&p, &status, if all { "all" } else { "me" })
                .await?;
            print!("{}", format::handoffs_text(&l));
        }
        Cmd::Claude { args } => {
            // Channels are a research preview: a server outside Anthropic's
            // allowlist loads only with this flag, after a one-time prompt.
            let status = claude_cmd()
                .args(["--dangerously-load-development-channels", "server:davra"])
                .args(&args)
                .env(mcp::CHANNEL_ENV, "1")
                .status()
                .context("Claude Code (`claude`) was not found on PATH; install it first")?;
            std::process::exit(status.code().unwrap_or(1));
        }
        Cmd::Setup {
            client,
            dry_run,
            undo,
            no_usage,
        } => {
            if undo {
                setup::undo(&client)?;
            } else {
                setup::run(&client, dry_run, !no_usage)?;
            }
        }
        Cmd::OtelHeaders { hub, project } => {
            let mut h = serde_json::Map::new();
            if let Ok(token) = config::normalize_hub(&hub).and_then(|hub| config::load_token(&hub))
            {
                h.insert("Authorization".into(), format!("Bearer {token}").into());
                h.insert("X-Davra-Project".into(), project.into());
            }
            println!("{}", serde_json::Value::Object(h));
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(e) = rt.block_on(run(cli)) {
        eprintln!("davra: {e:#}");
        std::process::exit(1);
    }
}

/// `claude` in this terminal (on Windows it is a .cmd shim, run via cmd).
fn claude_cmd() -> std::process::Command {
    if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "claude"]);
        c
    } else {
        std::process::Command::new("claude")
    }
}
