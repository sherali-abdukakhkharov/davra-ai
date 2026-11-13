//! Team management from the web app: single-use invite links and the roles
//! of the people in a project.
//!
//! Who may do what, by project role:
//! - owner: invite or manage maintainers, members and viewers;
//! - maintainer: invite or manage members and viewers;
//! - member, viewer: nothing, but anyone may leave (except an owner).
//!
//! Owners are only made or changed from the CLI (`davra-hub grant`).

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use chrono::{DateTime, Utc};
use davra_core::mentions::normalize_handle;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::accounts::{require_csrf, SessionUser};
use crate::db::{self, Caller, NewMessage, Project};
use crate::error::{AppError, AppResult};
use crate::webapp::{members, open_project, MemberItem};
use crate::AppState;

const INVITE_TTL_DAYS: i64 = 7;

/// Whether someone with project role `actor` may hand out, change or remove `role`.
pub fn outranks(actor: &str, role: &str) -> bool {
    match actor {
        "owner" => matches!(role, "maintainer" | "member" | "viewer"),
        "maintainer" => matches!(role, "member" | "viewer"),
        _ => false,
    }
}

fn is_admin(role: &str) -> bool {
    role == "owner" || role == "maintainer"
}

async fn admin_project(
    state: &AppState,
    user: Uuid,
    org: &str,
    slug: &str,
) -> AppResult<(Caller, Project)> {
    let (caller, p) = open_project(state, user, org, slug).await?;
    if !is_admin(&p.role) {
        return Err(AppError::Forbidden(
            "only owners and maintainers manage the team".into(),
        ));
    }
    Ok((caller, p))
}

fn new_code() -> String {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

fn hash_code(code: &str) -> Vec<u8> {
    crate::auth::hash_token(code)
}

#[derive(Serialize)]
pub struct InviteItem {
    id: Uuid,
    role: String,
    /// The existing member this link hands over, if any.
    member: Option<String>,
    created_by: Option<String>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

type InviteRow = (
    Uuid,
    String,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
);

fn invite_item((id, role, member, created_by, created_at, expires_at): InviteRow) -> InviteItem {
    InviteItem {
        id,
        role,
        member,
        created_by,
        created_at,
        expires_at,
    }
}

const INVITE_SELECT: &str = "select i.id, i.role, m.handle, c.handle, i.created_at, i.expires_at \
    from invites i left join members m on m.id = i.member_id left join members c on c.id = i.created_by";

async fn open_invites(pool: &PgPool, project_id: Uuid) -> AppResult<Vec<InviteItem>> {
    let rows: Vec<InviteRow> = sqlx::query_as(&format!(
        "{INVITE_SELECT} where i.project_id = $1 and i.used_at is null and i.revoked_at is null \
         and i.expires_at > now() order by i.created_at desc"
    ))
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(invite_item).collect())
}

#[derive(Serialize)]
pub struct InviteList {
    invites: Vec<InviteItem>,
}

pub async fn list_invites(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
) -> AppResult<Json<InviteList>> {
    let (_, p) = admin_project(&state, user.id, &org, &slug).await?;
    Ok(Json(InviteList {
        invites: open_invites(&state.pool, p.id).await?,
    }))
}

#[derive(Deserialize)]
pub struct NewInvite {
    role: String,
    /// Hand over this existing member (one without a web account).
    member: Option<String>,
}

#[derive(Serialize)]
pub struct InviteCreated {
    invite: InviteItem,
    /// Shown once: only a hash of the code is stored.
    url: String,
}

pub async fn create_invite(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
    headers: HeaderMap,
    Json(b): Json<NewInvite>,
) -> AppResult<Json<InviteCreated>> {
    require_csrf(&headers)?;
    let (caller, p) = admin_project(&state, user.id, &org, &slug).await?;
    let role = b.role.trim().to_ascii_lowercase();
    if !outranks(&p.role, &role) {
        return Err(AppError::Field(
            "role",
            if p.role == "owner" {
                "pick maintainer, member or viewer".into()
            } else {
                "maintainers can invite members and viewers".into()
            },
        ));
    }
    let member_id = match b.member.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(h) => {
            let row: Option<(Uuid, Option<Uuid>, Option<String>)> = sqlx::query_as(
                "select m.id, m.user_id, g.role from members m \
                 left join grants g on g.member_id = m.id and g.project_id = $3 \
                 where m.org_id = $1 and m.handle = $2 and m.kind = 'human'",
            )
            .bind(p.org_id)
            .bind(h.trim_start_matches('@').to_ascii_lowercase())
            .bind(p.id)
            .fetch_optional(&state.pool)
            .await?;
            let (id, account, current) = row
                .ok_or_else(|| AppError::Field("member", format!("no person @{h} in this org")))?;
            if account.is_some() {
                return Err(AppError::Field(
                    "member",
                    format!("@{h} already has a Davra account"),
                ));
            }
            if let Some(current) = current {
                if !outranks(&p.role, &current) {
                    return Err(AppError::Field(
                        "member",
                        format!("you can't change @{h}'s role"),
                    ));
                }
            }
            Some(id)
        }
    };
    let code = new_code();
    let id: Uuid = sqlx::query_scalar(
        "insert into invites (code_hash, project_id, role, member_id, created_by, expires_at) \
         values ($1, $2, $3, $4, $5, now() + make_interval(days => $6)) returning id",
    )
    .bind(hash_code(&code))
    .bind(p.id)
    .bind(&role)
    .bind(member_id)
    .bind(caller.member_id)
    .bind(INVITE_TTL_DAYS as i32)
    .fetch_one(&state.pool)
    .await?;
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "invite.create",
        Some(&p.slug),
        serde_json::json!({ "invite": id, "role": role, "member": b.member }),
    )
    .await?;
    let row: InviteRow = sqlx::query_as(&format!("{INVITE_SELECT} where i.id = $1"))
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    let hub = crate::downloads::hub_url(&state, &headers);
    Ok(Json(InviteCreated {
        invite: invite_item(row),
        url: format!("{hub}/invite/{code}"),
    }))
}

