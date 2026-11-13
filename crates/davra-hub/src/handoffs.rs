//! Handoffs: one agent (or role) asks another to do something because a change
//! affects their area. They mirror the repository's handoff files when a
//! project uses that convention; the file in git stays the source of truth.

use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, NaiveDate, Utc};
use davra_core::api::*;
use davra_core::handoff_file::split_labels;
use davra_core::mentions::normalize_handle;
use davra_core::{format, redact};
use serde::Deserialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::api::{project_in, relay, scoped};
use crate::db::{self, Caller, NewMessage, Project};
use crate::error::{AppError, AppResult};
use crate::AppState;

const MAX_BODY: usize = 100_000;
const MAX_SYNC_ITEMS: usize = 200;

#[derive(FromRow)]
struct Row {
    id: i64,
    ticket: Option<String>,
    from_label: String,
    to_labels: Vec<String>,
    status: String,
    body: String,
    done_note: Option<String>,
    source_path: Option<String>,
    source_hash: Option<String>,
    created_at: DateTime<Utc>,
    done_at: Option<DateTime<Utc>>,
    message_id: Option<i64>,
}

impl Row {
    fn view(self) -> HandoffView {
        HandoffView {
            id: self.id,
            ticket: self.ticket,
            from: self.from_label,
            to: self.to_labels,
            status: self.status,
            body: self.body,
            done_note: self.done_note,
            source_path: self.source_path,
            created_at: self.created_at,
            done_at: self.done_at,
            message_id: self.message_id,
        }
    }
}

const SELECT: &str =
    "select id, ticket, from_label, to_labels, status, body, done_note, source_path, \
    source_hash, created_at, done_at, message_id from handoffs";

pub async fn open_in_project(
    pool: impl sqlx::PgExecutor<'_>,
    project_id: Uuid,
    limit: i64,
) -> Result<Vec<HandoffView>> {
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "{SELECT} where project_id = $1 and status = 'open' order by id desc limit $2"
    ))
    .bind(project_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Row::view).collect())
}

pub async fn open_for(
    pool: impl sqlx::PgExecutor<'_>,
    caller: &Caller,
    p: &Project,
    limit: i64,
) -> Result<Vec<HandoffView>> {
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "{SELECT} where project_id = $1 and status = 'open' and to_labels && $2 order by id desc limit $3"
    ))
    .bind(p.id)
    .bind(caller.labels())
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Row::view).collect())
}

/// Members of the project addressed by these labels (handle or role).
async fn addressed_handles(
    pool: &PgPool,
    project_id: Uuid,
    labels: &[String],
) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select m.handle from grants g join members m on m.id = g.member_id \
         where g.project_id = $1 and (m.handle = any($2) or m.roles && $2) order by m.handle",
    )
    .bind(project_id)
    .bind(labels)
    .fetch_all(pool)
    .await?)
}

fn clean_labels(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in raw {
        for l in split_labels(r) {
            if !out.contains(&l) {
                out.push(l);
            }
        }
    }
    out
}

fn ticket_part(t: &Option<String>) -> String {
    t.as_deref().map(|t| format!(" · {t}")).unwrap_or_default()
}

/// Posts the "new handoff" message to the room and links it.
async fn announce_new(
    state: &AppState,
    p: &Project,
    author: Option<&Caller>,
    h: &HandoffView,
) -> AppResult<i64> {
    let mut mentions = addressed_handles(&state.pool, p.id, &h.to).await?;
    for l in &h.to {
        if !mentions.contains(l) {
            mentions.push(l.clone());
        }
    }
    let body = format!(
        "Handoff #{}{} · {} → {}\n{}",
        h.id,
        ticket_part(&h.ticket),
        h.from,
        h.to.join(", "),
        format::truncate_chars(&h.body, 1200)
    );
    let id = db::insert_message(
        &state.pool,
        &NewMessage {
            org_id: p.org_id,
            project_id: p.id,
            author_id: author.map(|c| c.member_id),
            author_name: Some(h.from.clone()),
            kind: "handoff",
            topic: Some("handoffs".into()),
            body: body.clone(),
            reply_to: None,
            mentions,
            source: "api",
            tg_message_id: None,
            tg_thread_id: None,
            created_at: None,
        },
    )
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("insert returned no id")))?;
    sqlx::query("update handoffs set message_id = $2 where id = $1")
        .bind(h.id)
        .bind(id)
        .execute(&state.pool)
        .await?;
    relay(
        state,
        p.tg_chat_id,
        id,
        &h.body,
        |lang, people| {
            crate::tgcard::handoff_new(
                lang,
                h.id,
                h.ticket.as_deref(),
                &h.from,
                &h.to,
                &h.body,
                people,
            )
        },
        None,
    )
    .await?;
    Ok(id)
}

