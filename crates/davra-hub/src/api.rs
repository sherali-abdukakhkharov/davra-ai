//! HTTP API, versioned under `/api/v1`.

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderValue};
use axum::routing::{get, post};
use axum::{Json, Router};
use davra_core::api::*;
use davra_core::{mentions, redact, MAX_POST_CHARS, PROTOCOL_VERSION};
use serde::Deserialize;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::db::{self, Caller, NewMessage, Project};
use crate::error::{AppError, AppResult};
use crate::AppState;

/// An agent may not post when this many consecutive messages are already
/// from agents: a human has to step in first.
pub const LOOP_GUARD: i64 = 4;

pub fn router(state: AppState) -> Router {
    let web_dir = state.web_dir.clone();
    let api = Router::new()
        .route("/api/v1/auth/register", post(crate::accounts::register))
        .route("/api/v1/auth/login", post(crate::accounts::login))
        .route("/api/v1/auth/logout", post(crate::accounts::logout))
        .route("/api/v1/auth/session", get(crate::accounts::session))
        .route("/healthz", get(|| async { "ok" }))
        .route("/.well-known/davra.json", get(well_known))
        .route("/api/v1/me", get(me))
        .route("/api/v1/projects/{slug}/brief", get(brief))
        .route("/api/v1/projects/{slug}/inbox", get(inbox))
        .route("/api/v1/projects/{slug}/ack", post(ack))
        .route("/api/v1/projects/{slug}/messages", post(post_message))
        .route("/api/v1/projects/{slug}/search", get(search))
        .route("/api/v1/messages/{id}", get(get_message))
        .route(
            "/api/v1/projects/{slug}/handoffs",
            get(crate::handoffs::list).post(crate::handoffs::create),
        )
        .route(
            "/api/v1/projects/{slug}/handoffs/sync",
            post(crate::handoffs::sync),
        )
        .route("/api/v1/handoffs/{id}", get(crate::handoffs::get_one))
        .route("/api/v1/handoffs/{id}/done", post(crate::handoffs::done))
        .route(
            "/api/v1/projects/{slug}/questions",
            post(crate::questions::ask),
        )
        .route(
            "/api/v1/questions/{id}/answer",
            post(crate::questions::answer),
        )
        .route("/api/{*rest}", axum::routing::any(api_not_found))
        .layer(DefaultBodyLimit::max(64 * 1024))
        // Push payloads can be large; this route has its own limit.
        .route(
            "/webhooks/github/{org}/{slug}",
            post(crate::github::webhook).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        );

    let app = match web_dir {
        // The web app: static files with SPA fallback to index.html.
        Some(dir) => {
            let index = dir.join("index.html");
            let web = ServeDir::new(&dir).fallback(ServeFile::new(index));
            api.fallback_service(web)
        }
        None => api,
    };
    app.layer(SetResponseHeaderLayer::if_not_present(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
             font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
        ),
    ))
    .layer(SetResponseHeaderLayer::if_not_present(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    ))
    .layer(SetResponseHeaderLayer::if_not_present(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    ))
    .layer(TraceLayer::new_for_http())
    .with_state(state)
}

async fn api_not_found() -> AppError {
    AppError::NotFound("no such API endpoint".into())
}

async fn well_known(State(state): State<AppState>) -> Json<WellKnown> {
    let mut features = vec![
        "brief".into(),
        "inbox".into(),
        "search:lexical".into(),
        "handoffs".into(),
    ];
    if state.embedder.is_some() {
        features.push("search:semantic".into());
    }
    if state.telegram.is_some() {
        features.push("chat:telegram".into());
    }
    Json(WellKnown {
        name: "davra".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        protocol: PROTOCOL_VERSION,
        auth: vec!["token".into()],
        features,
    })
}

pub async fn project(state: &AppState, caller: &Caller, slug: &str) -> AppResult<Project> {
    db::project_for(&state.pool, caller, slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("project '{slug}' not found or not granted")))
}

