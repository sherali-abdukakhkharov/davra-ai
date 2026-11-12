//! The signed-in web app's API (`/api/v1/web/...`), authenticated by the
//! session cookie. A web user acts through the human member linked to their
//! account in each org, so every permission check is the same one agents and
//! the CLI go through.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{DateTime, Utc};
use davra_core::api::{
    HandoffView, MemberKind, MessageView, PostRequest, PostResponse, SearchHit, SearchResults,
};
use davra_core::mentions::normalize_handle;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::accounts::{require_csrf, SessionUser};
use crate::db::{self, Caller, Project};
use crate::error::{AppError, AppResult};
use crate::AppState;

const LINK_TTL_SECS: i64 = 600;

/// The user's human member in `org`, as a Caller.
async fn caller_in(pool: &PgPool, user_id: Uuid, org: &str) -> AppResult<Option<Caller>> {
    type Row = (Uuid, Uuid, String, String, Option<String>, Vec<String>);
    let row: Option<Row> = sqlx::query_as(
        "select m.id, m.org_id, o.slug, m.handle, m.display_name, m.roles from members m \
         join orgs o on o.id = m.org_id where m.user_id = $1 and o.slug = $2 and m.kind = 'human'",
    )
    .bind(user_id)
    .bind(org)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(member_id, org_id, org_slug, handle, display_name, roles)| Caller {
            member_id,
            org_id,
            org_slug,
            kind: MemberKind::Human,
            handle,
            display_name,
            owner_id: None,
            owner_handle: None,
            roles,
        },
    ))
}

async fn open_project(
    state: &AppState,
    user: Uuid,
    org: &str,
    slug: &str,
) -> AppResult<(Caller, Project)> {
    let not_found = || AppError::NotFound(format!("project {org}/{slug} not found"));
    let caller = caller_in(&state.pool, user, org)
        .await?
        .ok_or_else(not_found)?;
    let p = db::project_for(&state.pool, &caller, slug)
        .await?
        .ok_or_else(not_found)?;
    Ok((caller, p))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ProjectItem {
    pub org: String,
    pub slug: String,
    pub name: Option<String>,
    pub role: String,
    pub handle: String,
    pub members: i64,
    pub agents: i64,
    pub messages: i64,
    pub open_handoffs: i64,
    pub last_activity: Option<DateTime<Utc>>,
    pub telegram: bool,
}

const ITEM_SELECT: &str = "select o.slug as org, p.slug, p.name, g.role, m.handle, \
    (select count(*) from grants g2 join members x on x.id = g2.member_id where g2.project_id = p.id and x.kind = 'human') as members, \
    (select count(*) from grants g2 join members x on x.id = g2.member_id where g2.project_id = p.id and x.kind = 'agent') as agents, \
    (select count(*) from messages x where x.project_id = p.id) as messages, \
    (select count(*) from handoffs h where h.project_id = p.id and h.status = 'open') as open_handoffs, \
    (select max(x.created_at) from messages x where x.project_id = p.id) as last_activity, \
    p.tg_chat_id is not null as telegram \
    from members m join orgs o on o.id = m.org_id join grants g on g.member_id = m.id \
    join projects p on p.id = g.project_id where m.user_id = $1 and m.kind = 'human'";

#[derive(Serialize)]
pub struct Overview {
    user: crate::accounts::UserView,
    telegram: TelegramState,
    projects: Vec<ProjectItem>,
}

#[derive(Serialize)]
struct TelegramState {
    linked: bool,
}

pub async fn overview(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
) -> AppResult<Json<Overview>> {
    let linked: bool = sqlx::query_scalar("select tg_user_id is not null from users where id = $1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    let projects: Vec<ProjectItem> = sqlx::query_as(&format!(
        "{ITEM_SELECT} order by last_activity desc nulls last, p.slug"
    ))
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(Overview {
        user,
        telegram: TelegramState { linked },
        projects,
    }))
}

#[derive(Serialize)]
pub struct LinkResponse {
    code: String,
    bot: String,
    deep_link: String,
    expires_in: i64,
}