/// Posts "done" as a reply to the handoff's announcement, addressed to the sender.
async fn announce_done(
    state: &AppState,
    p: &Project,
    by: Option<&Caller>,
    h: &HandoffView,
) -> AppResult<()> {
    let mut labels = vec![h.from.clone()];
    if let Some(c) = by {
        labels.retain(|l| l != &c.handle);
    }
    let mentions = addressed_handles(&state.pool, p.id, &labels).await?;
    let note = h.done_note.clone().unwrap_or_else(|| "closed".into());
    let body = format!(
        "Done · handoff #{}{} · {}",
        h.id,
        ticket_part(&h.ticket),
        note
    );
    let reply = match h.message_id {
        Some(m) => db::reply_target(&state.pool, p.id, m).await?,
        None => None,
    };
    let id = db::insert_message(
        &state.pool,
        &NewMessage {
            org_id: p.org_id,
            project_id: p.id,
            author_id: by.map(|c| c.member_id),
            author_name: Some(h.to.first().cloned().unwrap_or_default()),
            kind: "handoff",
            topic: Some("handoffs".into()),
            body: body.clone(),
            reply_to: reply.as_ref().map(|r| r.id),
            mentions,
            source: "api",
            tg_message_id: None,
            tg_thread_id: None,
            created_at: None,
        },
    )
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("insert returned no id")))?;
    let closer = by
        .map(|c| c.handle.clone())
        .unwrap_or_else(|| h.to.first().cloned().unwrap_or_default());
    let note_shown = format::truncate_chars(&note, 2000);
    relay(
        state,
        p.tg_chat_id,
        id,
        &note_shown,
        |lang, people| {
            crate::tgcard::handoff_done(
                lang,
                h.id,
                h.ticket.as_deref(),
                &closer,
                &note_shown,
                people,
            )
        },
        reply.as_ref(),
    )
    .await?;
    Ok(())
}

fn default_from(caller: &Caller) -> String {
    caller
        .roles
        .first()
        .cloned()
        .unwrap_or_else(|| caller.handle.clone())
}

struct Insert<'a> {
    p: &'a Project,
    author: Option<&'a Caller>,
    ticket: Option<String>,
    from: String,
    to: Vec<String>,
    status: &'a str,
    body: String,
    done_note: Option<String>,
    source_path: Option<String>,
    source_hash: Option<String>,
    created_at: Option<DateTime<Utc>>,
}

async fn insert(pool: impl sqlx::PgExecutor<'_>, i: Insert<'_>) -> Result<HandoffView> {
    let row: Row = sqlx::query_as(
        "insert into handoffs (org_id, project_id, ticket, from_label, to_labels, status, body, done_note, \
         source_path, source_hash, author_id, created_at, done_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, coalesce($12, now()), \
                 case when $6 = 'done' then now() end) \
         returning id, ticket, from_label, to_labels, status, body, done_note, source_path, source_hash, \
                   created_at, done_at, message_id",
    )
    .bind(i.p.org_id)
    .bind(i.p.id)
    .bind(&i.ticket)
    .bind(&i.from)
    .bind(&i.to)
    .bind(i.status)
    .bind(&i.body)
    .bind(&i.done_note)
    .bind(&i.source_path)
    .bind(&i.source_hash)
    .bind(i.author.map(|c| c.member_id))
    .bind(i.created_at)
    .fetch_one(pool)
    .await?;
    Ok(row.view())
}

fn validate_body(body: &str) -> AppResult<String> {
    let body = body.trim();
    if body.is_empty() {
        return Err(AppError::BadRequest("handoff body is empty".into()));
    }
    if body.chars().count() > MAX_BODY {
        return Err(AppError::BadRequest(format!(
            "handoff body is longer than {MAX_BODY} characters"
        )));
    }
    Ok(redact::redact(body).0)
}

fn writable(p: &Project) -> AppResult<()> {
    if p.role == "viewer" {
        return Err(AppError::Forbidden("viewers cannot change handoffs".into()));
    }
    Ok(())
}

pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(req): Json<HandoffCreate>,
) -> AppResult<Json<HandoffView>> {
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    writable(&p)?;
    let to = clean_labels(&req.to);
    if to.is_empty() {
        return Err(AppError::BadRequest(
            "a handoff needs at least one recipient (handle or role)".into(),
        ));
    }
    let from = req
        .from
        .as_deref()
        .and_then(normalize_handle)
        .unwrap_or_else(|| default_from(&caller));
    let h = insert(
        &mut *tx,
        Insert {
            p: &p,
            author: Some(&caller),
            ticket: req.ticket.filter(|t| !t.trim().is_empty()),
            from,
            to,
            status: "open",
            body: validate_body(&req.body)?,
            done_note: None,
            source_path: None,
            source_hash: None,
            created_at: None,
        },
    )
    .await?;
    tx.commit().await?;
    let mid = announce_new(&state, &p, Some(&caller), &h).await?;
    Ok(Json(HandoffView {
        message_id: Some(mid),
        ..h
    }))
}

#[derive(Deserialize)]
pub struct ListQuery {
    status: Option<String>,
    /// `me` (default): addressed to me; `all`: everything.
    scope: Option<String>,
    limit: Option<i64>,
}

pub async fn list(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<HandoffList>> {
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    let status = q.status.unwrap_or_else(|| "open".into());
    let all_status = status == "all";
    let all_scope = q.scope.as_deref() == Some("all");
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "{SELECT} where project_id = $1 and ($2 or status = $3) and ($4 or to_labels && $5 or from_label = any($5)) \
         order by id desc limit $6"
    ))
    .bind(p.id)
    .bind(all_status)
    .bind(&status)
    .bind(all_scope)
    .bind(caller.labels())
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(HandoffList {
        project: p.slug,
        handoffs: rows.into_iter().map(Row::view).collect(),
    }))
}

