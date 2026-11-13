//! Admin commands, run on the hub host: `davra-hub org|project|member|token ...`.
//! The web admin panel replaces most of these in v0.3.

use anyhow::{bail, Context, Result};
use davra_core::mentions::normalize_handle;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{hash_token, new_token};
use crate::db;

const ROLES: &[&str] = &["owner", "maintainer", "member", "viewer", "agent"];

fn slug(raw: &str) -> Result<String> {
    normalize_handle(raw)
        .with_context(|| format!("'{raw}' is not a valid slug (letters, digits, '-', '_', '.')"))
}

/// Resolves `--org`, or the only org when there is exactly one.
pub async fn resolve_org(pool: &PgPool, org: Option<&str>) -> Result<(Uuid, String)> {
    if let Some(s) = org {
        let s = slug(s)?;
        let id: Option<Uuid> = sqlx::query_scalar("select id from orgs where slug = $1")
            .bind(&s)
            .fetch_optional(pool)
            .await?;
        return Ok((id.with_context(|| format!("org '{s}' does not exist"))?, s));
    }
    let all: Vec<(Uuid, String)> = sqlx::query_as("select id, slug from orgs order by slug")
        .fetch_all(pool)
        .await?;
    match all.len() {
        1 => Ok(all.into_iter().next().expect("one org")),
        0 => bail!("no orgs yet: run `davra-hub org create <slug>` first"),
        _ => bail!("several orgs exist; pass --org"),
    }
}

async fn project_id(pool: &PgPool, org_id: Uuid, project: &str) -> Result<Uuid> {
    let s = slug(project)?;
    let id: Option<Uuid> =
        sqlx::query_scalar("select id from projects where org_id = $1 and slug = $2")
            .bind(org_id)
            .bind(&s)
            .fetch_optional(pool)
            .await?;
    id.with_context(|| format!("project '{s}' does not exist"))
}

async fn member_id(pool: &PgPool, org_id: Uuid, handle: &str) -> Result<(Uuid, String)> {
    let h = slug(handle)?;
    let row: Option<(Uuid, String)> =
        sqlx::query_as("select id, kind from members where org_id = $1 and handle = $2")
            .bind(org_id)
            .bind(&h)
            .fetch_optional(pool)
            .await?;
    row.with_context(|| format!("member '{h}' does not exist"))
}

pub async fn org_create(pool: &PgPool, raw: &str, name: Option<&str>) -> Result<()> {
    let s = slug(raw)?;
    let id: Uuid = sqlx::query_scalar("insert into orgs (slug, name) values ($1, $2) returning id")
        .bind(&s)
        .bind(name)
        .fetch_one(pool)
        .await
        .with_context(|| format!("could not create org '{s}' (does it exist already?)"))?;
    db::audit(
        pool,
        Some(id),
        None,
        "org.create",
        Some(&s),
        serde_json::json!({}),
    )
    .await?;
    println!("org '{s}' created");
    Ok(())
}

pub async fn project_create(
    pool: &PgPool,
    org: Option<&str>,
    raw: &str,
    name: Option<&str>,
    tg_chat: Option<i64>,
) -> Result<()> {
    let (org_id, org_slug) = resolve_org(pool, org).await?;
    let s = slug(raw)?;
    sqlx::query("insert into projects (org_id, slug, name, tg_chat_id) values ($1, $2, $3, $4)")
        .bind(org_id)
        .bind(&s)
        .bind(name)
        .bind(tg_chat)
        .execute(pool)
        .await
        .with_context(|| format!("could not create project '{s}' (slug or chat already used?)"))?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "project.create",
        Some(&s),
        serde_json::json!({ "tg_chat_id": tg_chat }),
    )
    .await?;
    println!("project '{org_slug}/{s}' created");
    Ok(())
}

pub async fn project_github(
    pool: &PgPool,
    key: Option<&crate::secrets::MasterKey>,
    org: Option<&str>,
    project: &str,
    repo: &str,
    public_url: &str,
) -> Result<()> {
    let (org_id, org_slug) = resolve_org(pool, org).await?;
    let pid = project_id(pool, org_id, project).await?;
    if repo.split('/').count() != 2 {
        bail!("--repo must look like owner/name");
    }
    let secret = crate::auth::new_token().replacen("dvr_", "ghw_", 1);
    if key.is_none() {
        eprintln!("warning: DAVRA_MASTER_KEY is not set, so this secret is stored unencrypted");
    }
    sqlx::query("update projects set github_repo = $2, github_secret = $3 where id = $1")
        .bind(pid)
        .bind(repo)
        .bind(crate::secrets::protect(key, pid.as_bytes(), &secret))
        .execute(pool)
        .await?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "project.github",
        Some(project),
        serde_json::json!({ "repo": repo }),
    )
    .await?;
    let base = public_url.trim_end_matches('/');
    println!("GitHub → {repo} → Settings → Webhooks → Add webhook:");
    println!("  Payload URL:  {base}/webhooks/github/{org_slug}/{project}");
    println!("  Content type: application/json");
    println!("  Secret:       {secret}");
    println!("  Events:       Pushes, Pull requests, Workflow runs");
    Ok(())
}

