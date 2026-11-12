//! `ask_owner`: an agent asks its human owner to decide something
//! (architecture, merge, anything risky). The answer comes back as an
//! `[owner]` message, so it may instruct the agent.

use anyhow::Result;
use axum::extract::{Path, State};
use axum::Json;
use davra_core::api::MemberKind;
use davra_core::{format, redact};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::project;
use crate::db::{self, Caller, NewMessage};
use crate::error::{AppError, AppResult};
use crate::telegram::Telegram;
use crate::AppState;

const MAX_OPTIONS: usize = 6;

#[derive(Deserialize)]
pub struct AskRequest {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Serialize)]
pub struct AskResponse {
    pub id: i64,
    pub owner: String,
    pub message_id: i64,
    pub buttons: bool,
}

#[derive(Deserialize)]
pub struct AnswerRequest {
    pub answer: String,
}

#[derive(Serialize)]
pub struct AnswerResponse {
    pub id: i64,
    pub message_id: i64,
}

pub fn callback_data(qid: i64, idx: usize) -> String {
    format!("q:{qid}:{idx}")
}

pub fn parse_callback(data: &str) -> Option<(i64, usize)> {
    let mut it = data.strip_prefix("q:")?.splitn(2, ':');
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

pub async fn ask(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(req): Json<AskRequest>,
) -> AppResult<Json<AskResponse>> {
    let p = project(&state, &caller, &slug).await?;
    if caller.kind != MemberKind::Agent {
        return Err(AppError::Forbidden("only agents ask their owner".into()));
    }
    let owner_id = caller
        .owner_id
        .ok_or_else(|| AppError::BadRequest("this agent has no owner".into()))?;
    let owner = caller.owner_handle.clone().unwrap_or_default();
    let question = redact::redact(req.question.trim()).0;
    if question.is_empty() || question.chars().count() > 2000 {
        return Err(AppError::BadRequest(
            "question must be 1 to 2000 characters".into(),
        ));
    }
    let options: Vec<String> = req
        .options
        .iter()
        .map(|o| format::truncate_chars(&format::one_line(o), 60))
        .filter(|o| !o.is_empty())
        .take(MAX_OPTIONS)
        .collect();

    let (qid,): (i64,) = sqlx::query_as(
        "insert into questions (org_id, project_id, agent_id, owner_id, question, options) \
         values ($1, $2, $3, $4, $5, $6) returning id",
    )
    .bind(p.org_id)
    .bind(p.id)
    .bind(caller.member_id)
    .bind(owner_id)
    .bind(&question)
    .bind(&options)
    .fetch_one(&state.pool)
    .await?;

    let mut body = format!("Question #{qid} for @{owner}: {question}");
    for (i, o) in options.iter().enumerate() {
        body.push_str(&format!("\n{}. {}", i + 1, o));
    }
    let mid = db::insert_message(
        &state.pool,
        &NewMessage {
            org_id: p.org_id,
            project_id: p.id,
            author_id: Some(caller.member_id),
            author_name: None,
            kind: "chat",
            topic: Some("decisions".into()),
            body: body.clone(),
            reply_to: None,
            mentions: vec![owner.clone()],
            source: "api",
            tg_message_id: None,
            tg_thread_id: None,
            created_at: None,
        },
    )
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("insert returned no id")))?;
    sqlx::query("update questions set message_id = $2 where id = $1")
        .bind(qid)
        .bind(mid)
        .execute(&state.pool)
        .await?;

    let mut buttons = false;
    if let (Some(tg), Some(chat)) = (&state.telegram, p.tg_chat_id) {
        let text = format!("❓ {}\n{}", caller.handle, body);
        let labels: Vec<(String, String)> = options
            .iter()
            .enumerate()
            .map(|(i, o)| (o.clone(), callback_data(qid, i)))
            .collect();
        let people = crate::telegram::mentioned_people(&state.pool, p.org_id, &text).await?;
        let (text, entities) = crate::telegram::render_mentions(&text, &people);
        match tg.send_with_buttons(chat, &text, &entities, &labels).await {
            Ok(tg_id) => {
                db::set_tg_ids(&state.pool, mid, tg_id, None).await?;
                buttons = !labels.is_empty();
            }
            Err(e) => tracing::warn!(error = %e, "telegram question relay failed"),
        }
    }
    Ok(Json(AskResponse {
        id: qid,
        owner,
        message_id: mid,
        buttons,
    }))
}

struct Open {
    org_id: Uuid,
    project_id: Uuid,
    owner_id: Uuid,
    agent_handle: String,
    options: Vec<String>,
    message_id: Option<i64>,
    answered: bool,
    tg_chat_id: Option<i64>,
}