async fn load(tx: &mut sqlx::PgConnection, caller: &Caller, id: i64) -> AppResult<(Project, Row)> {
    let not_found = || AppError::NotFound(format!("handoff #{id} not found"));
    let found: Option<(Uuid, String)> = sqlx::query_as(
        "select p.org_id, p.slug from handoffs h join projects p on p.id = h.project_id where h.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (org, slug) = found.ok_or_else(not_found)?;
    if org != caller.org_id {
        return Err(not_found());
    }
    let p = db::project_for(&mut *tx, caller, &slug)
        .await?
        .ok_or_else(not_found)?;
    let row: Row = sqlx::query_as(&format!("{SELECT} where id = $1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    Ok((p, row))
}

pub async fn get_one(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<i64>,
) -> AppResult<Json<HandoffView>> {
    let mut tx = scoped(&state, &caller).await?;
    let (_, row) = load(&mut tx, &caller, id).await?;
    tx.commit().await?;
    Ok(Json(row.view()))
}

pub async fn done(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<i64>,
    Json(req): Json<HandoffDone>,
) -> AppResult<Json<HandoffView>> {
    let mut tx = scoped(&state, &caller).await?;
    let (p, row) = load(&mut tx, &caller, id).await?;
    writable(&p)?;
    if row.status == "done" {
        return Ok(Json(row.view()));
    }
    let note = redact::redact(req.note.trim()).0;
    let row: Row = sqlx::query_as(
        "update handoffs set status = 'done', done_note = $2, done_by = $3, done_at = now(), updated_at = now() \
         where id = $1 returning id, ticket, from_label, to_labels, status, body, done_note, source_path, \
         source_hash, created_at, done_at, message_id",
    )
    .bind(id)
    .bind(if note.is_empty() { None } else { Some(note) })
    .bind(caller.member_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let h = row.view();
    announce_done(&state, &p, Some(&caller), &h).await?;
    Ok(Json(h))
}

fn created_at(date: Option<&str>) -> Option<DateTime<Utc>> {
    let d = NaiveDate::parse_from_str(date?.trim(), "%Y-%m-%d").ok()?;
    Some(d.and_hms_opt(9, 0, 0)?.and_utc())
}

/// The note a receiving agent leaves: the first `Done:` line under the frontmatter.
fn done_note_from(body: &str) -> Option<String> {
    let first = body.lines().find(|l| !l.trim().is_empty())?;
    first
        .trim()
        .strip_prefix("Done:")
        .map(|n| n.trim().to_string())
}

pub async fn sync(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(req): Json<HandoffSync>,
) -> AppResult<Json<HandoffSyncResult>> {
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    writable(&p)?;
    if req.items.len() > MAX_SYNC_ITEMS {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_SYNC_ITEMS} items per sync"
        )));
    }
    let mut res = HandoffSyncResult::default();
    // Announced after the commit, so the room message can link committed rows.
    let mut announce: Vec<(bool, HandoffView)> = Vec::new();
    for item in req.items {
        let path = item.source_path.trim().replace('\\', "/");
        if path.is_empty() || path.contains("..") || path.starts_with('/') {
            return Err(AppError::BadRequest(format!(
                "bad source_path '{}'",
                item.source_path
            )));
        }
        let Some(from) = normalize_handle(&item.from) else {
            continue;
        };
        let to = clean_labels(&item.to);
        let status = if item.status == "done" {
            "done"
        } else {
            "open"
        };
        let body = validate_body(&item.body)?;
        let note = if status == "done" {
            done_note_from(&body)
        } else {
            None
        };

        let existing: Option<Row> = sqlx::query_as(&format!(
            "{SELECT} where project_id = $1 and source_path = $2"
        ))
        .bind(p.id)
        .bind(&path)
        .fetch_optional(&mut *tx)
        .await?;
        match existing {
            None => {
                let h = insert(
                    &mut *tx,
                    Insert {
                        p: &p,
                        author: Some(&caller),
                        ticket: item.ticket.clone(),
                        from,
                        to,
                        status,
                        body,
                        done_note: note,
                        source_path: Some(path),
                        source_hash: Some(item.source_hash.clone()),
                        created_at: created_at(item.created.as_deref()),
                    },
                )
                .await?;
                res.created += 1;
                res.touched.push(SyncedHandoff {
                    id: h.id,
                    source_path: h.source_path.clone().unwrap_or_default(),
                });
                if req.announce && status == "open" {
                    announce.push((true, h));
                }
            }
            Some(old) if old.source_hash.as_deref() == Some(item.source_hash.as_str()) => {
                res.unchanged += 1;
            }
            Some(old) => {
                let was = old.status.clone();
                let row: Row = sqlx::query_as(
                    "update handoffs set ticket = $2, from_label = $3, to_labels = $4, status = $5, body = $6, \
                     source_hash = $7, updated_at = now(), \
                     done_note = case when $5 = 'done' then coalesce($8, done_note) else null end, \
                     done_at = case when $5 = 'done' then coalesce(done_at, now()) else null end, \
                     done_by = case when $5 = 'done' then coalesce(done_by, $9) else null end \
                     where id = $1 returning id, ticket, from_label, to_labels, status, body, done_note, \
                     source_path, source_hash, created_at, done_at, message_id",
                )
                .bind(old.id)
                .bind(&item.ticket)
                .bind(&from)
                .bind(&to)
                .bind(status)
                .bind(&body)
                .bind(&item.source_hash)
                .bind(&note)
                .bind(caller.member_id)
                .fetch_one(&mut *tx)
                .await?;
                let h = row.view();
                res.touched.push(SyncedHandoff {
                    id: h.id,
                    source_path: h.source_path.clone().unwrap_or_default(),
                });
                match (was.as_str(), status) {
                    ("open", "done") => {
                        res.closed += 1;
                        if req.announce {
                            announce.push((false, h));
                        }
                    }
                    ("done", "open") => res.reopened += 1,
                    _ => res.updated += 1,
                }
            }
        }
    }
    tx.commit().await?;
    for (new, h) in &announce {
        if *new {
            announce_new(&state, &p, Some(&caller), h).await?;
        } else {
            announce_done(&state, &p, Some(&caller), h).await?;
        }
    }
    Ok(Json(res))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn done_note_is_first_line() {
        assert_eq!(
            done_note_from("\nDone: added counter\nWhat changed: x").as_deref(),
            Some("added counter")
        );
        assert_eq!(done_note_from("What changed: x"), None);
    }

    #[test]
    fn labels_are_cleaned() {
        assert_eq!(
            clean_labels(&["core, auth".into(), "@Core".into(), "db".into()]),
            vec!["core", "auth", "db"]
        );
    }

    #[test]
    fn created_dates() {
        assert_eq!(
            created_at(Some("2026-11-02")).unwrap().to_rfc3339(),
            "2026-11-02T09:00:00+00:00"
        );
        assert!(created_at(Some("yesterday")).is_none());
    }
}