pub async fn revoke_invite(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug, id)): Path<(String, String, Uuid)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    require_csrf(&headers)?;
    let (caller, p) = admin_project(&state, user.id, &org, &slug).await?;
    let done = sqlx::query(
        "update invites set revoked_at = now() where id = $1 and project_id = $2 \
         and used_at is null and revoked_at is null",
    )
    .bind(id)
    .bind(p.id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if done == 0 {
        return Err(AppError::NotFound("invite not found".into()));
    }
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "invite.revoke",
        Some(&p.slug),
        serde_json::json!({ "invite": id }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct InviteInfo {
    org: String,
    slug: String,
    name: Option<String>,
    role: String,
    invited_by: Option<String>,
    member: Option<String>,
    /// open, used, expired or revoked.
    status: &'static str,
}

fn status_of(
    used: Option<DateTime<Utc>>,
    revoked: Option<DateTime<Utc>>,
    expires: DateTime<Utc>,
) -> &'static str {
    if used.is_some() {
        "used"
    } else if revoked.is_some() {
        "revoked"
    } else if expires <= Utc::now() {
        "expired"
    } else {
        "open"
    }
}

/// Public: what an invite link is for, so the page can say it before sign-in.
pub async fn invite_info(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<Json<InviteInfo>> {
    type Row = (
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
        DateTime<Utc>,
    );
    let row: Option<Row> = sqlx::query_as(
        "select o.slug, p.slug, p.name, i.role, c.handle, m.handle, i.used_at, i.revoked_at, i.expires_at \
         from invites i join projects p on p.id = i.project_id join orgs o on o.id = p.org_id \
         left join members c on c.id = i.created_by left join members m on m.id = i.member_id \
         where i.code_hash = $1",
    )
    .bind(hash_code(code.trim()))
    .fetch_optional(&state.pool)
    .await?;
    let (org, slug, name, role, invited_by, member, used, revoked, expires) =
        row.ok_or_else(|| AppError::NotFound("this invite link is not valid".into()))?;
    Ok(Json(InviteInfo {
        org,
        slug,
        name,
        role,
        invited_by,
        member,
        status: status_of(used, revoked, expires),
    }))
}

#[derive(Deserialize, Default)]
pub struct AcceptBody {
    handle: Option<String>,
}

#[derive(Serialize)]
pub struct Accepted {
    org: String,
    slug: String,
    handle: String,
    /// The user was already in the project; nothing changed.
    already: bool,
}

pub async fn accept_invite(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path(code): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> AppResult<Json<Accepted>> {
    require_csrf(&headers)?;
    let b: AcceptBody = if body.iter().all(u8::is_ascii_whitespace) {
        AcceptBody::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| AppError::BadRequest("body is not JSON".into()))?
    };
    let mut tx = state.pool.begin().await?;
    type Row = (
        Uuid,
        Uuid,
        Uuid,
        String,
        String,
        Option<i64>,
        String,
        Option<Uuid>,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
        DateTime<Utc>,
    );
    let row: Option<Row> = sqlx::query_as(
        "select i.id, p.id, p.org_id, o.slug, p.slug, p.tg_chat_id, i.role, i.member_id, \
         i.used_at, i.revoked_at, i.expires_at \
         from invites i join projects p on p.id = i.project_id join orgs o on o.id = p.org_id \
         where i.code_hash = $1 for update of i",
    )
    .bind(hash_code(code.trim()))
    .fetch_optional(&mut *tx)
    .await?;
    let (invite_id, pid, org_id, org, slug, chat, role, target, used, revoked, expires) =
        row.ok_or_else(|| AppError::NotFound("this invite link is not valid".into()))?;
    match status_of(used, revoked, expires) {
        "open" => {}
        "used" => return Err(AppError::BadRequest("this invite was already used".into())),
        "revoked" => return Err(AppError::BadRequest("this invite was withdrawn".into())),
        _ => return Err(AppError::BadRequest("this invite has expired".into())),
    }

    let mine: Option<(Uuid, String)> = sqlx::query_as(
        "select id, handle from members where org_id = $1 and user_id = $2 and kind = 'human'",
    )
    .bind(org_id)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    let tg: Option<i64> = sqlx::query_scalar("select tg_user_id from users where id = $1")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    let (member_id, handle, claimed) = match (mine, target) {
        (Some((m, h)), Some(t)) if m != t => {
            return Err(AppError::BadRequest(format!(
                "you are already @{h} in {org}; ask for an invite that isn't for another person"
            )))
        }
        (Some((m, h)), _) => (m, h, false),
        (None, Some(t)) => {
            let claimed: Option<String> = sqlx::query_scalar(
                "update members set user_id = $2 where id = $1 and user_id is null returning handle",
            )
            .bind(t)
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?;
            let h = claimed.ok_or_else(|| {
                AppError::BadRequest("this person already has a Davra account".into())
            })?;
            if let Some(tg) = tg {
                sqlx::query(
                    "update members m set tg_user_id = $2 where m.id = $1 and m.tg_user_id is null \
                     and not exists (select 1 from members x where x.org_id = m.org_id and x.tg_user_id = $2)",
                )
                .bind(t)
                .bind(tg)
                .execute(&mut *tx)
                .await?;
            }
            (t, h, true)
        }
        (None, None) => {
            // A Telegram username known from another org is the best handle.
            let known: Option<String> = sqlx::query_scalar(
                "select tg_username from members where user_id = $1 and tg_username is not null limit 1",
            )
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?;
            let raw = b.handle.as_deref().map(str::trim).filter(|s| !s.is_empty());
            let handle = match raw {
                Some(h) => normalize_handle(h).ok_or_else(|| {
                    AppError::Field("handle", "use a–z, 0–9, - _ . (up to 64)".into())
                })?,
                None => known
                    .clone()
                    .ok_or_else(|| AppError::Field("handle", "pick a handle".into()))?,
            };
            let tg_free = match tg {
                Some(tg) => sqlx::query_scalar::<_, bool>(
                    "select not exists (select 1 from members where org_id = $1 and tg_user_id = $2)",
                )
                .bind(org_id)
                .bind(tg)
                .fetch_one(&mut *tx)
                .await?,
                None => false,
            };
            let id: Option<Uuid> = sqlx::query_scalar(
                "insert into members (org_id, kind, handle, display_name, user_id, tg_user_id, tg_username) \
                 values ($1, 'human', $2, $3, $4, $5, $6) on conflict (org_id, handle) do nothing returning id",
            )
            .bind(org_id)
            .bind(&handle)
            .bind(&user.name)
            .bind(user.id)
            .bind(if tg_free { tg } else { None })
            .bind(known.filter(|k| *k == handle))
            .fetch_optional(&mut *tx)
            .await?;
            let id = id.ok_or_else(|| {
                AppError::FieldConflict("handle", format!("@{handle} is taken in {org}"))
            })?;
            (id, handle, false)
        }
    };

    let current: Option<String> =
        sqlx::query_scalar("select role from grants where member_id = $1 and project_id = $2")
            .bind(member_id)
            .bind(pid)
            .fetch_optional(&mut *tx)
            .await?;
    let already = match current {
        // A hand-over invite sets the role the admin picked.
        Some(c) if target == Some(member_id) && c != role && c != "owner" => {
            sqlx::query("update grants set role = $3 where member_id = $1 and project_id = $2")
                .bind(member_id)
                .bind(pid)
                .bind(&role)
                .execute(&mut *tx)
                .await?;
            false
        }
        // Claiming a CLI member that already had this role is still a join.
        Some(_) => !claimed,
        None => {
            sqlx::query("insert into grants (member_id, project_id, role) values ($1, $2, $3)")
                .bind(member_id)
                .bind(pid)
                .bind(&role)
                .execute(&mut *tx)
                .await?;
            false
        }
    };
    // An invite for someone already inside stays usable for whoever it was meant for.
    if !already {
        sqlx::query("update invites set used_at = now(), used_by = $2 where id = $1")
            .bind(invite_id)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    if !already {
        db::audit(
            &state.pool,
            Some(org_id),
            Some(member_id),
            "invite.accept",
            Some(&slug),
            serde_json::json!({ "invite": invite_id, "role": role }),
        )
        .await?;
        let text = format!("@{handle} joined the project as {role}.");
        if let Some(id) = db::insert_message(
            &state.pool,
            &NewMessage {
                org_id,
                project_id: pid,
                author_id: None,
                author_name: Some("davra".into()),
                kind: "system",
                topic: None,
                body: text.clone(),
                reply_to: None,
                mentions: vec![],
                source: "api",
                tg_message_id: None,
                tg_thread_id: None,
                created_at: None,
            },
        )
        .await?
        {
            // The join is done; a Telegram hiccup must not undo it for the user.
            let mention = format!("@{handle}");
            if let Err(e) = crate::api::relay(
                &state,
                chat,
                id,
                &mention,
                |lang, people| crate::tgcard::joined(lang, &handle, &role, people),
                None,
            )
            .await
            {
                tracing::warn!(error = ?e, "could not relay the join notice");
            }
        }
    }
    Ok(Json(Accepted {
        org,
        slug,
        handle,
        already,
    }))
}

/// A person or agent in the project: (member id, kind, project role, agent owner).
async fn project_member(
    pool: &PgPool,
    p: &Project,
    handle: &str,
) -> AppResult<(Uuid, String, String, Option<Uuid>)> {
    let row: Option<(Uuid, String, String, Option<Uuid>)> = sqlx::query_as(
        "select m.id, m.kind, g.role, m.owner_id from members m join grants g on g.member_id = m.id \
         where g.project_id = $1 and m.handle = $2",
    )
    .bind(p.id)
    .bind(handle.trim_start_matches('@').to_ascii_lowercase())
    .fetch_optional(pool)
    .await?;
    row.ok_or_else(|| AppError::NotFound(format!("@{handle} is not in this project")))
}

#[derive(Deserialize)]
pub struct MemberRoleBody {
    role: String,
}

#[derive(Serialize)]
pub struct MemberList {
    members: Vec<MemberItem>,
}

pub async fn set_member_role(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug, handle)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(b): Json<MemberRoleBody>,
) -> AppResult<Json<MemberList>> {
    require_csrf(&headers)?;
    let (caller, p) = admin_project(&state, user.id, &org, &slug).await?;
    let (id, kind, current, _) = project_member(&state.pool, &p, &handle).await?;
    let role = b.role.trim().to_ascii_lowercase();
    if kind != "human" {
        return Err(AppError::BadRequest(
            "agents always have the agent role".into(),
        ));
    }
    if id == caller.member_id {
        return Err(AppError::Forbidden("you can't change your own role".into()));
    }
    if !outranks(&p.role, &current) {
        return Err(AppError::Forbidden(format!(
            "you can't change @{handle}'s role"
        )));
    }
    if !outranks(&p.role, &role) {
        return Err(AppError::Field(
            "role",
            format!("you can't make someone {role}"),
        ));
    }
    sqlx::query("update grants set role = $3 where member_id = $1 and project_id = $2")
        .bind(id)
        .bind(p.id)
        .bind(&role)
        .execute(&state.pool)
        .await?;
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "grant.role",
        Some(&handle),
        serde_json::json!({ "project": p.slug, "from": current, "to": role }),
    )
    .await?;
    Ok(Json(MemberList {
        members: members(&mut *state.pool.acquire().await?, &p, caller.member_id).await?,
    }))
}

/// Takes a person (with their agents) or an agent out of the project.
/// Their messages stay; they can be invited back.
pub async fn remove_member(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug, handle)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    require_csrf(&headers)?;
    let (caller, p) = open_project(&state, user.id, &org, &slug).await?;
    let (id, kind, role, owner) = project_member(&state.pool, &p, &handle).await?;
    let allowed = if kind == "agent" {
        owner == Some(caller.member_id) || is_admin(&p.role)
    } else if id == caller.member_id {
        role != "owner"
    } else {
        outranks(&p.role, &role)
    };
    if !allowed {
        return Err(AppError::Forbidden(if id == caller.member_id {
            "an owner can't leave; ask the hub admin to transfer ownership first".into()
        } else {
            format!("you can't remove @{handle}")
        }));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "delete from grants where project_id = $1 and (member_id = $2 or member_id in \
         (select id from members where owner_id = $2))",
    )
    .bind(p.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("delete from cursors where project_id = $1 and member_id = $2")
        .bind(p.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        if id == caller.member_id {
            "grant.leave"
        } else {
            "grant.remove"
        },
        Some(&handle),
        serde_json::json!({ "project": p.slug, "kind": kind }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_outranks_whom() {
        assert!(outranks("owner", "maintainer"));
        assert!(outranks("owner", "viewer"));
        assert!(!outranks("owner", "owner"));
        assert!(!outranks("owner", "agent"));
        assert!(outranks("maintainer", "member"));
        assert!(!outranks("maintainer", "maintainer"));
        assert!(!outranks("member", "viewer"));
        assert!(!outranks("viewer", "viewer"));
    }

    #[test]
    fn invite_status() {
        let later = Utc::now() + chrono::Duration::days(1);
        let earlier = Utc::now() - chrono::Duration::days(1);
        assert_eq!(status_of(None, None, later), "open");
        assert_eq!(status_of(Some(earlier), None, later), "used");
        assert_eq!(status_of(None, Some(earlier), later), "revoked");
        assert_eq!(status_of(None, None, earlier), "expired");
    }
}