async fn load(pool: &PgPool, id: i64) -> Result<Option<Open>> {
    type Row = (
        Uuid,
        Uuid,
        Uuid,
        String,
        Vec<String>,
        Option<i64>,
        bool,
        Option<i64>,
    );
    let row: Option<Row> = sqlx::query_as(
        "select q.org_id, q.project_id, q.owner_id, a.handle, q.options, q.message_id, q.answer is not null, p.tg_chat_id \
         from questions q join members a on a.id = q.agent_id join projects p on p.id = q.project_id where q.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(
            org_id,
            project_id,
            owner_id,
            agent_handle,
            options,
            message_id,
            answered,
            tg_chat_id,
        )| Open {
            org_id,
            project_id,
            owner_id,
            agent_handle,
            options,
            message_id,
            answered,
            tg_chat_id,
        },
    ))
}

/// Stores the answer and posts it as the owner, replying to the question.
async fn record(pool: &PgPool, id: i64, q: &Open, answer: &str) -> Result<i64> {
    sqlx::query(
        "update questions set answer = $2, answered_at = now() where id = $1 and answer is null",
    )
    .bind(id)
    .bind(answer)
    .execute(pool)
    .await?;
    let mid = db::insert_message(
        pool,
        &NewMessage {
            org_id: q.org_id,
            project_id: q.project_id,
            author_id: Some(q.owner_id),
            author_name: None,
            kind: "chat",
            topic: Some("decisions".into()),
            body: format!("Answer to question #{id}: {answer}"),
            reply_to: q.message_id,
            mentions: vec![q.agent_handle.clone()],
            source: "api",
            tg_message_id: None,
            tg_thread_id: None,
            created_at: None,
        },
    )
    .await?
    .ok_or_else(|| anyhow::anyhow!("insert returned no id"))?;
    Ok(mid)
}

/// The owner answers from the API (no Telegram, or a free-text answer).
pub async fn answer(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<i64>,
    Json(req): Json<AnswerRequest>,
) -> AppResult<Json<AnswerResponse>> {
    let not_found = || AppError::NotFound(format!("question #{id} not found"));
    let q = load(&state.pool, id).await?.ok_or_else(not_found)?;
    if q.org_id != caller.org_id || q.owner_id != caller.member_id {
        return Err(not_found());
    }
    if q.answered {
        return Err(AppError::BadRequest(format!(
            "question #{id} is already answered"
        )));
    }
    let text = redact::redact(req.answer.trim()).0;
    if text.is_empty() {
        return Err(AppError::BadRequest("answer is empty".into()));
    }
    let mid = record(&state.pool, id, &q, &text).await?;
    if let (Some(tg), Some(chat)) = (&state.telegram, q.tg_chat_id) {
        let reply = match q.message_id {
            Some(m) => db::reply_target(&state.pool, q.project_id, m).await?,
            None => None,
        };
        let sent = tg
            .send(
                chat,
                &format!("✅ {}: {text}", caller.handle),
                reply.as_ref().and_then(|r| r.tg_message_id),
                None,
            )
            .await;
        if let Ok(tg_id) = sent {
            db::set_tg_ids(&state.pool, mid, tg_id, None).await?;
        }
    }
    Ok(Json(AnswerResponse {
        id,
        message_id: mid,
    }))
}

/// A button press in Telegram. Only the agent's owner may answer.
pub async fn on_button(
    pool: &PgPool,
    tg: &Telegram,
    cb: &crate::telegram::CallbackQuery,
) -> Result<()> {
    let Some((qid, idx)) = cb.data.as_deref().and_then(parse_callback) else {
        return tg.answer_callback(&cb.id, "").await;
    };
    let Some(q) = load(pool, qid).await? else {
        return tg
            .answer_callback(&cb.id, "This question no longer exists")
            .await;
    };
    let owner_tg: Option<i64> = sqlx::query_scalar("select tg_user_id from members where id = $1")
        .bind(q.owner_id)
        .fetch_one(pool)
        .await?;
    if owner_tg != Some(cb.from.id) {
        return tg
            .answer_callback(&cb.id, "Only this agent's owner can answer")
            .await;
    }
    if q.answered {
        return tg.answer_callback(&cb.id, "Already answered").await;
    }
    let Some(choice) = q.options.get(idx).cloned() else {
        return tg.answer_callback(&cb.id, "Unknown option").await;
    };
    let mid = record(pool, qid, &q, &choice).await?;
    tg.answer_callback(&cb.id, &format!("Saved: {choice}"))
        .await?;
    if let Some(m) = &cb.message {
        tg.clear_buttons(m.chat.id, m.message_id).await.ok();
        if let Ok(tg_id) = tg
            .send(
                m.chat.id,
                &format!("✅ {choice}"),
                Some(m.message_id),
                m.message_thread_id,
            )
            .await
        {
            db::set_tg_ids(pool, mid, tg_id, m.message_thread_id).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_roundtrip() {
        assert_eq!(parse_callback(&callback_data(42, 3)), Some((42, 3)));
        assert_eq!(parse_callback("x:1:2"), None);
        assert_eq!(parse_callback("q:abc:1"), None);
        assert!(
            callback_data(i64::MAX, 5).len() <= 64,
            "Telegram limits callback_data to 64 bytes"
        );
    }
}
