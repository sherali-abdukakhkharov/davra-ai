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
    let downloads_dir = state.downloads_dir.clone();
    let api = Router::new()
        .route("/api/v1/auth/register", post(crate::accounts::register))
        .route("/api/v1/auth/login", post(crate::accounts::login))
        .route("/api/v1/auth/logout", post(crate::accounts::logout))
        .route("/api/v1/auth/session", get(crate::accounts::session))
        .route("/healthz", get(|| async { "ok" }))
        .route("/install.ps1", get(crate::downloads::install_ps1))
        .route("/install.sh", get(crate::downloads::install_sh))
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
        .route("/api/v1/web/overview", get(crate::webapp::overview))
        .route(
            "/api/v1/web/telegram/link",
            post(crate::webapp::telegram_link),
        )
        .route("/api/v1/web/projects", post(crate::webapp::create_project))
        .route(
            "/api/v1/web/projects/{org}/{slug}",
            get(crate::webapp::project_detail),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/search",
            get(crate::webapp::search),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/messages",
            get(crate::webapp::older_messages).post(crate::webapp::post),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/agents",
            post(crate::webapp::create_agent),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/agents/{handle}/roles",
            post(crate::webapp::set_roles),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/agents/{handle}/tokens",
            post(crate::webapp::create_token),
        )
        .route(
            "/api/v1/web/tokens/{id}/revoke",
            post(crate::webapp::revoke_token),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/invites",
            get(crate::team::list_invites).post(crate::team::create_invite),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/invites/{id}/revoke",
            post(crate::team::revoke_invite),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/members/{handle}/role",
            post(crate::team::set_member_role),
        )
        .route(
            "/api/v1/web/projects/{org}/{slug}/members/{handle}/remove",
            post(crate::team::remove_member),
        )
        .route("/api/v1/web/invites/{code}", get(crate::team::invite_info))
        .route(
            "/api/v1/web/invites/{code}/accept",
            post(crate::team::accept_invite),
        )
        .route("/api/{*rest}", axum::routing::any(api_not_found))
        .layer(DefaultBodyLimit::max(64 * 1024))
        // A repo sync carries many handoff files; it has its own limit.
        .route(
            "/api/v1/projects/{slug}/handoffs/sync",
            post(crate::handoffs::sync).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        // Push payloads can be large; this route has its own limit.
        .route(
            "/webhooks/github/{org}/{slug}",
            post(crate::github::webhook).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        );

    let api = match downloads_dir {
        Some(dir) => api
            .route_service(
                "/downloads/SHA256SUMS",
                tower_http::set_header::SetResponseHeader::overriding(
                    ServeFile::new(dir.join("SHA256SUMS")),
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                ),
            )
            .nest_service("/downloads", ServeDir::new(dir)),
        None => api,
    };
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
    project_in(&state.pool, caller, slug).await
}

/// [`project`] on a given connection, e.g. an org-scoped transaction.
pub async fn project_in(
    db: impl sqlx::PgExecutor<'_>,
    caller: &Caller,
    slug: &str,
) -> AppResult<Project> {
    db::project_for(db, caller, slug)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("project '{slug}' not found or not granted")))
}

/// A transaction that only sees `caller`'s org: Postgres row-level security
/// backs up the org checks in the queries.
pub async fn scoped(
    state: &AppState,
    caller: &Caller,
) -> AppResult<sqlx::Transaction<'static, sqlx::Postgres>> {
    Ok(db::scoped(&state.pool, state.rls, caller.org_id).await?)
}

async fn me(State(state): State<AppState>, caller: Caller) -> AppResult<Json<Me>> {
    let mut tx = scoped(&state, &caller).await?;
    let projects = db::projects_of(&mut *tx, &caller).await?;
    tx.commit().await?;
    Ok(Json(Me {
        org: caller.org_slug.clone(),
        member: caller.member_ref(),
        projects,
    }))
}

async fn brief(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
) -> AppResult<Json<Brief>> {
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    let reader = caller.reader();
    let cursor = db::cursor(&mut *tx, caller.member_id, p.id).await?;
    let (inbox, _) = db::inbox(&mut *tx, &caller, &p, cursor, 10).await?;
    let unread = db::unread_count(&mut *tx, &caller, &p, cursor).await?;
    let recent = db::recent(&mut *tx, p.id, 15).await?;
    let members = db::members_of_project(&mut *tx, p.id).await?;
    let total_messages = db::total_messages(&mut *tx, p.id).await?;
    let handoffs = crate::handoffs::open_for(&mut *tx, &caller, &p, 10).await?;
    tx.commit().await?;
    Ok(Json(Brief {
        project: p.as_ref(),
        me: caller.member_ref(),
        members,
        inbox: inbox.into_iter().map(|m| m.view(&reader)).collect(),
        unread,
        recent: recent.into_iter().map(|m| m.view(&reader)).collect(),
        total_messages,
        handoffs,
    }))
}

