//! The commands the Davra Desktop UI calls (see web/src/desktop/bridge.ts).
//! All the work is done by the `davra_cli` library, the same code behind
//! `davra setup`; this file only shapes it for the UI. Errors reach the UI as
//! plain messages.

use std::path::{Path, PathBuf};
use std::time::Duration;

use davra_cli::client::Client;
use davra_cli::config::{self, Config};
use davra_cli::connect::{self, ClientId, Plan, Step};
use davra_cli::install;
use serde::Serialize;

type R<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Runs blocking work (files, `claude mcp get`, PowerShell) off the UI thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> R<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[derive(Serialize)]
pub struct ProjectItem {
    slug: String,
    role: String,
}

#[derive(Serialize)]
pub struct Me {
    handle: String,
    kind: String,
    org: String,
    owner: Option<String>,
    projects: Vec<ProjectItem>,
}

#[derive(Serialize)]
pub struct Cli {
    path: Option<String>,
    version: Option<String>,
    bundled_version: String,
    on_path: bool,
}

#[derive(Serialize)]
pub struct ClientItem {
    id: &'static str,
    name: &'static str,
    detected: bool,
    connected: bool,
    partial: bool,
    detail: String,
}

#[derive(Serialize)]
pub struct RepoItem {
    path: String,
    project: String,
    hub: Option<String>,
    usage: bool,
    has_file: bool,
}

#[derive(Serialize)]
pub struct Status {
    version: String,
    hub: Option<String>,
    me: Option<Me>,
    me_error: Option<String>,
    cli: Cli,
    clients: Vec<ClientItem>,
    repos: Vec<RepoItem>,
}

#[derive(Serialize)]
pub struct Change {
    kind: &'static str,
    title: String,
    path: Option<String>,
    diff: String,
    new_file: bool,
}

#[derive(Serialize)]
pub struct PlanView {
    changes: Vec<Change>,
    notes: Vec<String>,
}

#[derive(Serialize)]
pub struct Applied {
    backups: Vec<String>,
    status: Status,
}

/// The CLI shipped inside the app (Tauri's externalBin sidecar sits next to
/// the app's own executable, without the target triple).
fn bundled_cli() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no app folder"))?;
    let p = dir.join(if cfg!(windows) { "davra.exe" } else { "davra" });
    anyhow::ensure!(
        p.exists(),
        "the bundled davra CLI is missing at {}",
        p.display()
    );
    Ok(p)
}

/// The CLI the clients' configs point to: the installed copy, so it keeps
/// working when the app moves or updates.
fn cli_exe() -> anyhow::Result<String> {
    Ok(connect::exe_string(&install::cli_path()?))
}

fn cli_installed() -> bool {
    install::cli_path().is_ok_and(|p| p.exists())
}

fn install_cli_now() -> anyhow::Result<()> {
    install::install_from(&bundled_cli()?)?;
    install::add_to_path(&install::bin_dir()?)?;
    Ok(())
}

async fn fetch_me(hub: &str) -> anyhow::Result<Option<Me>> {
    let token = match config::load_token(hub) {
        Ok(t) => t,
        // Not logged in (yet, or after logout): the connect screen, not an error.
        Err(e) if format!("{e:#}").contains("not logged in") => return Ok(None),
        Err(e) => return Err(e),
    };
    let me = Client::new(hub, token, Duration::from_secs(8))?
        .me()
        .await?;
    Ok(Some(Me {
        handle: me.member.handle,
        kind: me.member.kind.as_str().to_string(),
        org: me.org,
        owner: me.member.owner,
        projects: me
            .projects
            .into_iter()
            .map(|p| ProjectItem {
                slug: p.slug,
                role: p.role,
            })
            .collect(),
    }))
}

