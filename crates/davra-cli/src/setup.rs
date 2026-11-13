//! `davra setup <claude|cursor|codex>`: connects a coding client on this
//! machine. The changes come from `connect` as a plan: they are listed (with
//! diffs under --dry-run), every changed file is backed up to
//! `~/.davra/backups/`, and running it again adds nothing twice.

use std::path::Path;

use anyhow::Result;

use crate::connect::{self, ClientId, Plan};

fn print_plan(plan: &Plan, diffs: bool) {
    for step in &plan.steps {
        match step.path() {
            Some(p) => println!("  + {}\n      {}", step.title(), p.display()),
            None => println!("  + {}\n      $ {}", step.title(), step.diff()),
        }
        if diffs && step.path().is_some() {
            for line in step.diff().lines() {
                println!("      {line}");
            }
        }
    }
    for n in &plan.notes {
        println!("  {n}");
    }
}

pub fn run(client: &str, dry_run: bool, usage: bool) -> Result<()> {
    let client = ClientId::parse(client)?;
    let exe = connect::exe_string(&std::env::current_exe()?);
    let cwd = std::env::current_dir()?;
    let plan = connect::setup_plan(client, &exe, &cwd, usage)?;
    println!("Davra setup for {}", client.name());
    println!("  davra:    {exe}");
    if plan.is_empty() {
        println!("  already connected; nothing to change.");
    } else {
        print_plan(&plan, dry_run);
    }
    if client == ClientId::Claude && davra_core::project_file::find(&cwd).is_none() {
        println!(
            "\n  Note: {} has no .davra.toml, so Davra stays silent here.\n  \
             Commit one at the repository root (no secrets in it):\n    \
             hub     = \"https://your-hub\"\n    org     = \"your-org\"\n    project = \"your-project\"",
            cwd.display()
        );
    } else if client == ClientId::Claude && usage {
        println!("  Usage reporting sends counts only (tokens, cost, lines, commits). Skip it with --no-usage.");
    }
    if dry_run {
        println!("\n--dry-run: nothing changed.");
        return Ok(());
    }
    for b in connect::apply(&plan)? {
        println!("  backup:   {}", b.display());
    }
    if client == ClientId::Claude {
        println!("\nDone. Open a new Claude Code session in a repo with a .davra.toml.");
        println!("For live messages (they reach the agent even while it waits), start it with:");
        println!("  davra claude");
    } else {
        println!(
            "\nDone. Restart {} to load the davra MCP server.",
            client.name()
        );
    }
    Ok(())
}

/// Removes what `run` added, including this repository's usage reporting.
pub fn undo(client: &str) -> Result<()> {
    let client = ClientId::parse(client)?;
    let mut plan = connect::disconnect(client)?;
    let cwd = std::env::current_dir()?;
    if client == ClientId::Claude {
        if let Some((file, pf)) = davra_core::project_file::find(&cwd) {
            let root = file.parent().unwrap_or(Path::new("."));
            let hub = crate::config::target(&cwd)?.hub;
            let exe = connect::exe_string(&std::env::current_exe()?);
            let usage_off = connect::repo(root, &hub, pf.org.as_deref(), &pf.project, false, &exe)?;
            plan.steps.extend(usage_off.steps);
        }
    }
    if plan.is_empty() {
        println!("Nothing of Davra's is set up for {}.", client.name());
        return Ok(());
    }
    print_plan(&plan, false);
    for b in connect::apply(&plan)? {
        println!("  backup:   {}", b.display());
    }
    println!("Removed Davra from {}.", client.name());
    Ok(())
}
