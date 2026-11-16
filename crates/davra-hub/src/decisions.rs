//! Decision log: what the team decided, who decided, why, and instead of
//! what. Agents record decisions (MCP `decision_record`); the hub keeps the
//! record, announces it in the room and Telegram, and the brief shows the
//! latest accepted ones. The repository keeps the same decision as a file
//! (docs/memory/decisions), whose path the client reports back.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use davra_core::api::{DecisionCreate, DecisionFilePath, DecisionList, DecisionView, MemberKind};
use davra_core::mentions::normalize_handle;
use davra_core::{format, redact};
use serde::Deserialize;
use uuid::Uuid;

use crate::accounts::SessionUser;
use crate::api::{project_in, relay, scoped};
use crate::db::{self, Caller, NewMessage, Project};
use crate::error::{AppError, AppResult};
use crate::AppState;

const MAX_TITLE: usize = 120;
const MAX_TEXT: usize = 2000;
const MAX_INSTEAD: usize = 1000;
const MAX_TICKET: usize = 40;

const SELECT: &str = "select d.id, d.ticket, d.title, d.decision, d.why, d.instead, d.decided_by, \
     a.handle as author, d.status, d.supersedes, d.superseded_by, d.source_message, d.file_path, \
     d.message_id, d.created_at from decisions d left join members a on a.id = d.author_id";

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    ticket: Option<String>,
    title: String,
    decision: String,
    why: String,
    instead: Option<String>,
    decided_by: String,
    author: Option<String>,
    status: String,
    supersedes: Option<i64>,
    superseded_by: Option<i64>,
    source_message: Option<i64>,
    file_path: Option<String>,
    message_id: Option<i64>,
    created_at: DateTime<Utc>,
}

impl Row {
    fn view(self) -> DecisionView {
        DecisionView {
            id: self.id,
            ticket: self.ticket,
            title: self.title,
            decision: self.decision,
            why: self.why,
            instead: self.instead,
            decided_by: self.decided_by,
            author: self.author,
            status: self.status,
            supersedes: self.supersedes,
            superseded_by: self.superseded_by,
            source_message: self.source_message,
            file_path: self.file_path,
            message_id: self.message_id,
            created_at: self.created_at,
        }
    }
}

/// The latest accepted decisions in a project, newest first (for the brief).
pub async fn latest(
    c: &mut sqlx::PgConnection,
    project: Uuid,
    n: i64,
) -> anyhow::Result<Vec<DecisionView>> {
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "{SELECT} where d.project_id = $1 and d.status = 'accepted' order by d.id desc limit $2"
    ))
    .bind(project)
    .bind(n)
    .fetch_all(&mut *c)
    .await?;
    Ok(rows.into_iter().map(Row::view).collect())
}

fn text(field: &'static str, v: &str, max: usize) -> AppResult<String> {
    let v = v.trim();
    if v.is_empty() {
        return Err(AppError::BadRequest(format!("{field} is empty")));
    }
    if v.chars().count() > max {
        return Err(AppError::BadRequest(format!(
            "{field} is longer than {max} characters"
        )));
    }
    Ok(redact::redact(v).0)
}

/// A title from the decision itself: its first line, cut to a short length.
fn default_title(decision: &str) -> String {
    let first = decision
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    format::truncate_chars(&format::one_line(first), 80)
}

pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(req): Json<DecisionCreate>,
) -> AppResult<Json<DecisionView>> {
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    if p.role == "viewer" {
        return Err(AppError::Forbidden(
            "viewers cannot record decisions".into(),
        ));
    }
    let decision = text("decision", &req.decision, MAX_TEXT)?;
    let why = text("why", &req.why, MAX_TEXT)?;
    let instead = match req
        .instead
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(i) => Some(text("instead", i, MAX_INSTEAD)?),
        None => None,
    };
    let title = match req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(t) => text("title", &format::one_line(t), MAX_TITLE)?,
        None => default_title(&decision),
    };
    let ticket = req
        .ticket
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| format::truncate_chars(t, MAX_TICKET));

    // Who decided: a person in this project. An agent's default is its owner.
    let decided_by = match req.decided_by.as_deref() {
        Some(h) => normalize_handle(h.trim_start_matches('@'))
            .ok_or_else(|| AppError::BadRequest(format!("decided_by '{h}' is not a handle")))?,
        None => match caller.kind {
            MemberKind::Agent => caller.owner_handle.clone().ok_or_else(|| {
                AppError::BadRequest("say who decided (decided_by): this agent has no owner".into())
            })?,
            MemberKind::Human => caller.handle.clone(),
        },
    };
    let person: Option<String> = sqlx::query_scalar(
        "select m.handle from members m join grants g on g.member_id = m.id \
         where g.project_id = $1 and m.kind = 'human' and m.handle = $2",
    )
    .bind(p.id)
    .bind(&decided_by)
    .fetch_optional(&mut *tx)
    .await?;
    if person.is_none() {
        return Err(AppError::BadRequest(format!(
            "@{decided_by} is not a person in this project; decided_by must name who decided"
        )));
    }

    if let Some(m) = req.source_message {
        let here: Option<i64> =
            sqlx::query_scalar("select id from messages where id = $1 and project_id = $2")
                .bind(m)
                .bind(p.id)
                .fetch_optional(&mut *tx)
                .await?;
        if here.is_none() {
            return Err(AppError::BadRequest(format!(
                "source_message #{m} is not in this project"
            )));
        }
    }
    if let Some(old) = req.supersedes {
        let open: Option<i64> = sqlx::query_scalar(
            "update decisions set status = 'superseded' where id = $1 and project_id = $2 \
             and status = 'accepted' returning id",
        )
        .bind(old)
        .bind(p.id)
        .fetch_optional(&mut *tx)
        .await?;
        if open.is_none() {
            return Err(AppError::BadRequest(format!(
                "D{old} is not an accepted decision in this project"
            )));
        }
    }

    let id: i64 = sqlx::query_scalar(
        "insert into decisions (org_id, project_id, author_id, decided_by, ticket, title, decision, why, \
         instead, supersedes, source_message) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) returning id",
    )
    .bind(p.org_id)
    .bind(p.id)
    .bind(caller.member_id)
    .bind(&decided_by)
    .bind(&ticket)
    .bind(&title)
    .bind(&decision)
    .bind(&why)
    .bind(&instead)
    .bind(req.supersedes)
    .bind(req.source_message)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(old) = req.supersedes {
        sqlx::query("update decisions set superseded_by = $2 where id = $1")
            .bind(old)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let d = one(&mut tx, id).await?;
    tx.commit().await?;

    let mid = announce(&state, &p, &caller, &d).await?;
    Ok(Json(DecisionView {
        message_id: Some(mid),
        ..d
    }))
}

async fn one(c: &mut sqlx::PgConnection, id: i64) -> AppResult<DecisionView> {
    let row: Row = sqlx::query_as(&format!("{SELECT} where d.id = $1"))
        .bind(id)
        .fetch_one(&mut *c)
        .await?;
    Ok(row.view())
}

/// Posts the decision in the room (kind `decision`: the loop guard skips it)
/// and relays it to the project's chat.
async fn announce(
    state: &AppState,
    p: &Project,
    caller: &Caller,
    d: &DecisionView,
) -> AppResult<i64> {
    let mut body = format!(
        "Decision D{}{} · decided by @{}\n{}\nDecision: {}\nWhy: {}",
        d.id,
        d.ticket
            .as_deref()
            .map(|t| format!(" · {t}"))
            .unwrap_or_default(),
        d.decided_by,
        d.title,
        format::truncate_chars(&d.decision, 1500),
        format::truncate_chars(&d.why, 800),
    );
    if let Some(i) = &d.instead {
        body.push_str(&format!("\nInstead of: {i}"));
    }
    if let Some(o) = d.supersedes {
        body.push_str(&format!("\nReplaces D{o}."));
    }
    let mid = db::insert_message(
        &state.pool,
        &NewMessage {
            org_id: p.org_id,
            project_id: p.id,
            author_id: Some(caller.member_id),
            author_name: None,
            kind: "decision",
            topic: Some("decisions".into()),
            body: body.clone(),
            reply_to: None,
            mentions: vec![],
            source: "api",
            tg_message_id: None,
            tg_thread_id: None,
            created_at: None,
        },
    )
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("insert returned no id")))?;
    sqlx::query("update decisions set message_id = $2 where id = $1")
        .bind(d.id)
        .bind(mid)
        .execute(&state.pool)
        .await?;
    relay(
        state,
        p.tg_chat_id,
        mid,
        &format!("@{}", d.decided_by),
        |lang, people| crate::tgcard::decision(lang, d, people),
        None,
    )
    .await?;
    Ok(mid)
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// `accepted` (default) or `all`.
    status: Option<String>,
    ticket: Option<String>,
    limit: Option<i64>,
}