async fn me(State(state): State<AppState>, caller: Caller) -> AppResult<Json<Me>> {
    Ok(Json(Me {
        org: caller.org_slug.clone(),
        member: caller.member_ref(),
        projects: db::projects_of(&state.pool, &caller).await?,
    }))
}

async fn brief(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
) -> AppResult<Json<Brief>> {
    let p = project(&state, &caller, &slug).await?;
    let reader = caller.reader();
    let cursor = db::cursor(&state.pool, caller.member_id, p.id).await?;
    let (inbox, _) = db::inbox(&state.pool, &caller, &p, cursor, 10).await?;
    let unread = db::unread_count(&state.pool, &caller, &p, cursor).await?;
    let recent = db::recent(&state.pool, p.id, 15).await?;
    Ok(Json(Brief {
        project: p.as_ref(),
        me: caller.member_ref(),
        members: db::members_of_project(&state.pool, p.id).await?,
        inbox: inbox.into_iter().map(|m| m.view(&reader)).collect(),
        unread,
        recent: recent.into_iter().map(|m| m.view(&reader)).collect(),
        total_messages: db::total_messages(&state.pool, p.id).await?,
        handoffs: crate::handoffs::open_for(&state.pool, &caller, &p, 10).await?,
    }))
}

#[derive(Deserialize)]
struct InboxQuery {
    limit: Option<i64>,
}

async fn inbox(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Query(q): Query<InboxQuery>,
) -> AppResult<Json<Inbox>> {
    let p = project(&state, &caller, &slug).await?;
    let cursor = db::cursor(&state.pool, caller.member_id, p.id).await?;
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let (rows, more) = db::inbox(&state.pool, &caller, &p, cursor, limit).await?;
    let reader = caller.reader();
    Ok(Json(Inbox {
        project: p.slug,
        messages: rows.into_iter().map(|m| m.view(&reader)).collect(),
        cursor,
        more,
    }))
}

async fn ack(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(req): Json<AckRequest>,
) -> AppResult<Json<AckResponse>> {
    let p = project(&state, &caller, &slug).await?;
    let cursor = db::ack(&state.pool, caller.member_id, p.id, req.up_to).await?;
    Ok(Json(AckResponse { cursor }))
}

async fn post_message(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(req): Json<PostRequest>,
) -> AppResult<Json<PostResponse>> {
    let p = project(&state, &caller, &slug).await?;
    if p.role == "viewer" {
        return Err(AppError::Forbidden("viewers cannot post".into()));
    }
    let text = req.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("text is empty".into()));
    }
    if text.chars().count() > MAX_POST_CHARS {
        return Err(AppError::BadRequest(format!(
            "text is longer than {MAX_POST_CHARS} characters"
        )));
    }
    if caller.kind == MemberKind::Agent
        && db::last_n_by_agents(&state.pool, p.id, LOOP_GUARD).await?
    {
        return Err(AppError::TooMany(format!(
            "the last {LOOP_GUARD} messages are all from agents; wait for a human to reply"
        )));
    }

    let reply = match req.reply_to {
        Some(id) => Some(
            db::reply_target(&state.pool, p.id, id)
                .await?
                .ok_or_else(|| {
                    AppError::BadRequest(format!("reply_to #{id} is not in this project"))
                })?,
        ),
        None => None,
    };

    let (body, redacted) = redact::redact(text);
    let mut addressed = mentions::parse(&body);
    for h in req.to.iter().filter_map(|h| mentions::normalize_handle(h)) {
        if !addressed.contains(&h) {
            addressed.push(h);
        }
    }
    if let Some(author) = reply.as_ref().and_then(|r| r.author_handle.clone()) {
        if author != caller.handle && !addressed.contains(&author) {
            addressed.push(author);
        }
    }

    let id = db::insert_message(
        &state.pool,
        &NewMessage {
            org_id: p.org_id,
            project_id: p.id,
            author_id: Some(caller.member_id),
            author_name: None,
            kind: "chat",
            topic: req.topic.clone(),
            body: body.clone(),
            reply_to: reply.as_ref().map(|r| r.id),
            mentions: addressed,
            source: "api",
            tg_message_id: None,
            tg_thread_id: None,
            created_at: None,
        },
    )
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("insert returned no id")))?;

    // The author has obviously seen everything up to their own message.
    db::ack(&state.pool, caller.member_id, p.id, id).await?;

    let text = format!("🤖 {}\n{}", caller.handle, body);
    let relayed = relay(&state, p.tg_chat_id, id, &text, reply.as_ref()).await?;

    Ok(Json(PostResponse {
        id,
        relayed,
        redacted,
    }))
}

