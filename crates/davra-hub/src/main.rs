mod accounts;
mod admin;
mod api;
mod auth;
mod db;
mod downloads;
mod embed;
mod error;
mod github;
mod handoffs;
mod questions;
mod team;
mod telegram;
mod tgexport;
mod trust;
mod webapp;

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub telegram: Option<Arc<telegram::Telegram>>,
    pub limiter: Arc<accounts::RateLimiter>,
    pub cookie_secure: bool,
    pub registration_open: bool,
    /// Built web app (index.html + assets). None: API only.
    pub web_dir: Option<std::path::PathBuf>,
    pub embedder: Option<Arc<embed::Embedder>>,
    /// Public base URL, e.g. https://davra.qitmir.uz (shown in connect commands).
    pub public_url: Option<String>,
    /// Prebuilt davra binaries + SHA256SUMS, served under /downloads.
    pub downloads_dir: Option<std::path::PathBuf>,
}

#[derive(Parser)]
#[command(
    name = "davra-hub",
    version,
    about = "Davra hub: a shared room for human + AI teams"
)]
struct Cli {
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the HTTP API and the Telegram bridge.
    Serve {
        #[arg(long, env = "DAVRA_BIND", default_value = "0.0.0.0:8080")]
        bind: String,
        /// Bot token from @BotFather. Without it the hub runs API-only.
        #[arg(long, env = "DAVRA_TELEGRAM_TOKEN", hide_env_values = true)]
        telegram_token: Option<String>,
        /// Skip applying database migrations at startup.
        #[arg(long)]
        no_migrate: bool,
        /// Directory with the built web app (web/dist).
        #[arg(long, env = "DAVRA_WEB_DIR")]
        web_dir: Option<std::path::PathBuf>,
        /// Mark the session cookie Secure (turn off only for plain-http local testing).
        #[arg(long, env = "DAVRA_COOKIE_SECURE", default_value_t = true, action = clap::ArgAction::Set)]
        cookie_secure: bool,
        /// Allow anyone to create a web account.
        #[arg(long, env = "DAVRA_REGISTRATION", default_value = "open", value_parser = ["open", "closed"])]
        registration: String,
        /// Ollama-compatible embedding server, e.g. http://embed:11434. Unset: lexical search only.
        #[arg(long, env = "DAVRA_EMBED_URL")]
        embed_url: Option<String>,
        #[arg(long, env = "DAVRA_EMBED_MODEL", default_value = "bge-m3")]
        embed_model: String,
        /// Public base URL of this hub, e.g. https://davra.example.uz
        #[arg(long, env = "DAVRA_PUBLIC_URL")]
        public_url: Option<String>,
        /// Directory with prebuilt davra binaries and SHA256SUMS.
        #[arg(long, env = "DAVRA_DOWNLOADS_DIR")]
        downloads_dir: Option<std::path::PathBuf>,
    },
    /// Apply database migrations and exit.
    Migrate,
    /// Show orgs, projects and members.
    Status,
    #[command(subcommand)]
    Org(OrgCmd),
    #[command(subcommand)]
    Project(ProjectCmd),
    #[command(subcommand)]
    Member(MemberCmd),
    /// Give a member a role in a project.
    Grant {
        handle: String,
        project: String,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        org: Option<String>,
    },
    #[command(subcommand)]
    Token(TokenCmd),
    /// Import a Telegram Desktop export (result.json) into a project.
    ImportTelegram {
        project: String,
        file: std::path::PathBuf,
        #[arg(long)]
        org: Option<String>,
    },
}