async fn build_status() -> R<Status> {
    let cfg = Config::load().map_err(err)?;
    let hub = cfg.hub.clone();
    let (me, me_error) = match &hub {
        None => (None, None),
        Some(h) => match fetch_me(h).await {
            Ok(m) => (m, None),
            Err(e) => (None, Some(err(e))),
        },
    };
    let local = {
        let hub = hub.clone();
        blocking(move || {
            let path = install::cli_path()?;
            let installed = path.exists();
            let cli = Cli {
                version: installed.then(|| install::version_of(&path)).flatten(),
                path: installed.then(|| path.display().to_string()),
                bundled_version: install::VERSION.to_string(),
                on_path: install::on_path(&install::bin_dir()?),
            };
            let exe = cli_exe()?;
            let clients = ClientId::ALL
                .iter()
                .map(|&id| {
                    let s = connect::state(id, &exe);
                    ClientItem {
                        id: id.as_str(),
                        name: id.name(),
                        detected: s.detected,
                        connected: s.connected,
                        partial: s.partial,
                        detail: s.detail,
                    }
                })
                .collect();
            let repos = cfg
                .repos
                .iter()
                .map(|p| {
                    let s = connect::repo_state(p, hub.as_deref());
                    RepoItem {
                        path: p.display().to_string(),
                        project: s.project.unwrap_or_default(),
                        hub: s.hub,
                        usage: s.usage,
                        has_file: s.has_file,
                    }
                })
                .collect();
            Ok((cli, clients, repos))
        })
        .await?
    };
    let (cli, clients, repos) = local;
    Ok(Status {
        version: env!("CARGO_PKG_VERSION").to_string(),
        hub,
        me,
        me_error,
        cli,
        clients,
        repos,
    })
}

fn view(plan: &Plan, install_first: bool) -> PlanView {
    let mut changes = Vec::new();
    if install_first {
        let to = install::cli_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        changes.push(Change {
            kind: "command",
            title: "Install the davra CLI and add it to PATH".into(),
            path: None,
            diff: format!("copy davra → {to}"),
            new_file: true,
        });
    }
    for s in &plan.steps {
        changes.push(Change {
            kind: if matches!(s, Step::Write { .. }) {
                "file"
            } else {
                "command"
            },
            title: s.title().to_string(),
            path: s.path().map(|p| p.display().to_string()),
            diff: s.diff(),
            new_file: s.is_new_file(),
        });
    }
    PlanView {
        changes,
        notes: plan.notes.clone(),
    }
}

fn backups(list: Vec<PathBuf>) -> Vec<String> {
    list.into_iter().map(|p| p.display().to_string()).collect()
}

#[tauri::command]
pub async fn status() -> R<Status> {
    build_status().await
}

#[tauri::command]
pub async fn login(hub: String, token: String) -> R<Status> {
    let hub = config::normalize_hub(&hub).map_err(err)?;
    let token = token.trim().to_string();
    if !token.starts_with("dvr_") {
        return Err("a Davra token starts with dvr_".into());
    }
    // Check it before storing anything.
    Client::new(&hub, token.clone(), Duration::from_secs(10))
        .map_err(err)?
        .me()
        .await
        .map_err(err)?;
    let h = hub.clone();
    blocking(move || {
        config::store_token(&h, &token)?;
        let mut cfg = Config::load()?;
        cfg.hub = Some(h);
        cfg.save()
    })
    .await?;
    build_status().await
}

#[tauri::command]
pub async fn logout() -> R<Status> {
    blocking(|| {
        if let Some(h) = Config::load()?.hub {
            config::delete_token(&h)?;
        }
        Ok(())
    })
    .await?;
    build_status().await
}

#[tauri::command]
pub async fn install_cli() -> R<Status> {
    blocking(install_cli_now).await?;
    build_status().await
}

#[tauri::command]
pub async fn plan_connect(client: String) -> R<PlanView> {
    blocking(move || {
        let id = ClientId::parse(&client)?;
        let plan = connect::connect(id, &cli_exe()?)?;
        Ok(view(&plan, !cli_installed()))
    })
    .await
}

