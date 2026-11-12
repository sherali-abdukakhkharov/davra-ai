mod client;
mod config;
mod handoffs;
mod mcp;

use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use davra_core::api::PostRequest;
use davra_core::format;
use davra_core::project_file;

use crate::client::Client;

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
        #[arg(long)]
        project: Option<String>,
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
    Setup {
        #[arg(value_parser = ["claude"])]
        client: String,
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

/// Hooks receive JSON on stdin; we only need `cwd` from it.
fn hook_cwd() -> PathBuf {
    let fallback = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if std::io::stdin().is_terminal() {
        return fallback;
    }
    let mut buf = String::new();
    if std::io::stdin()
        .take(1 << 20)
        .read_to_string(&mut buf)
        .is_err()
    {
        return fallback;
    }
    serde_json::from_str::<serde_json::Value>(&buf)
        .ok()
        .and_then(|v| v.get("cwd").and_then(|c| c.as_str()).map(PathBuf::from))
        .unwrap_or(fallback)
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
            config::store_token(&hub, &token)?;
            let mut cfg = config::Config::load()?;
            cfg.hub = Some(hub.clone());
            if project.is_some() {
                cfg.default_project = project;
            }
            cfg.save()?;
            println!(
                "Logged in to {hub} as @{} (org {}). Token stored in the OS keychain.",
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
            println!("Token for {hub} removed from the OS keychain.");
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
        Cmd::Inbox { hook, project } => {
            if hook {
                let cwd = hook_cwd();
                if project.is_none() && project_file::find(&cwd).is_none() {
                    return Ok(());
                }
                match inbox(cwd, project, true, Duration::from_secs(3)).await {
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
        Cmd::Setup { client: _ } => {
            let exe = std::env::current_exe()?;
            let exe = exe.display();
            println!("# 1. Register the MCP server for all your projects:");
            println!("claude mcp add --scope user davra -- \"{exe}\" mcp\n");
            println!(
                "# 2. Add these hooks to ~/.claude/settings.json (merge with existing \"hooks\"):"
            );
            let hooks = serde_json::json!({
                "hooks": {
                    "SessionStart": [{ "hooks": [{ "type": "command", "command": format!("\"{exe}\" brief --hook") }] }],
                    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": format!("\"{exe}\" inbox --hook") }] }]
                }
            });
            println!("{}", serde_json::to_string_pretty(&hooks)?);
            println!(
                "\n# The installer (v0.4) will do both steps for you, with a diff and a backup."
            );
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