#[derive(Subcommand)]
enum OrgCmd {
    Create {
        slug: String,
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    Create {
        slug: String,
        #[arg(long)]
        org: Option<String>,
        #[arg(long)]
        name: Option<String>,
        /// Telegram group id (send /id in the group to see it).
        #[arg(long, allow_hyphen_values = true)]
        tg_chat: Option<i64>,
    },
    /// Connect a GitHub repository: prints the webhook URL and a new secret.
    Github {
        slug: String,
        /// owner/name
        #[arg(long)]
        repo: String,
        /// Public base URL of this hub, e.g. https://davra.qitmir.uz
        #[arg(long, env = "DAVRA_PUBLIC_URL")]
        public_url: String,
        #[arg(long)]
        org: Option<String>,
    },
    LinkTelegram {
        slug: String,
        #[arg(long, allow_hyphen_values = true)]
        chat: i64,
        #[arg(long)]
        org: Option<String>,
    },
}

#[derive(Subcommand)]
enum MemberCmd {
    Add {
        handle: String,
        #[arg(long, value_parser = ["human", "agent"])]
        kind: String,
        #[arg(long)]
        org: Option<String>,
        #[arg(long)]
        name: Option<String>,
        /// Owner handle (agents only).
        #[arg(long)]
        owner: Option<String>,
        /// Telegram user id (send /id to the bot to see it).
        #[arg(long)]
        tg_user: Option<i64>,
        /// Projects to grant access to (repeatable).
        #[arg(long = "project")]
        projects: Vec<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// Set the roles a member covers (handoffs to a role reach them), e.g. core,auth,db.
    Roles {
        handle: String,
        /// Comma-separated; empty string clears.
        #[arg(long, allow_hyphen_values = true)]
        set: String,
        #[arg(long)]
        org: Option<String>,
    },
    /// Rename a member (mentions in history and handoff labels follow).
    Rename {
        handle: String,
        new_handle: String,
        #[arg(long)]
        org: Option<String>,
    },
    /// Link a Telegram user id to a member.
    LinkTelegram {
        handle: String,
        #[arg(long)]
        tg_user: i64,
        #[arg(long)]
        org: Option<String>,
    },
}

#[derive(Subcommand)]
enum TokenCmd {
    /// Create a token for a member. Printed once on stdout.
    Create {
        handle: String,
        /// Device name, e.g. "lionali-macbook".
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        org: Option<String>,
    },
    Revoke {
        id: Uuid,
    },
}

async fn connect(url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await
        .context("could not connect to Postgres (check DATABASE_URL)")
}

async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("migration failed")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "davra_hub=info,tower_http=warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let pool = connect(&cli.database_url).await?;