pub async fn project_link_telegram(
    pool: &PgPool,
    org: Option<&str>,
    project: &str,
    chat: i64,
) -> Result<()> {
    let (org_id, _) = resolve_org(pool, org).await?;
    let pid = project_id(pool, org_id, project).await?;
    sqlx::query("update projects set tg_chat_id = $2 where id = $1")
        .bind(pid)
        .bind(chat)
        .execute(pool)
        .await
        .context("could not link chat (is it linked to another project?)")?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "project.link_telegram",
        Some(project),
        serde_json::json!({ "tg_chat_id": chat }),
    )
    .await?;
    println!("project '{project}' now listens to Telegram chat {chat}");
    Ok(())
}

pub struct MemberAdd<'a> {
    pub org: Option<&'a str>,
    pub handle: &'a str,
    pub kind: &'a str,
    pub name: Option<&'a str>,
    pub owner: Option<&'a str>,
    pub tg_user: Option<i64>,
    pub projects: &'a [String],
    pub role: Option<&'a str>,
}

pub async fn member_add(pool: &PgPool, a: MemberAdd<'_>) -> Result<()> {
    let (org_id, _) = resolve_org(pool, a.org).await?;
    let h = slug(a.handle)?;
    if a.kind != "human" && a.kind != "agent" {
        bail!("--kind must be 'human' or 'agent'");
    }
    let owner_id = match (a.kind, a.owner) {
        ("agent", Some(o)) => {
            let (id, kind) = member_id(pool, org_id, o).await?;
            if kind != "human" {
                bail!("an agent's owner must be a human");
            }
            Some(id)
        }
        ("agent", None) => bail!("an agent needs --owner <human handle>"),
        (_, Some(_)) => bail!("only agents have an owner"),
        _ => None,
    };
    let id: Uuid = sqlx::query_scalar(
        "insert into members (org_id, kind, handle, display_name, owner_id, tg_user_id) \
         values ($1, $2, $3, $4, $5, $6) returning id",
    )
    .bind(org_id)
    .bind(a.kind)
    .bind(&h)
    .bind(a.name)
    .bind(owner_id)
    .bind(a.tg_user)
    .fetch_one(pool)
    .await
    .with_context(|| format!("could not add '{h}' (handle or Telegram user already used?)"))?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "member.add",
        Some(&h),
        serde_json::json!({ "kind": a.kind }),
    )
    .await?;
    println!("{} '{h}' added", a.kind);
    let default_role = if a.kind == "agent" { "agent" } else { "member" };
    for p in a.projects {
        grant_inner(pool, org_id, id, &h, p, a.role.unwrap_or(default_role)).await?;
    }
    Ok(())
}

async fn grant_inner(
    pool: &PgPool,
    org_id: Uuid,
    member: Uuid,
    handle: &str,
    project: &str,
    role: &str,
) -> Result<()> {
    if !ROLES.contains(&role) {
        bail!("role must be one of {}", ROLES.join(", "));
    }
    let pid = project_id(pool, org_id, project).await?;
    sqlx::query(
        "insert into grants (member_id, project_id, role) values ($1, $2, $3) \
         on conflict (member_id, project_id) do update set role = excluded.role",
    )
    .bind(member)
    .bind(pid)
    .bind(role)
    .execute(pool)
    .await?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "grant.set",
        Some(handle),
        serde_json::json!({ "project": project, "role": role }),
    )
    .await?;
    println!("'{handle}' is {role} in '{project}'");
    Ok(())
}

pub async fn member_roles(pool: &PgPool, org: Option<&str>, handle: &str, set: &str) -> Result<()> {
    let (org_id, _) = resolve_org(pool, org).await?;
    let (mid, _) = member_id(pool, org_id, handle).await?;
    let roles = davra_core::handoff_file::split_labels(set);
    sqlx::query("update members set roles = $2 where id = $1")
        .bind(mid)
        .bind(&roles)
        .execute(pool)
        .await?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "member.roles",
        Some(handle),
        serde_json::json!({ "roles": roles }),
    )
    .await?;
    if roles.is_empty() {
        println!("'{handle}' has no roles");
    } else {
        println!("'{handle}' covers: {}", roles.join(", "));
    }
    Ok(())
}