/// Shows a stored message in the project's chat, if one is linked.
pub async fn relay(
    state: &AppState,
    chat_id: Option<i64>,
    message_id: i64,
    text: &str,
    reply: Option<&db::ReplyTarget>,
) -> AppResult<bool> {
    let (Some(tg), Some(chat_id)) = (&state.telegram, chat_id) else {
        return Ok(false);
    };
    let reply_tg = reply.and_then(|r| r.tg_message_id);
    let thread = reply.and_then(|r| r.tg_thread_id);
    match tg.send(chat_id, text, reply_tg, thread).await {
        Ok(tg_id) => {
            db::set_tg_ids(&state.pool, message_id, tg_id, thread).await?;
            Ok(true)
        }
        Err(e) => {
            tracing::warn!(error = %e, message = message_id, "telegram relay failed");
            Ok(false)
        }
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    k: Option<i64>,
}

async fn search(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<SearchResults>> {
    let p = project(&state, &caller, &slug).await?;
    let query = q.q.trim();
    if query.is_empty() {
        return Err(AppError::BadRequest("q is empty".into()));
    }
    if query.chars().count() > 300 {
        return Err(AppError::BadRequest("q is too long".into()));
    }
    let k = q.k.unwrap_or(5).clamp(1, 20);
    let reader = caller.reader();
    // Semantic neighbours join the lexical results when an embedder is set up;
    // if it is slow or down, search quietly stays lexical.
    let qvec = match &state.embedder {
        Some(e) => match e.embed_one(query).await {
            Ok(v) => Some(v),
            Err(err) => {
                tracing::warn!(error = %err, "query embedding failed; lexical search only");
                None
            }
        },
        None => None,
    };
    let hits = db::search(&state.pool, p.id, query, k, qvec.as_deref())
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
    db::audit(
        &state.pool,
        Some(caller.org_id),
        Some(caller.member_id),
        "search",
        Some(&p.slug),
        serde_json::json!({ "chars": query.chars().count() }),
    )
    .await?;
    Ok(Json(SearchResults {
        project: p.slug,
        query: query.to_string(),
        hits,
    }))
}

#[derive(Deserialize)]
struct ContextQuery {
    context: Option<i64>,
}

async fn get_message(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<i64>,
    Query(q): Query<ContextQuery>,
) -> AppResult<Json<MessageContext>> {
    let not_found = || AppError::NotFound(format!("message #{id} not found"));
    let (org_id, slug) = db::message_project(&state.pool, id)
        .await?
        .ok_or_else(not_found)?;
    if org_id != caller.org_id {
        return Err(not_found());
    }
    // Same answer for "does not exist" and "not yours", so ids leak nothing.
    let p = db::project_for(&state.pool, &caller, &slug)
        .await?
        .ok_or_else(not_found)?;
    let n = q.context.unwrap_or(3).clamp(0, 10);
    let reader = caller.reader();
    let message = db::message_by_id(&state.pool, id)
        .await?
        .ok_or_else(not_found)?;
    let (before, after) = db::around(&state.pool, p.id, id, n).await?;
    Ok(Json(MessageContext {
        project: p.slug,
        before: before.into_iter().map(|m| m.view(&reader)).collect(),
        message: message.view(&reader),
        after: after.into_iter().map(|m| m.view(&reader)).collect(),
    }))
}