    match cli.cmd {
        Cmd::Serve {
            bind,
            telegram_token,
            no_migrate,
            web_dir,
            cookie_secure,
            registration,
            embed_url,
            embed_model,
            public_url,
            downloads_dir,
        } => {
            if !no_migrate {
                migrate(&pool).await?;
            }
            let telegram = match telegram_token.filter(|t| !t.trim().is_empty()) {
                Some(t) => {
                    let tg = Arc::new(telegram::Telegram::connect(t.trim()).await?);
                    tokio::spawn(tg.clone().run_poller(pool.clone()));
                    Some(tg)
                }
                None => {
                    tracing::info!("DAVRA_TELEGRAM_TOKEN not set: running without Telegram");
                    None
                }
            };
            let web_dir = match web_dir {
                Some(d) if d.join("index.html").is_file() => Some(d),
                Some(d) => {
                    tracing::warn!(dir = %d.display(), "DAVRA_WEB_DIR has no index.html: serving API only");
                    None
                }
                None => None,
            };
            let embedder = match embed_url.filter(|u| !u.trim().is_empty()) {
                Some(u) => {
                    let e = Arc::new(embed::Embedder::new(u.trim(), &embed_model)?);
                    tokio::spawn(e.clone().run_worker(pool.clone()));
                    Some(e)
                }
                None => None,
            };
            let app = api::router(AppState {
                pool,
                telegram,
                limiter: Arc::new(accounts::RateLimiter::default()),
                cookie_secure,
                registration_open: registration == "open",
                web_dir,
                embedder,
                public_url: public_url.map(|u| u.trim_end_matches('/').to_string()),
                downloads_dir: downloads_dir.filter(|d| d.join("SHA256SUMS").is_file()),
            });
            let listener = tokio::net::TcpListener::bind(&bind)
                .await
                .with_context(|| format!("could not bind {bind}"))?;
            tracing::info!(%bind, "davra-hub listening");
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        }
        Cmd::Migrate => {
            migrate(&pool).await?;
            println!("migrations applied");
        }
        Cmd::Status => admin::status(&pool).await?,
        Cmd::Org(OrgCmd::Create { slug, name }) => {
            admin::org_create(&pool, &slug, name.as_deref()).await?
        }
        Cmd::Project(ProjectCmd::Create {
            slug,
            org,
            name,
            tg_chat,
        }) => admin::project_create(&pool, org.as_deref(), &slug, name.as_deref(), tg_chat).await?,
        Cmd::Project(ProjectCmd::Github {
            slug,
            repo,
            public_url,
            org,
        }) => admin::project_github(&pool, org.as_deref(), &slug, &repo, &public_url).await?,
        Cmd::Project(ProjectCmd::LinkTelegram { slug, chat, org }) => {
            admin::project_link_telegram(&pool, org.as_deref(), &slug, chat).await?
        }
        Cmd::Member(MemberCmd::Add {
            handle,
            kind,
            org,
            name,
            owner,
            tg_user,
            projects,
            role,
        }) => {
            admin::member_add(
                &pool,
                admin::MemberAdd {
                    org: org.as_deref(),
                    handle: &handle,
                    kind: &kind,
                    name: name.as_deref(),
                    owner: owner.as_deref(),
                    tg_user,
                    projects: &projects,
                    role: role.as_deref(),
                },
            )
            .await?
        }
        Cmd::Member(MemberCmd::Roles { handle, set, org }) => {
            admin::member_roles(&pool, org.as_deref(), &handle, &set).await?
        }
        Cmd::Member(MemberCmd::Rename {
            handle,
            new_handle,
            org,
        }) => admin::member_rename(&pool, org.as_deref(), &handle, &new_handle).await?,
        Cmd::Member(MemberCmd::LinkTelegram {
            handle,
            tg_user,
            org,
        }) => admin::member_link_telegram(&pool, org.as_deref(), &handle, tg_user).await?,
        Cmd::Grant {
            handle,
            project,
            role,
            org,
        } => admin::grant(&pool, org.as_deref(), &handle, &project, role.as_deref()).await?,
        Cmd::Token(TokenCmd::Create { handle, name, org }) => {
            admin::token_create(&pool, org.as_deref(), &handle, name.as_deref()).await?
        }
        Cmd::Token(TokenCmd::Revoke { id }) => admin::token_revoke(&pool, id).await?,
        Cmd::ImportTelegram { project, file, org } => {
            let (org_id, _) = admin::resolve_org(&pool, org.as_deref()).await?;
            let slug =
                davra_core::mentions::normalize_handle(&project).context("invalid project slug")?;
            let (pid,): (Uuid,) =
                sqlx::query_as("select id from projects where org_id = $1 and slug = $2")
                    .bind(org_id)
                    .bind(&slug)
                    .fetch_optional(&pool)
                    .await?
                    .with_context(|| format!("project '{slug}' does not exist"))?;
            let chat = db::ChatProject {
                id: pid,
                org_id,
                slug: slug.clone(),
            };
            let stats = tgexport::import(&pool, &chat, &file).await?;
            db::audit(
                &pool,
                Some(org_id),
                None,
                "import.telegram",
                Some(&slug),
                serde_json::json!({ "inserted": stats.inserted }),
            )
            .await?;
            println!(
                "{}: {} messages read, {} imported, {} skipped",
                slug, stats.seen, stats.inserted, stats.skipped
            );
        }
    }
    Ok(())
}