pub async fn member_rename(
    pool: &PgPool,
    org: Option<&str>,
    handle: &str,
    new_handle: &str,
) -> Result<()> {
    let (org_id, _) = resolve_org(pool, org).await?;
    let (mid, _) = member_id(pool, org_id, handle).await?;
    let new = slug(new_handle)?;
    match db::rename_member(pool, mid, &new).await? {
        db::Rename::Done { old } => println!("'{old}' is now '{new}'"),
        db::Rename::Same => println!("'{new}' already has that handle"),
        db::Rename::Taken => bail!("'{new}' is already used in this org"),
    }
    Ok(())
}

pub async fn member_link_telegram(
    pool: &PgPool,
    org: Option<&str>,
    handle: &str,
    tg_user: i64,
) -> Result<()> {
    let (org_id, _) = resolve_org(pool, org).await?;
    let (mid, _) = member_id(pool, org_id, handle).await?;
    sqlx::query("update members set tg_user_id = $2 where id = $1")
        .bind(mid)
        .bind(tg_user)
        .execute(pool)
        .await
        .context("could not link (is this Telegram user linked to someone else?)")?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "member.link_telegram",
        Some(handle),
        serde_json::json!({}),
    )
    .await?;
    println!("'{handle}' is Telegram user {tg_user}");
    Ok(())
}

pub async fn grant(
    pool: &PgPool,
    org: Option<&str>,
    handle: &str,
    project: &str,
    role: Option<&str>,
) -> Result<()> {
    let (org_id, _) = resolve_org(pool, org).await?;
    let (mid, kind) = member_id(pool, org_id, handle).await?;
    let default_role = if kind == "agent" { "agent" } else { "member" };
    grant_inner(
        pool,
        org_id,
        mid,
        handle,
        project,
        role.unwrap_or(default_role),
    )
    .await
}

pub async fn token_create(
    pool: &PgPool,
    org: Option<&str>,
    handle: &str,
    name: Option<&str>,
) -> Result<()> {
    let (org_id, _) = resolve_org(pool, org).await?;
    let (mid, _) = member_id(pool, org_id, handle).await?;
    let token = new_token();
    let id: Uuid = sqlx::query_scalar(
        "insert into tokens (member_id, token_hash, name) values ($1, $2, $3) returning id",
    )
    .bind(mid)
    .bind(hash_token(&token))
    .bind(name)
    .fetch_one(pool)
    .await?;
    db::audit(
        pool,
        Some(org_id),
        None,
        "token.create",
        Some(handle),
        serde_json::json!({ "token_id": id, "name": name }),
    )
    .await?;
    eprintln!("token {id} created for '{handle}'. It is shown once; store it with `davra login`.");
    println!("{token}");
    Ok(())
}

pub async fn token_revoke(pool: &PgPool, id: Uuid) -> Result<()> {
    let org: Option<Uuid> = sqlx::query_scalar(
        "update tokens t set revoked_at = now() from members m \
         where t.id = $1 and m.id = t.member_id and t.revoked_at is null returning m.org_id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let org = org.with_context(|| format!("no active token {id}"))?;
    db::audit(
        pool,
        Some(org),
        None,
        "token.revoke",
        None,
        serde_json::json!({ "token_id": id }),
    )
    .await?;
    println!("token {id} revoked");
    Ok(())
}

pub async fn status(pool: &PgPool) -> Result<()> {
    let orgs: Vec<(Uuid, String)> = sqlx::query_as("select id, slug from orgs order by slug")
        .fetch_all(pool)
        .await?;
    if orgs.is_empty() {
        println!("no orgs yet");
    }
    for (oid, oslug) in orgs {
        println!("org {oslug}");
        let projects: Vec<(String, Option<i64>, i64)> = sqlx::query_as(
            "select p.slug, p.tg_chat_id, (select count(*) from messages m where m.project_id = p.id) \
             from projects p where p.org_id = $1 order by p.slug",
        )
        .bind(oid)
        .fetch_all(pool)
        .await?;
        for (s, chat, n) in projects {
            let chat = chat
                .map(|c| format!("telegram {c}"))
                .unwrap_or_else(|| "no chat".into());
            println!("  project {s} · {chat} · {n} messages");
        }
        type Row = (String, String, Option<String>, Option<i64>, i64);
        let members: Vec<Row> = sqlx::query_as(
            "select m.handle, m.kind, ow.handle, m.tg_user_id, \
             (select count(*) from tokens t where t.member_id = m.id and t.revoked_at is null) \
             from members m left join members ow on ow.id = m.owner_id where m.org_id = $1 order by m.kind desc, m.handle",
        )
        .bind(oid)
        .fetch_all(pool)
        .await?;
        for (h, kind, owner, tg, tokens) in members {
            let owner = owner.map(|o| format!(" of {o}")).unwrap_or_default();
            let tg = tg.map(|t| format!(" · tg {t}")).unwrap_or_default();
            println!("  {kind} {h}{owner}{tg} · {tokens} active token(s)");
        }
    }
    Ok(())
}