#[derive(Deserialize)]
struct InboxQuery {
    limit: Option<i64>,
    /// Seconds to hold the request open while the inbox is empty (long-poll).
    wait: Option<u64>,
}

async fn inbox(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Query(q): Query<InboxQuery>,
) -> AppResult<Json<Inbox>> {
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    tx.commit().await?;
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let wait = std::time::Duration::from_secs(q.wait.unwrap_or(0)).min(crate::live::MAX_WAIT);
    let deadline = tokio::time::Instant::now() + wait;
    // Subscribe before the first query so a message stored in between still wakes us.
    let mut rx = state.events.subscribe();
    let (cursor, rows, more) = loop {
        // A fresh short transaction per check: no connection is held while waiting.
        let mut tx = scoped(&state, &caller).await?;
        let cursor = db::cursor(&mut *tx, caller.member_id, p.id).await?;
        let (rows, more) = db::inbox(&mut *tx, &caller, &p, cursor, limit).await?;
        tx.commit().await?;
        if !rows.is_empty() || !crate::live::wait_for(&mut rx, p.id, deadline).await {
            break (cursor, rows, more);
        }
    };
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
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
    let cursor = db::ack(&mut tx, caller.member_id, p.id, req.up_to).await?;
    tx.commit().await?;
    Ok(Json(AckResponse { cursor }))
}

async fn post_message(
    State(state): State<AppState>,
    caller: Caller,
    Path(slug): Path<String>,
    Json(req): Json<PostRequest>,
) -> AppResult<Json<PostResponse>> {
    let p = project(&state, &caller, &slug).await?;
    Ok(Json(post_as(&state, &caller, &p, req).await?))
}

/// Stores a message from `caller` in project `p` and relays it to the chat.
pub async fn post_as(
    state: &AppState,
    caller: &Caller,
    p: &Project,
    req: PostRequest,
) -> AppResult<PostResponse> {
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
    let mut tx = scoped(state, caller).await?;
    if caller.kind == MemberKind::Agent && db::last_n_by_agents(&mut *tx, p.id, LOOP_GUARD).await? {
        return Err(AppError::TooMany(format!(
            "the last {LOOP_GUARD} messages are all from agents; wait for a human to reply"
        )));
    }

    let reply = match req.reply_to {
        Some(id) => Some(db::reply_target(&mut *tx, p.id, id).await?.ok_or_else(|| {
            AppError::BadRequest(format!("reply_to #{id} is not in this project"))
        })?),
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
        &mut *tx,
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
    db::ack(&mut tx, caller.member_id, p.id, id).await?;
    tx.commit().await?;

    let icon = if caller.kind == MemberKind::Agent {
        "🤖"
    } else {
        "💬"
    };
    let text = format!("{icon} {}\n{}", caller.handle, body);
    let relayed = relay(state, p.tg_chat_id, id, &text, reply.as_ref()).await?;

    Ok(PostResponse {
        id,
        relayed,
        redacted,
    })
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
    // @handles become real Telegram mentions so people get notified.
    let org: Option<uuid::Uuid> = sqlx::query_scalar("select org_id from messages where id = $1")
        .bind(message_id)
        .fetch_optional(&state.pool)
        .await?;
    let (text, entities) = match org {
        Some(org) => {
            let people = crate::telegram::mentioned_people(&state.pool, org, text).await?;
            crate::telegram::render_mentions(text, &people)
        }
        None => (text.to_string(), vec![]),
    };
    match tg
        .send_rich(chat_id, &text, &entities, reply_tg, thread)
        .await
    {
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
    let mut tx = scoped(&state, &caller).await?;
    let p = project_in(&mut *tx, &caller, &slug).await?;
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
    let hits = db::search(&mut tx, p.id, query, k, qvec.as_deref())
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
        &mut *tx,
        Some(caller.org_id),
        Some(caller.member_id),
        "search",
        Some(&p.slug),
        serde_json::json!({ "chars": query.chars().count() }),
    )
    .await?;
    tx.commit().await?;
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
    let mut tx = scoped(&state, &caller).await?;
    let (org_id, slug) = db::message_project(&mut *tx, id)
        .await?
        .ok_or_else(not_found)?;
    if org_id != caller.org_id {
        return Err(not_found());
    }
    // Same answer for "does not exist" and "not yours", so ids leak nothing.
    let p = db::project_for(&mut *tx, &caller, &slug)
        .await?
        .ok_or_else(not_found)?;
    let n = q.context.unwrap_or(3).clamp(0, 10);
    let reader = caller.reader();
    let message = db::message_by_id(&mut *tx, id)
        .await?
        .ok_or_else(not_found)?;
    let (before, after) = db::around(&mut tx, p.id, id, n).await?;
    tx.commit().await?;
    Ok(Json(MessageContext {
        project: p.slug,
        before: before.into_iter().map(|m| m.view(&reader)).collect(),
        message: message.view(&reader),
        after: after.into_iter().map(|m| m.view(&reader)).collect(),
    }))
}