pub async fn telegram_link(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    headers: HeaderMap,
) -> AppResult<Json<LinkResponse>> {
    require_csrf(&headers)?;
    let tg = state
        .telegram
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("this hub has no Telegram bot configured".into()))?;
    let mut b = [0u8; 12];
    rand::rng().fill_bytes(&mut b);
    let code = hex::encode(b);
    sqlx::query("delete from web_links where user_id = $1 and used_at is null")
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    sqlx::query("insert into web_links (code, user_id, expires_at) values ($1, $2, now() + make_interval(secs => $3))")
        .bind(&code)
        .bind(user.id)
        .bind(LINK_TTL_SECS as f64)
        .execute(&state.pool)
        .await?;
    Ok(Json(LinkResponse {
        deep_link: format!("https://t.me/{}?start=link_{code}", tg.username),
        bot: tg.username.clone(),
        code,
        expires_in: LINK_TTL_SECS,
    }))
}

/// Called by the bot for `/start link_<code>`. Returns the reply text.
pub async fn complete_link(
    pool: &PgPool,
    code: &str,
    tg_user_id: i64,
    tg_username: Option<&str>,
) -> anyhow::Result<String> {
    let user: Option<(Uuid,)> = sqlx::query_as(
        "update web_links set used_at = now() where code = $1 and used_at is null and expires_at > now() \
         returning user_id",
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    let Some((user_id,)) = user else {
        return Ok("This link has expired. Open Davra and press “Connect Telegram” again.".into());
    };
    let taken: Option<Uuid> =
        sqlx::query_scalar("select id from users where tg_user_id = $1 and id <> $2")
            .bind(tg_user_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    if taken.is_some() {
        return Ok("This Telegram account is already linked to another Davra account.".into());
    }
    sqlx::query("update users set tg_user_id = $2 where id = $1")
        .bind(user_id)
        .bind(tg_user_id)
        .execute(pool)
        .await?;
    // Existing members with this Telegram id now belong to the account...
    sqlx::query("update members set user_id = $1 where tg_user_id = $2 and kind = 'human' and user_id is null")
        .bind(user_id)
        .bind(tg_user_id)
        .execute(pool)
        .await?;
    // ...and members created on the web learn their Telegram id.
    sqlx::query(
        "update members m set tg_user_id = $2 where m.user_id = $1 and m.tg_user_id is null \
         and not exists (select 1 from members x where x.org_id = m.org_id and x.tg_user_id = $2)",
    )
    .bind(user_id)
    .bind(tg_user_id)
    .execute(pool)
    .await?;
    let projects: Vec<String> = sqlx::query_scalar(
        "select o.slug || '/' || p.slug from members m join orgs o on o.id = m.org_id \
         join grants g on g.member_id = m.id join projects p on p.id = g.project_id \
         where m.user_id = $1 order by 1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    // Use the Telegram username as the handle, so @mentions notify the person.
    let mut renamed: Vec<String> = Vec::new();
    if let Some(u) = tg_username.and_then(normalize_handle) {
        let mine: Vec<(Uuid, String)> = sqlx::query_as(
            "select m.id, o.slug from members m join orgs o on o.id = m.org_id \
             where m.user_id = $1 and m.kind = 'human'",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        for (mid, org) in mine {
            sqlx::query("update members set tg_username = $2 where id = $1")
                .bind(mid)
                .bind(&u)
                .execute(pool)
                .await?;
            if let db::Rename::Done { .. } = db::rename_member(pool, mid, &u).await? {
                renamed.push(org);
            }
        }
    }
    let projects: Vec<String> = projects;
    let rename_note = match (renamed.is_empty(), tg_username) {
        (false, Some(u)) => format!("\nYour name in Davra is now @{}.", u.to_ascii_lowercase()),
        (_, Some(_)) => String::new(),
        // Without a username, mentions still reach the person (as a link on
        // their name), but a username is easier for everyone to type.
        (_, None) => "\nTip: set a Telegram username (Settings → Username) so teammates and agents can @mention you by it. \
                      Until then mentions show your name and still notify you."
            .to_string(),
    };
    db::audit(
        pool,
        None,
        None,
        "user.link_telegram",
        Some(&user_id.to_string()),
        serde_json::json!({}),
    )
    .await?;
    Ok(if projects.is_empty() {
        "Linked ✓ No Davra project uses this Telegram account yet.".into()
    } else {
        format!(
            "Linked ✓ Your projects: {}{rename_note}",
            projects.join(", ")
        )
    })
}

#[derive(Serialize)]
pub struct TokenItem {
    id: Uuid,
    name: Option<String>,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct MemberItem {
    handle: String,
    kind: MemberKind,
    display_name: Option<String>,
    owner: Option<String>,
    roles: Vec<String>,
    role: String,
    last_seen: Option<DateTime<Utc>>,
    is_me: bool,
    mine: bool,
    tokens: Vec<TokenItem>,
}

async fn members(pool: &PgPool, p: &Project, me: Uuid) -> AppResult<Vec<MemberItem>> {
    type Row = (
        Uuid,
        String,
        String,
        Option<String>,
        Option<String>,
        Vec<String>,
        String,
        Option<Uuid>,
        Option<DateTime<Utc>>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "select m.id, m.handle, m.kind, m.display_name, ow.handle, m.roles, g.role, m.owner_id, \
         (select max(t.last_used_at) from tokens t where t.member_id = m.id) \
         from grants g join members m on m.id = g.member_id left join members ow on ow.id = m.owner_id \
         where g.project_id = $1 order by m.kind desc, m.handle",
    )
    .bind(p.id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, handle, kind, display_name, owner, roles, role, owner_id, last_seen) in rows {
        let mine = owner_id == Some(me);
        let tokens = if mine {
            type TokenRow = (Uuid, Option<String>, DateTime<Utc>, Option<DateTime<Utc>>);
            let t: Vec<TokenRow> = sqlx::query_as(
                "select id, name, created_at, last_used_at from tokens where member_id = $1 and revoked_at is null \
                 order by created_at",
            )
            .bind(id)
            .fetch_all(pool)
            .await?;
            t.into_iter()
                .map(|(id, name, created_at, last_used_at)| TokenItem {
                    id,
                    name,
                    created_at,
                    last_used_at,
                })
                .collect()
        } else {
            vec![]
        };
        out.push(MemberItem {
            handle,
            kind: MemberKind::parse(&kind).unwrap_or(MemberKind::Human),
            display_name,
            owner,
            roles,
            role,
            last_seen,
            is_me: id == me,
            mine,
            tokens,
        });
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct ProjectHead {
    org: String,
    slug: String,
    name: Option<String>,
    role: String,
    handle: String,
    telegram: bool,
}

#[derive(Serialize)]
pub struct Stats {
    messages: i64,
    messages_7d: i64,
    open_handoffs: i64,
    done_handoffs_7d: i64,
    agents: i64,
}

#[derive(Serialize)]
pub struct ProjectDetail {
    project: ProjectHead,
    stats: Stats,
    members: Vec<MemberItem>,
    messages: Vec<MessageView>,
    handoffs: Vec<HandoffView>,
}

pub async fn project_detail(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
) -> AppResult<Json<ProjectDetail>> {
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    let pool = &state.pool;
    let (messages_7d, open_handoffs, done_7d, agents): (i64, i64, i64, i64) = sqlx::query_as(
        "select (select count(*) from messages where project_id = $1 and created_at > now() - interval '7 days'), \
         (select count(*) from handoffs where project_id = $1 and status = 'open'), \
         (select count(*) from handoffs where project_id = $1 and status = 'done' and done_at > now() - interval '7 days'), \
         (select count(*) from grants g join members m on m.id = g.member_id where g.project_id = $1 and m.kind = 'agent')",
    )
    .bind(p.id)
    .fetch_one(pool)
    .await?;
    let reader = caller.reader();
    let messages = db::recent(pool, p.id, 30)
        .await?
        .into_iter()
        .map(|m| m.view(&reader))
        .collect();
    let handoffs = crate::handoffs::open_in_project(pool, p.id, 20).await?;
    Ok(Json(ProjectDetail {
        project: ProjectHead {
            org: caller.org_slug.clone(),
            slug: p.slug.clone(),
            name: p.name.clone(),
            role: p.role.clone(),
            handle: caller.handle.clone(),
            telegram: p.tg_chat_id.is_some(),
        },
        stats: Stats {
            messages: db::total_messages(pool, p.id).await?,
            messages_7d,
            open_handoffs,
            done_handoffs_7d: done_7d,
            agents,
        },
        members: members(pool, &p, caller.member_id).await?,
        messages,
        handoffs,
    }))
}

#[derive(Deserialize)]
pub struct BeforeQuery {
    before: i64,
}

#[derive(Serialize)]
pub struct MessagePage {
    messages: Vec<MessageView>,
}

pub async fn older_messages(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
    Query(q): Query<BeforeQuery>,
) -> AppResult<Json<MessagePage>> {
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    let reader = caller.reader();
    let messages = db::before(&state.pool, p.id, q.before, 30)
        .await?
        .into_iter()
        .map(|m| m.view(&reader))
        .collect();
    Ok(Json(MessagePage { messages }))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    k: Option<i64>,
}

pub async fn search(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResults>> {
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    let query = q.q.trim();
    if query.is_empty() || query.chars().count() > 300 {
        return Err(AppError::Field("q", "enter 1 to 300 characters".into()));
    }
    let qvec = match &state.embedder {
        Some(e) => e.embed_one(query).await.ok(),
        None => None,
    };
    let reader = caller.reader();
    let hits = db::search(
        &state.pool,
        p.id,
        query,
        q.k.unwrap_or(10).clamp(1, 20),
        qvec.as_deref(),
    )
    .await?
    .into_iter()
    .map(|(row, score)| {
        let v = row.view(&reader);
        SearchHit {
            id: v.id,
            created_at: v.created_at,
            author: v.author,
            trust: v.trust,
            snippet: davra_core::format::truncate_chars(&v.body, 280),
            score,
        }
    })
    .collect();
    Ok(Json(SearchResults {
        project: p.slug,
        query: query.to_string(),
        hits,
    }))
}

#[derive(Deserialize)]
pub struct PostBody {
    text: String,
}

pub async fn post(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
    headers: HeaderMap,
    Json(b): Json<PostBody>,
) -> AppResult<Json<PostResponse>> {
    require_csrf(&headers)?;
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    let req = PostRequest {
        text: b.text,
        ..Default::default()
    };
    Ok(Json(crate::api::post_as(&state, &caller, &p, req).await?))
}

#[derive(Deserialize)]
pub struct NewAgent {
    handle: String,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Serialize)]
pub struct AgentResponse {
    agent: MemberItem,
}

fn clean_roles(raw: &[String]) -> Vec<String> {
    let joined = raw.join(",");
    davra_core::handoff_file::split_labels(&joined)
}

async fn agent_item(pool: &PgPool, p: &Project, me: Uuid, handle: &str) -> AppResult<MemberItem> {
    members(pool, p, me)
        .await?
        .into_iter()
        .find(|m| m.handle == handle)
        .ok_or_else(|| AppError::NotFound(format!("agent {handle} not found")))
}

pub async fn create_agent(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
    headers: HeaderMap,
    Json(b): Json<NewAgent>,
) -> AppResult<Json<AgentResponse>> {
    require_csrf(&headers)?;
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    if p.role == "viewer" {
        return Err(AppError::Forbidden("viewers cannot add agents".into()));
    }
    let handle = normalize_handle(&b.handle)
        .ok_or_else(|| AppError::Field("handle", "use a–z, 0–9, - _ . (up to 64)".into()))?;
    let roles = clean_roles(&b.roles);
    let id: Option<Uuid> = sqlx::query_scalar(
        "insert into members (org_id, kind, handle, owner_id, roles) values ($1, 'agent', $2, $3, $4) \
         on conflict (org_id, handle) do nothing returning id",
    )
    .bind(p.org_id)
    .bind(&handle)
    .bind(caller.member_id)
    .bind(&roles)
    .fetch_optional(&state.pool)
    .await?;
    let id = id.ok_or_else(|| {
        AppError::FieldConflict("handle", format!("{handle} is already taken in this org"))
    })?;
    sqlx::query("insert into grants (member_id, project_id, role) values ($1, $2, 'agent')")
        .bind(id)
        .bind(p.id)
        .execute(&state.pool)
        .await?;
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "agent.create",
        Some(&handle),
        serde_json::json!({ "project": p.slug }),
    )
    .await?;
    Ok(Json(AgentResponse {
        agent: agent_item(&state.pool, &p, caller.member_id, &handle).await?,
    }))
}

/// The agent's id, if the caller owns it or is owner/maintainer of the project.
async fn manageable_agent(
    pool: &PgPool,
    caller: &Caller,
    p: &Project,
    handle: &str,
    owner_only: bool,
) -> AppResult<Uuid> {
    let row: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "select m.id, m.owner_id from members m join grants g on g.member_id = m.id \
         where g.project_id = $1 and m.handle = $2 and m.kind = 'agent'",
    )
    .bind(p.id)
    .bind(handle)
    .fetch_optional(pool)
    .await?;
    let (id, owner) = row.ok_or_else(|| AppError::NotFound(format!("agent {handle} not found")))?;
    let mine = owner == Some(caller.member_id);
    let admin = p.role == "owner" || p.role == "maintainer";
    if mine || (!owner_only && admin) {
        Ok(id)
    } else {
        Err(AppError::Forbidden(
            "only the agent's owner can do this".into(),
        ))
    }
}

#[derive(Deserialize)]
pub struct RolesBody {
    roles: Vec<String>,
}

pub async fn set_roles(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug, handle)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(b): Json<RolesBody>,
) -> AppResult<Json<AgentResponse>> {
    require_csrf(&headers)?;
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    let id = manageable_agent(&state.pool, &caller, &p, &handle, false).await?;
    let roles = clean_roles(&b.roles);
    sqlx::query("update members set roles = $2 where id = $1")
        .bind(id)
        .bind(&roles)
        .execute(&state.pool)
        .await?;
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "member.roles",
        Some(&handle),
        serde_json::json!({ "roles": roles }),
    )
    .await?;
    Ok(Json(AgentResponse {
        agent: agent_item(&state.pool, &p, caller.member_id, &handle).await?,
    }))
}

#[derive(Deserialize)]
pub struct TokenBody {
    name: Option<String>,
}

#[derive(Serialize)]
pub struct NewToken {
    id: Uuid,
    token: String,
    hub: String,
    commands: Vec<String>,
}

pub async fn create_token(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug, handle)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(b): Json<TokenBody>,
) -> AppResult<Json<NewToken>> {
    require_csrf(&headers)?;
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    let id = manageable_agent(&state.pool, &caller, &p, &handle, true).await?;
    let name = b
        .name
        .map(|n| davra_core::format::truncate_chars(n.trim(), 60))
        .filter(|n| !n.is_empty());
    let token = crate::auth::new_token();
    let tid: Uuid = sqlx::query_scalar(
        "insert into tokens (member_id, token_hash, name) values ($1, $2, $3) returning id",
    )
    .bind(id)
    .bind(crate::auth::hash_token(&token))
    .bind(&name)
    .fetch_one(&state.pool)
    .await?;
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "token.create",
        Some(&handle),
        serde_json::json!({ "token_id": tid, "via": "web" }),
    )
    .await?;
    let hub = state
        .public_url
        .clone()
        .unwrap_or_else(|| "https://your-hub".into());
    let commands = vec![
        format!("davra login --hub {hub} --project {}", p.slug),
        "claude mcp add --scope user davra -- davra mcp".to_string(),
        "davra setup claude".to_string(),
    ];
    Ok(Json(NewToken {
        id: tid,
        token,
        hub,
        commands,
    }))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    require_csrf(&headers)?;
    let org: Option<Uuid> = sqlx::query_scalar(
        "update tokens t set revoked_at = now() from members a, members o \
         where t.id = $1 and a.id = t.member_id and o.id = a.owner_id and o.user_id = $2 and t.revoked_at is null \
         returning a.org_id",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?;
    let org = org.ok_or_else(|| AppError::NotFound("token not found".into()))?;
    db::audit(
        &state.pool,
        Some(org),
        None,
        "token.revoke",
        None,
        serde_json::json!({ "token_id": id, "via": "web" }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct NewProject {
    org: String,
    org_name: Option<String>,
    slug: String,
    name: Option<String>,
    handle: String,
}

#[derive(Serialize)]
pub struct ProjectResponse {
    project: ProjectItem,
}

pub async fn create_project(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    headers: HeaderMap,
    Json(b): Json<NewProject>,
) -> AppResult<Json<ProjectResponse>> {
    require_csrf(&headers)?;
    let bad = |f: &'static str| AppError::Field(f, "use a–z, 0–9, - _ . (up to 64)".into());
    let org = normalize_handle(&b.org).ok_or_else(|| bad("org"))?;
    let slug = normalize_handle(&b.slug).ok_or_else(|| bad("slug"))?;
    let handle = normalize_handle(&b.handle).ok_or_else(|| bad("handle"))?;
    let mut tx = state.pool.begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("select id from orgs where slug = $1")
        .bind(&org)
        .fetch_optional(&mut *tx)
        .await?;
    let (org_id, member_id) = match existing {
        Some(org_id) => {
            let me: Option<(Uuid, String)> = sqlx::query_as(
                "select m.id, m.handle from members m where m.org_id = $1 and m.user_id = $2 and m.kind = 'human'",
            )
            .bind(org_id)
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some((mid, _)) = me else {
                return Err(AppError::FieldConflict(
                    "org",
                    format!("{org} is taken; pick another name"),
                ));
            };
            let can: bool = sqlx::query_scalar(
                "select exists (select 1 from grants where member_id = $1 and role in ('owner', 'maintainer'))",
            )
            .bind(mid)
            .fetch_one(&mut *tx)
            .await?;
            if !can {
                return Err(AppError::Forbidden(
                    "only owners or maintainers can add projects to this org".into(),
                ));
            }
            (org_id, mid)
        }
        None => {
            let org_id: Uuid =
                sqlx::query_scalar("insert into orgs (slug, name) values ($1, $2) returning id")
                    .bind(&org)
                    .bind(
                        b.org_name
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty()),
                    )
                    .fetch_one(&mut *tx)
                    .await?;
            let tg: Option<i64> = sqlx::query_scalar("select tg_user_id from users where id = $1")
                .bind(user.id)
                .fetch_one(&mut *tx)
                .await?;
            let mid: Uuid = sqlx::query_scalar(
                "insert into members (org_id, kind, handle, display_name, user_id, tg_user_id) \
                 values ($1, 'human', $2, $3, $4, $5) returning id",
            )
            .bind(org_id)
            .bind(&handle)
            .bind(&user.name)
            .bind(user.id)
            .bind(tg)
            .fetch_one(&mut *tx)
            .await?;
            (org_id, mid)
        }
    };
    let pid: Option<Uuid> = sqlx::query_scalar(
        "insert into projects (org_id, slug, name) values ($1, $2, $3) on conflict (org_id, slug) do nothing returning id",
    )
    .bind(org_id)
    .bind(&slug)
    .bind(b.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .fetch_optional(&mut *tx)
    .await?;
    let pid =
        pid.ok_or_else(|| AppError::FieldConflict("slug", format!("{org}/{slug} already exists")))?;
    sqlx::query("insert into grants (member_id, project_id, role) values ($1, $2, 'owner')")
        .bind(member_id)
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    db::audit(
        &state.pool,
        Some(org_id),
        Some(member_id),
        "project.create",
        Some(&slug),
        serde_json::json!({ "via": "web" }),
    )
    .await?;
    let project: ProjectItem = sqlx::query_as(&format!("{ITEM_SELECT} and p.id = $2"))
        .bind(user.id)
        .bind(pid)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(ProjectResponse { project }))
}
