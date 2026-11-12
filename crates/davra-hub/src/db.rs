//! All SQL lives here. Queries are checked at runtime (no compile-time DB).

use anyhow::Result;
use chrono::{DateTime, Utc};
use davra_core::api::{MemberKind, MemberRef, MessageView, ProjectRef};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::trust::{trust_for, Author, Reader};

/// The authenticated member making a request.
#[derive(Clone, Debug)]
pub struct Caller {
    pub member_id: Uuid,
    pub org_id: Uuid,
    pub org_slug: String,
    pub kind: MemberKind,
    pub handle: String,
    pub display_name: Option<String>,
    pub owner_id: Option<Uuid>,
    pub owner_handle: Option<String>,
    pub roles: Vec<String>,
}

impl Caller {
    /// Handle plus roles: what a handoff can be addressed to.
    pub fn labels(&self) -> Vec<String> {
        let mut l = vec![self.handle.clone()];
        l.extend(self.roles.iter().cloned());
        l
    }

    pub fn reader(&self) -> Reader {
        Reader {
            id: self.member_id,
            owner_id: self.owner_id,
        }
    }

    pub fn member_ref(&self) -> MemberRef {
        MemberRef {
            handle: self.handle.clone(),
            kind: self.kind,
            display_name: self.display_name.clone(),
            owner: self.owner_handle.clone(),
            roles: self.roles.clone(),
        }
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub org_id: Uuid,
    pub slug: String,
    pub name: Option<String>,
    pub tg_chat_id: Option<i64>,
    pub role: String,
}

impl Project {
    pub fn as_ref(&self) -> ProjectRef {
        ProjectRef {
            slug: self.slug.clone(),
            name: self.name.clone(),
            role: self.role.clone(),
        }
    }
}

#[derive(Debug, FromRow)]
pub struct MsgRow {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub author_id: Option<Uuid>,
    pub author_handle: Option<String>,
    pub author_name: Option<String>,
    pub author_kind: Option<String>,
    pub author_owner: Option<Uuid>,
    pub kind: String,
    pub topic: Option<String>,
    pub body: String,
    pub reply_to: Option<i64>,
    pub mentions: Vec<String>,
}

impl MsgRow {
    pub fn view(self, reader: &Reader) -> MessageView {
        let author_kind = self.author_kind.as_deref().and_then(MemberKind::parse);
        let trust = trust_for(
            reader,
            &Author {
                id: self.author_id,
                kind: author_kind,
                owner_id: self.author_owner,
            },
        );
        MessageView {
            id: self.id,
            created_at: self.created_at,
            author: self
                .author_handle
                .or(self.author_name)
                .unwrap_or_else(|| "unknown".into()),
            author_kind,
            trust,
            kind: self.kind,
            topic: self.topic,
            body: self.body,
            reply_to: self.reply_to,
            mentions: self.mentions,
        }
    }
}

const MSG_SELECT: &str = "select m.id, m.created_at, m.author_id, a.handle as author_handle, \
    coalesce(a.display_name, m.author_name) as author_name, a.kind as author_kind, \
    a.owner_id as author_owner, m.kind, m.topic, m.body, m.reply_to, m.mentions \
    from messages m left join members a on a.id = m.author_id";

/// Messages addressed to a member: @handle, @all, @agents (for agents) or a
/// reply to one of the member's own messages. $1 project, $2 after id,
/// $3 member id, $4 handle, $5 is agent.
const INBOX_WHERE: &str = "m.project_id = $1 and m.id > $2 and m.author_id is distinct from $3 \
    and ($4 = any(m.mentions) or 'all' = any(m.mentions) or ($5 and 'agents' = any(m.mentions)) \
    or m.reply_to in (select r.id from messages r where r.project_id = $1 and r.author_id = $3))";

#[derive(FromRow)]
struct CallerRow {
    member_id: Uuid,
    org_id: Uuid,
    org_slug: String,
    kind: String,
    handle: String,
    display_name: Option<String>,
    owner_id: Option<Uuid>,
    owner_handle: Option<String>,
    roles: Vec<String>,
    token_id: Uuid,
}

pub async fn caller_by_token_hash(pool: &PgPool, hash: &[u8]) -> Result<Option<Caller>> {
    let row: Option<CallerRow> = sqlx::query_as(
        "select m.id as member_id, m.org_id, o.slug as org_slug, m.kind, m.handle, m.display_name, \
         m.owner_id, ow.handle as owner_handle, m.roles, t.id as token_id \
         from tokens t join members m on m.id = t.member_id join orgs o on o.id = m.org_id \
         left join members ow on ow.id = m.owner_id \
         where t.token_hash = $1 and t.revoked_at is null",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else { return Ok(None) };
    sqlx::query(
        "update tokens set last_used_at = now() where id = $1 \
         and (last_used_at is null or last_used_at < now() - interval '1 minute')",
    )
    .bind(r.token_id)
    .execute(pool)
    .await?;
    Ok(Some(Caller {
        member_id: r.member_id,
        org_id: r.org_id,
        org_slug: r.org_slug,
        kind: MemberKind::parse(&r.kind).unwrap_or(MemberKind::Human),
        handle: r.handle,
        display_name: r.display_name,
        owner_id: r.owner_id,
        owner_handle: r.owner_handle,
        roles: r.roles,
    }))
}

pub async fn project_for(pool: &PgPool, caller: &Caller, slug: &str) -> Result<Option<Project>> {
    Ok(sqlx::query_as(
        "select p.id, p.org_id, p.slug, p.name, p.tg_chat_id, g.role \
         from projects p join grants g on g.project_id = p.id \
         where g.member_id = $1 and p.org_id = $2 and p.slug = $3",
    )
    .bind(caller.member_id)
    .bind(caller.org_id)
    .bind(slug)
    .fetch_optional(pool)
    .await?)
}

pub async fn projects_of(pool: &PgPool, caller: &Caller) -> Result<Vec<ProjectRef>> {
    let rows: Vec<Project> = sqlx::query_as(
        "select p.id, p.org_id, p.slug, p.name, p.tg_chat_id, g.role \
         from projects p join grants g on g.project_id = p.id \
         where g.member_id = $1 order by p.slug",
    )
    .bind(caller.member_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(Project::as_ref).collect())
}

pub async fn members_of_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<MemberRef>> {
    type Row = (String, String, Option<String>, Option<String>, Vec<String>);
    let rows: Vec<Row> = sqlx::query_as(
        "select m.handle, m.kind, m.display_name, ow.handle, m.roles from grants g \
         join members m on m.id = g.member_id left join members ow on ow.id = m.owner_id \
         where g.project_id = $1 order by m.kind desc, m.handle",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(handle, kind, display_name, owner, roles)| MemberRef {
            handle,
            kind: MemberKind::parse(&kind).unwrap_or(MemberKind::Human),
            display_name,
            owner,
            roles,
        })
        .collect())
}

pub async fn recent(pool: &PgPool, project_id: Uuid, limit: i64) -> Result<Vec<MsgRow>> {
    let mut rows: Vec<MsgRow> = sqlx::query_as(&format!(
        "{MSG_SELECT} where m.project_id = $1 order by m.id desc limit $2"
    ))
    .bind(project_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

/// Up to `limit` messages older than `before`, oldest first.
pub async fn before(
    pool: &PgPool,
    project_id: Uuid,
    before: i64,
    limit: i64,
) -> Result<Vec<MsgRow>> {
    let mut rows: Vec<MsgRow> = sqlx::query_as(&format!(
        "{MSG_SELECT} where m.project_id = $1 and m.id < $2 order by m.id desc limit $3"
    ))
    .bind(project_id)
    .bind(before)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

pub async fn total_messages(pool: &PgPool, project_id: Uuid) -> Result<i64> {
    Ok(
        sqlx::query_scalar("select count(*) from messages where project_id = $1")
            .bind(project_id)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn cursor(pool: &PgPool, member_id: Uuid, project_id: Uuid) -> Result<i64> {
    Ok(
        sqlx::query_scalar("select last_ack from cursors where member_id = $1 and project_id = $2")
            .bind(member_id)
            .bind(project_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or(0),
    )
}

/// Returns up to `limit` unread addressed messages, and whether more exist.
pub async fn inbox(
    pool: &PgPool,
    caller: &Caller,
    project: &Project,
    after: i64,
    limit: i64,
) -> Result<(Vec<MsgRow>, bool)> {
    let mut rows: Vec<MsgRow> = sqlx::query_as(&format!(
        "{MSG_SELECT} where {INBOX_WHERE} order by m.id asc limit $6"
    ))
    .bind(project.id)
    .bind(after)
    .bind(caller.member_id)
    .bind(&caller.handle)
    .bind(caller.kind == MemberKind::Agent)
    .bind(limit + 1)
    .fetch_all(pool)
    .await?;
    let more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    Ok((rows, more))
}

pub async fn unread_count(
    pool: &PgPool,
    caller: &Caller,
    project: &Project,
    after: i64,
) -> Result<i64> {
    Ok(sqlx::query_scalar(&format!(
        "select count(*) from messages m where {INBOX_WHERE}"
    ))
    .bind(project.id)
    .bind(after)
    .bind(caller.member_id)
    .bind(&caller.handle)
    .bind(caller.kind == MemberKind::Agent)
    .fetch_one(pool)
    .await?)
}

/// Moves the cursor forward (never backwards, never past the last message).
pub async fn ack(pool: &PgPool, member_id: Uuid, project_id: Uuid, up_to: i64) -> Result<i64> {
    let max: i64 =
        sqlx::query_scalar("select coalesce(max(id), 0) from messages where project_id = $1")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    let target = up_to.clamp(0, max);
    Ok(sqlx::query_scalar(
        "insert into cursors (member_id, project_id, last_ack) values ($1, $2, $3) \
         on conflict (member_id, project_id) do update \
         set last_ack = greatest(cursors.last_ack, excluded.last_ack) returning last_ack",
    )
    .bind(member_id)
    .bind(project_id)
    .bind(target)
    .fetch_one(pool)
    .await?)
}

/// True when the last `n` messages in the project were all written by agents.
pub async fn last_n_by_agents(pool: &PgPool, project_id: Uuid, n: i64) -> Result<bool> {
    let kinds: Vec<Option<String>> = sqlx::query_scalar(
        "select a.kind from messages m left join members a on a.id = m.author_id \
         where m.project_id = $1 order by m.id desc limit $2",
    )
    .bind(project_id)
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok(kinds.len() as i64 == n && kinds.iter().all(|k| k.as_deref() == Some("agent")))
}

pub struct NewMessage {
    pub org_id: Uuid,
    pub project_id: Uuid,
    pub author_id: Option<Uuid>,
    pub author_name: Option<String>,
    pub kind: &'static str,
    pub topic: Option<String>,
    pub body: String,
    pub reply_to: Option<i64>,
    pub mentions: Vec<String>,
    pub source: &'static str,
    pub tg_message_id: Option<i64>,
    pub tg_thread_id: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Inserts a message. Returns `None` when a Telegram message was already stored.
pub async fn insert_message(pool: &PgPool, m: &NewMessage) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar(
        "insert into messages (org_id, project_id, author_id, author_name, kind, topic, body, reply_to, \
         mentions, source, tg_message_id, tg_thread_id, created_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, coalesce($13, now())) \
         on conflict (project_id, tg_message_id) where tg_message_id is not null do nothing \
         returning id",
    )
    .bind(m.org_id)
    .bind(m.project_id)
    .bind(m.author_id)
    .bind(&m.author_name)
    .bind(m.kind)
    .bind(&m.topic)
    .bind(&m.body)
    .bind(m.reply_to)
    .bind(&m.mentions)
    .bind(m.source)
    .bind(m.tg_message_id)
    .bind(m.tg_thread_id)
    .bind(m.created_at)
    .fetch_optional(pool)
    .await?)
}

pub async fn set_tg_ids(
    pool: &PgPool,
    id: i64,
    tg_message_id: i64,
    tg_thread_id: Option<i64>,
) -> Result<()> {
    sqlx::query("update messages set tg_message_id = $2, tg_thread_id = coalesce($3, tg_thread_id) where id = $1")
        .bind(id)
        .bind(tg_message_id)
        .bind(tg_thread_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub struct ReplyTarget {
    pub id: i64,
    pub author_handle: Option<String>,
    pub tg_message_id: Option<i64>,
    pub tg_thread_id: Option<i64>,
}

pub async fn reply_target(pool: &PgPool, project_id: Uuid, id: i64) -> Result<Option<ReplyTarget>> {
    type Row = (i64, Option<String>, Option<i64>, Option<i64>);
    let row: Option<Row> = sqlx::query_as(
        "select m.id, a.handle, m.tg_message_id, m.tg_thread_id from messages m \
         left join members a on a.id = m.author_id where m.project_id = $1 and m.id = $2",
    )
    .bind(project_id)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(id, author_handle, tg_message_id, tg_thread_id)| ReplyTarget {
            id,
            author_handle,
            tg_message_id,
            tg_thread_id,
        },
    ))
}

pub async fn reply_target_by_tg(
    pool: &PgPool,
    project_id: Uuid,
    tg_id: i64,
) -> Result<Option<ReplyTarget>> {
    let id: Option<i64> =
        sqlx::query_scalar("select id from messages where project_id = $1 and tg_message_id = $2")
            .bind(project_id)
            .bind(tg_id)
            .fetch_optional(pool)
            .await?;
    match id {
        Some(id) => reply_target(pool, project_id, id).await,
        None => Ok(None),
    }
}

/// Hybrid lexical search: full-text ('simple' config, works for Uzbek,
/// Russian and English alike) fused with trigram similarity for typos and
/// partial words, using Reciprocal Rank Fusion (k = 60).
pub async fn search(
    pool: &PgPool,
    project_id: Uuid,
    q: &str,
    k: i64,
    qvec: Option<&str>,
) -> Result<Vec<(MsgRow, f64)>> {
    let scored: Vec<(i64, f64)> = sqlx::query_as(
        "with fts as ( \
           select m.id, row_number() over (order by ts_rank_cd(m.tsv, q.tq) desc, m.id desc) as r \
           from messages m, websearch_to_tsquery('simple', $2) q(tq) \
           where m.project_id = $1 and m.tsv @@ q.tq order by r limit 20), \
         tri as ( \
           select m.id, row_number() over (order by word_similarity($2, m.body) desc, m.id desc) as r \
           from messages m where m.project_id = $1 and word_similarity($2, m.body) >= 0.4 \
           order by r limit 20), \
         vec as ( \
           select m.id, row_number() over (order by m.embedding <=> $4::vector, m.id desc) as r \
           from messages m where $4::text is not null and m.project_id = $1 and m.embedding is not null \
             and m.embedding <=> $4::vector < 0.65 \
           order by m.embedding <=> $4::vector limit 20) \
         select id, sum(1.0 / (60 + r))::float8 as score \
         from (select * from fts union all select * from tri union all select * from vec) x \
         group by id order by score desc, id desc limit $3",
    )
    .bind(project_id)
    .bind(q)
    .bind(k)
    .bind(qvec)
    .fetch_all(pool)
    .await?;
    if scored.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<i64> = scored.iter().map(|(id, _)| *id).collect();
    let rows: Vec<MsgRow> = sqlx::query_as(&format!("{MSG_SELECT} where m.id = any($1)"))
        .bind(&ids)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    let mut rows: Vec<Option<MsgRow>> = rows.into_iter().map(Some).collect();
    for (id, score) in scored {
        if let Some(slot) = rows
            .iter_mut()
            .find(|r| r.as_ref().is_some_and(|r| r.id == id))
        {
            out.push((slot.take().expect("checked above"), score));
        }
    }
    Ok(out)
}

pub async fn message_project(pool: &PgPool, id: i64) -> Result<Option<(Uuid, String)>> {
    Ok(sqlx::query_as(
        "select p.org_id, p.slug from messages m join projects p on p.id = m.project_id where m.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn message_by_id(pool: &PgPool, id: i64) -> Result<Option<MsgRow>> {
    Ok(sqlx::query_as(&format!("{MSG_SELECT} where m.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn around(
    pool: &PgPool,
    project_id: Uuid,
    id: i64,
    n: i64,
) -> Result<(Vec<MsgRow>, Vec<MsgRow>)> {
    let mut before: Vec<MsgRow> = sqlx::query_as(&format!(
        "{MSG_SELECT} where m.project_id = $1 and m.id < $2 order by m.id desc limit $3"
    ))
    .bind(project_id)
    .bind(id)
    .bind(n)
    .fetch_all(pool)
    .await?;
    before.reverse();
    let after: Vec<MsgRow> = sqlx::query_as(&format!(
        "{MSG_SELECT} where m.project_id = $1 and m.id > $2 order by m.id asc limit $3"
    ))
    .bind(project_id)
    .bind(id)
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok((before, after))
}

pub struct ChatProject {
    pub id: Uuid,
    pub org_id: Uuid,
    pub slug: String,
}

pub async fn project_by_tg_chat(pool: &PgPool, chat_id: i64) -> Result<Option<ChatProject>> {
    let row: Option<(Uuid, Uuid, String)> =
        sqlx::query_as("select id, org_id, slug from projects where tg_chat_id = $1")
            .bind(chat_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(id, org_id, slug)| ChatProject { id, org_id, slug }))
}

pub async fn member_by_tg_user(
    pool: &PgPool,
    org_id: Uuid,
    tg_user_id: i64,
) -> Result<Option<Uuid>> {
    Ok(
        sqlx::query_scalar("select id from members where org_id = $1 and tg_user_id = $2")
            .bind(org_id)
            .bind(tg_user_id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn update_tg_message(
    pool: &PgPool,
    project_id: Uuid,
    tg_message_id: i64,
    body: &str,
    mentions: &[String],
) -> Result<()> {
    sqlx::query(
        "update messages set body = $3, mentions = $4, edited_at = now() \
         where project_id = $1 and tg_message_id = $2",
    )
    .bind(project_id)
    .bind(tg_message_id)
    .bind(body)
    .bind(mentions)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum Rename {
    Done { old: String },
    Same,
    Taken,
}

/// Renames a member and keeps history addressable: mentions in stored
/// messages and handoff labels follow the new handle.
pub async fn rename_member(pool: &PgPool, member_id: Uuid, new_handle: &str) -> Result<Rename> {
    let mut tx = pool.begin().await?;
    let (org_id, old): (Uuid, String) =
        sqlx::query_as("select org_id, handle from members where id = $1 for update")
            .bind(member_id)
            .fetch_one(&mut *tx)
            .await?;
    if old == new_handle {
        return Ok(Rename::Same);
    }
    let taken: bool = sqlx::query_scalar(
        "select exists (select 1 from members where org_id = $1 and handle = $2)",
    )
    .bind(org_id)
    .bind(new_handle)
    .fetch_one(&mut *tx)
    .await?;
    if taken {
        return Ok(Rename::Taken);
    }
    sqlx::query("update members set handle = $2 where id = $1")
        .bind(member_id)
        .bind(new_handle)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "update messages set mentions = array_replace(mentions, $2, $3) where org_id = $1 and $2 = any(mentions)",
    )
    .bind(org_id)
    .bind(&old)
    .bind(new_handle)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "update handoffs set to_labels = array_replace(to_labels, $2, $3) where org_id = $1 and $2 = any(to_labels)",
    )
    .bind(org_id)
    .bind(&old)
    .bind(new_handle)
    .execute(&mut *tx)
    .await?;
    sqlx::query("update handoffs set from_label = $3 where org_id = $1 and from_label = $2")
        .bind(org_id)
        .bind(&old)
        .bind(new_handle)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit(
        pool,
        Some(org_id),
        Some(member_id),
        "member.rename",
        Some(new_handle),
        serde_json::json!({ "from": old }),
    )
    .await?;
    Ok(Rename::Done { old })
}

pub async fn kv_get(pool: &PgPool, key: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("select value from kv where key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?)
}

pub async fn kv_set(pool: &PgPool, key: &str, value: &str) -> Result<()> {
    sqlx::query("insert into kv (key, value) values ($1, $2) on conflict (key) do update set value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn audit(
    pool: &PgPool,
    org_id: Option<Uuid>,
    actor_id: Option<Uuid>,
    action: &str,
    target: Option<&str>,
    detail: serde_json::Value,
) -> Result<()> {
    sqlx::query("insert into audit_log (org_id, actor_id, action, target, detail) values ($1, $2, $3, $4, $5)")
        .bind(org_id)
        .bind(actor_id)
        .bind(action)
        .bind(target)
        .bind(sqlx::types::Json(detail))
        .execute(pool)
        .await?;
    Ok(())
}