#[tauri::command]
pub async fn apply_connect(client: String) -> R<Applied> {
    let b = blocking(move || {
        let id = ClientId::parse(&client)?;
        if !cli_installed() {
            install_cli_now()?;
        }
        connect::apply(&connect::connect(id, &cli_exe()?)?)
    })
    .await?;
    Ok(Applied {
        backups: backups(b),
        status: build_status().await?,
    })
}

#[tauri::command]
pub async fn plan_disconnect(client: String) -> R<PlanView> {
    blocking(move || {
        Ok(view(
            &connect::disconnect(ClientId::parse(&client)?)?,
            false,
        ))
    })
    .await
}

#[tauri::command]
pub async fn apply_disconnect(client: String) -> R<Applied> {
    let b =
        blocking(move || connect::apply(&connect::disconnect(ClientId::parse(&client)?)?)).await?;
    Ok(Applied {
        backups: backups(b),
        status: build_status().await?,
    })
}

fn repo_plan(path: &Path, project: &str, usage: bool) -> anyhow::Result<Plan> {
    let hub = Config::load()?
        .hub
        .ok_or_else(|| anyhow::anyhow!("connect to a hub first"))?;
    connect::repo(path, &hub, None, project, usage, &cli_exe()?)
}

#[tauri::command]
pub async fn plan_repo(path: String, project: String, usage: bool) -> R<PlanView> {
    blocking(move || {
        Ok(view(
            &repo_plan(Path::new(&path), &project, usage)?,
            usage && !cli_installed(),
        ))
    })
    .await
}

#[tauri::command]
pub async fn apply_repo(path: String, project: String, usage: bool) -> R<Applied> {
    let b = blocking(move || {
        let p = PathBuf::from(&path);
        if usage && !cli_installed() {
            install_cli_now()?;
        }
        let b = connect::apply(&repo_plan(&p, &project, usage)?)?;
        let mut cfg = Config::load()?;
        if !cfg.repos.contains(&p) {
            cfg.repos.push(p);
            cfg.save()?;
        }
        Ok(b)
    })
    .await?;
    Ok(Applied {
        backups: backups(b),
        status: build_status().await?,
    })
}

#[tauri::command]
pub async fn forget_repo(path: String) -> R<Status> {
    blocking(move || {
        let mut cfg = Config::load()?;
        cfg.repos.retain(|p| p != Path::new(&path));
        cfg.save()
    })
    .await?;
    build_status().await
}

// ---- Updates -----------------------------------------------------------------

/// A newer Davra Desktop the hub offers.
#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    current: String,
    notes: Option<String>,
}

/// The connected hub decides which version this machine gets: every hub
/// serves its own builds, so there is no update server baked into the app.
/// Updates are verified against the public key in tauri.conf.json.
fn updater(app: &tauri::AppHandle) -> anyhow::Result<Option<tauri_plugin_updater::Updater>> {
    use tauri_plugin_updater::UpdaterExt;
    let Some(hub) = Config::load()?.hub else {
        return Ok(None);
    };
    let url = format!(
        "{}/api/v1/desktop/update?target={{{{target}}}}&arch={{{{arch}}}}&current={{{{current_version}}}}",
        hub.trim_end_matches('/')
    );
    let u = app
        .updater_builder()
        .endpoints(vec![tauri::Url::parse(&url)?])?
        .timeout(Duration::from_secs(30))
        .build()?;
    Ok(Some(u))
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> R<Option<UpdateInfo>> {
    let Some(u) = updater(&app).map_err(err)? else {
        return Ok(None);
    };
    let found = u.check().await.map_err(|e| e.to_string())?;
    Ok(found.map(|f| UpdateInfo {
        version: f.version.clone(),
        current: f.current_version.clone(),
        notes: f.body.clone(),
    }))
}

/// Downloads, verifies and installs the update, then restarts the app.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> R<()> {
    let u = updater(&app)
        .map_err(err)?
        .ok_or_else(|| "connect to a hub first".to_string())?;
    let found = u
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "already up to date".to_string())?;
    found
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}