async fn list_in(
    c: &mut sqlx::PgConnection,
    p: &Project,
    status: Option<&str>,
    ticket: Option<&str>,
    limit: i64,
) -> AppResult<DecisionList> {
    let all = status == Some("all");
    let ticket = ticket.map(str::trim).filter(|t| !t.is_empty());
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "{SELECT} where d.project_id = $1 and ($2 or d.status = 'accepted') \
         and ($3::text is null or lower(d.ticket) = lower($3)) order by d.id desc limit $4"
    ))
    .bind(p.id)
    .bind(all)
    .bind(ticket)
    .bind(limit)
    .fetch_all(&mut *c)
    .await?;
    Ok(DecisionList {
        project: p.slug.clone(),
        decisions: rows.into_iter().map(Row::view).collect(),
    })
}

pub async fn list(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<DecisionList>> {
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    let l = list_in(
        &mut tx,
        &p,
        q.status.as_deref(),
        q.ticket.as_deref(),
        q.limit.unwrap_or(20).clamp(1, 100),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(l))
}

/// A decision visible to the caller, with its project.
async fn load(
    c: &mut sqlx::PgConnection,
    caller: &Caller,
    id: i64,
) -> AppResult<(Project, DecisionView)> {
    let not_found = || AppError::NotFound(format!("decision D{id} not found"));
    let found: Option<(Uuid, String)> = sqlx::query_as(
        "select p.org_id, p.slug from decisions d join projects p on p.id = d.project_id where d.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *c)
    .await?;
    let (org, slug) = found.ok_or_else(not_found)?;
    if org != caller.org_id {
        return Err(not_found());
    }
    let p = db::project_for(&mut *c, caller, &slug)
        .await?
        .ok_or_else(not_found)?;
    Ok((p, one(c, id).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<i64>,
) -> AppResult<Json<DecisionView>> {
    let mut tx = scoped(&state, &caller).await?;
    let (_, d) = load(&mut tx, &caller, id).await?;
    tx.commit().await?;
    Ok(Json(d))
}

/// The client wrote the decision's file in the repository: remember where.
pub async fn set_file(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<i64>,
    Json(req): Json<DecisionFilePath>,
) -> AppResult<Json<DecisionView>> {
    let mut tx = scoped(&state, &caller).await?;
    let (p, _) = load(&mut tx, &caller, id).await?;
    if p.role == "viewer" {
        return Err(AppError::Forbidden(
            "viewers cannot change decisions".into(),
        ));
    }
    let path = req.path.trim().replace('\\', "/");
    let ok = !path.is_empty()
        && path.len() <= 300
        && path.ends_with(".md")
        && !path.starts_with('/')
        && !path.contains(':')
        && !path.split('/').any(|s| s == ".." || s.is_empty());
    if !ok {
        return Err(AppError::BadRequest(
            "path must be a relative .md path inside the repository".into(),
        ));
    }
    sqlx::query("update decisions set file_path = $2 where id = $1")
        .bind(id)
        .bind(&path)
        .execute(&mut *tx)
        .await?;
    let d = one(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(d))
}

/// Every decision in a project, for the web app.
pub async fn web_list(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
) -> AppResult<Json<DecisionList>> {
    let (caller, p) = crate::webapp::open_project(&state, user.id, &org, &slug).await?;
    let mut tx = scoped(&state, &caller).await?;
    let l = list_in(&mut tx, &p, Some("all"), None, 200).await?;
    tx.commit().await?;
    Ok(Json(l))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_from_the_first_line() {
        assert_eq!(
            default_title("\n  Use ElevenLabs.\nMore."),
            "Use ElevenLabs."
        );
        assert_eq!(default_title(&"x".repeat(200)).chars().count(), 80);
    }
}
