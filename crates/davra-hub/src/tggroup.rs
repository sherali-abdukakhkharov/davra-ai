//! A project's owners link its Telegram group themselves, and choose the
//! forum topic the bot posts to.
//!
//! Linking: the web app hands out a one-time code; `t.me/<bot>?startgroup=<code>`
//! lets the person pick a group, adds the bot there and sends `/start <code>`.
//! Only the person who made the code can use it (their web account learns
//! their Telegram id if it didn't know it yet).
//!
//! Topics: the Bot API can't list a forum's topics, so the bot remembers the
//! ones it sees (created, renamed, or written in). `/here` in a topic, or the
//! web app, makes it the project's topic. The person who linked the group
//! and the project's owners choose it.

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::accounts::{require_csrf, SessionUser};
use crate::db::{self, Caller, Project};
use crate::error::{AppError, AppResult};
use crate::telegram::{ChatMemberUpdated, Telegram, TgMessage};
use crate::tgcard::Lang;
use crate::AppState;

const CODE_TTL_SECS: i64 = 900;
const MAX_TOPIC_NAME: usize = 128;

fn say(lang: Lang, en: &str, uz: &str) -> String {
    match lang {
        Lang::En => en.to_string(),
        Lang::Uz => uz.to_string(),
    }
}

// --- web API -----------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct Topic {
    id: i64,
    name: Option<String>,
}

#[derive(Serialize)]
pub struct Status {
    bot: Option<String>,
    linked: bool,
    chat_title: Option<String>,
    forum: bool,
    topic: Option<Topic>,
    topics: Vec<Topic>,
    can_link: bool,
    can_manage: bool,
    reads_all: Option<bool>,
}

#[derive(Serialize)]
pub struct LinkResponse {
    deep_link: String,
    bot: String,
    command: String,
    expires_in: i64,
}

#[derive(Deserialize)]
pub struct TopicRequest {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    create: Option<String>,
}

struct Linked {
    chat: Option<i64>,
    title: Option<String>,
    forum: bool,
    thread: Option<i64>,
    linked_by: Option<Uuid>,
}

async fn linked(pool: &PgPool, project: Uuid) -> Result<Linked> {
    let (chat, title, forum, thread, linked_by): (Option<i64>, Option<String>, bool, Option<i64>, Option<Uuid>) =
        sqlx::query_as(
            "select tg_chat_id, tg_chat_title, tg_forum, tg_thread_id, tg_linked_by from projects where id = $1",
        )
        .bind(project)
        .fetch_one(pool)
        .await?;
    Ok(Linked {
        chat,
        title,
        forum,
        thread,
        linked_by,
    })
}

fn may_manage(caller: &Caller, p: &Project, l: &Linked) -> bool {
    p.role == "owner" || l.linked_by == Some(caller.member_id)
}

async fn status_of(state: &AppState, caller: &Caller, p: &Project) -> AppResult<Status> {
    let l = linked(&state.pool, p.id).await?;
    let topics: Vec<Topic> = match l.chat {
        Some(chat) => sqlx::query_as::<_, (i64, Option<String>)>(
            "select thread_id, name from tg_topics where chat_id = $1 order by name nulls last, thread_id",
        )
        .bind(chat)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .map(|(id, name)| Topic { id, name })
        .collect(),
        None => Vec::new(),
    };
    let topic = l.thread.map(|id| Topic {
        id,
        name: topics
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.name.clone()),
    });
    let reads_all = match (&state.telegram, l.chat) {
        (Some(tg), Some(chat)) => tg.reads_all(chat).await.ok(),
        _ => None,
    };
    Ok(Status {
        bot: state.telegram.as_ref().map(|t| t.username.clone()),
        linked: l.chat.is_some(),
        chat_title: l.title.clone(),
        forum: l.forum,
        topic,
        topics,
        can_link: crate::team::is_admin(&p.role),
        can_manage: l.chat.is_some() && may_manage(caller, p, &l),
        reads_all,
    })
}

pub async fn status(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
) -> AppResult<Json<Status>> {
    let (caller, p) = crate::webapp::open_project(&state, user.id, &org, &slug).await?;
    Ok(Json(status_of(&state, &caller, &p).await?))
}

/// A one-time code (and deep link) that links a group to this project.
pub async fn link(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    headers: HeaderMap,
    Path((org, slug)): Path<(String, String)>,
) -> AppResult<Json<LinkResponse>> {
    require_csrf(&headers)?;
    let tg = state
        .telegram
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("this hub has no Telegram bot configured".into()))?;
    let (caller, p) = crate::team::admin_project(&state, user.id, &org, &slug).await?;
    let mut b = [0u8; 12];
    rand::rng().fill_bytes(&mut b);
    let code = format!("g_{}", hex::encode(b));
    sqlx::query(
        "delete from tg_group_links where project_id = $1 and member_id = $2 and used_at is null",
    )
    .bind(p.id)
    .bind(caller.member_id)
    .execute(&state.pool)
    .await?;
    sqlx::query(
        "insert into tg_group_links (code, org_id, project_id, member_id, user_id, expires_at) \
         values ($1, $2, $3, $4, $5, now() + make_interval(secs => $6))",
    )
    .bind(&code)
    .bind(p.org_id)
    .bind(p.id)
    .bind(caller.member_id)
    .bind(user.id)
    .bind(CODE_TTL_SECS as f64)
    .execute(&state.pool)
    .await?;
    Ok(Json(LinkResponse {
        deep_link: format!("https://t.me/{}?startgroup={code}", tg.username),
        command: format!("/start@{} {code}", tg.username),
        bot: tg.username.clone(),
        expires_in: CODE_TTL_SECS,
    }))
}

async fn managed(
    state: &AppState,
    user: Uuid,
    org: &str,
    slug: &str,
) -> AppResult<(Caller, Project, i64)> {
    let (caller, p) = crate::webapp::open_project(state, user, org, slug).await?;
    let l = linked(&state.pool, p.id).await?;
    let Some(chat) = l.chat else {
        return Err(AppError::BadRequest(
            "no Telegram group is linked to this project".into(),
        ));
    };
    if !may_manage(&caller, &p, &l) {
        return Err(AppError::Forbidden(
            "only the person who linked the group, or a project owner, can change this".into(),
        ));
    }
    Ok((caller, p, chat))
}

pub async fn unlink(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    headers: HeaderMap,
    Path((org, slug)): Path<(String, String)>,
) -> AppResult<Json<serde_json::Value>> {
    require_csrf(&headers)?;
    let (caller, p, chat) = managed(&state, user.id, &org, &slug).await?;
    clear_link(&state.pool, p.id).await?;
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "project.unlink_telegram",
        Some(&p.id.to_string()),
        json!({ "chat": chat }),
    )
    .await?;
    if let Some(tg) = &state.telegram {
        tg.set_thread(chat, None);
        let text = say(
            tg.lang,
            &format!(
                "This group is no longer linked to the Davra project {}/{}.",
                caller.org_slug, p.slug
            ),
            &format!(
                "Bu guruh endi Davra'dagi {}/{} loyihasiga ulanmagan.",
                caller.org_slug, p.slug
            ),
        );
        tg.send(chat, &text, None, None).await.ok();
    }
    Ok(Json(json!({ "ok": true })))
}

async fn clear_link(pool: &PgPool, project: Uuid) -> Result<()> {
    sqlx::query(
        "update projects set tg_chat_id = null, tg_chat_title = null, tg_forum = false, \
         tg_thread_id = null, tg_linked_by = null where id = $1",
    )
    .bind(project)
    .execute(pool)
    .await?;
    Ok(())
}

/// Chooses the topic the bot posts to: a known one, General (`id: null`), or
/// a new one the bot creates (`create`).
pub async fn set_topic(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    headers: HeaderMap,
    Path((org, slug)): Path<(String, String)>,
    Json(b): Json<TopicRequest>,
) -> AppResult<Json<Status>> {
    require_csrf(&headers)?;
    let (caller, p, chat) = managed(&state, user.id, &org, &slug).await?;
    let tg = state
        .telegram
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("this hub has no Telegram bot configured".into()))?;
    let thread = match (b.create, b.id) {
        (Some(name), _) => {
            let name = name.trim().to_string();
            if name.is_empty() || name.chars().count() > MAX_TOPIC_NAME {
                return Err(AppError::Field(
                    "create",
                    format!("topic name must be 1 to {MAX_TOPIC_NAME} characters"),
                ));
            }
            let id = tg.create_topic(chat, &name).await.map_err(|e| {
                AppError::BadRequest(format!(
                    "Telegram did not create the topic ({e}). The group must have topics turned on, \
                     and the bot must be an admin that can manage topics."
                ))
            })?;
            remember_topic(&state.pool, chat, id, Some(&name)).await?;
            Some(id)
        }
        (None, Some(id)) => {
            let known: Option<i64> = sqlx::query_scalar(
                "select thread_id from tg_topics where chat_id = $1 and thread_id = $2",
            )
            .bind(chat)
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
            if known.is_none() {
                return Err(AppError::BadRequest(format!(
                    "topic {id} is not known in this group"
                )));
            }
            Some(id)
        }
        (None, None) => None,
    };
    sqlx::query("update projects set tg_thread_id = $2 where id = $1")
        .bind(p.id)
        .bind(thread)
        .execute(&state.pool)
        .await?;
    tg.set_thread(chat, thread);
    db::audit(
        &state.pool,
        Some(p.org_id),
        Some(caller.member_id),
        "project.telegram_topic",
        Some(&p.id.to_string()),
        json!({ "chat": chat, "thread": thread }),
    )
    .await?;
    if thread.is_some() {
        let text = say(
            tg.lang,
            "Davra will post in this topic from now on.",
            "Davra endi şu topicga yozadi.",
        );
        tg.send(chat, &text, None, None).await.ok();
    }
    Ok(Json(status_of(&state, &caller, &p).await?))
}

// --- bot side ----------------------------------------------------------------

/// The code in "/start g_…" or "/link g_…" (optionally "/start@<bot>").
pub fn group_code<'a>(text: &'a str, bot: &str) -> Option<&'a str> {
    let mut words = text.split_whitespace();
    let cmd = words.next()?;
    let (name, to) = match cmd.split_once('@') {
        Some((n, b)) => (n, Some(b)),
        None => (cmd, None),
    };
    if !matches!(name, "/start" | "/link") || to.is_some_and(|b| !b.eq_ignore_ascii_case(bot)) {
        return None;
    }
    words
        .next()
        .filter(|c| c.starts_with("g_") && c.len() <= 64)
}

async fn remember_topic(pool: &PgPool, chat: i64, thread: i64, name: Option<&str>) -> Result<()> {
    sqlx::query(
        "insert into tg_topics (chat_id, thread_id, name) values ($1, $2, $3) \
         on conflict (chat_id, thread_id) do update set name = coalesce(excluded.name, tg_topics.name), seen_at = now()",
    )
    .bind(chat)
    .bind(thread)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every message from a group, before anything else: keeps the link, the
/// group's title and its topics current.
pub async fn observe(pool: &PgPool, tg: &Telegram, m: &TgMessage) -> Result<()> {
    let chat = m.chat.id;
    if let Some(new) = m.migrate_to_chat_id {
        // A group that becomes a supergroup gets a new id.
        let moved = sqlx::query("update projects set tg_chat_id = $2 where tg_chat_id = $1")
            .bind(chat)
            .bind(new)
            .execute(pool)
            .await?;
        if moved.rows_affected() > 0 {
            tracing::info!(
                from = chat,
                to = new,
                "linked Telegram group became a supergroup"
            );
            tg.load_threads(pool).await?;
        }
        return Ok(());
    }
    if !m.chat.is_group() {
        return Ok(());
    }
    let linked: bool =
        sqlx::query_scalar("select exists (select 1 from projects where tg_chat_id = $1)")
            .bind(chat)
            .fetch_one(pool)
            .await?;
    if !linked {
        return Ok(());
    }
    sqlx::query(
        "update projects set tg_chat_title = coalesce($2, tg_chat_title), tg_forum = $3 \
         where tg_chat_id = $1 and (tg_chat_title is distinct from coalesce($2, tg_chat_title) or tg_forum <> $3)",
    )
    .bind(chat)
    .bind(&m.chat.title)
    .bind(m.chat.is_forum)
    .execute(pool)
    .await?;
    if let Some(t) = &m.forum_topic_created {
        // The service message opens the topic: its id is the thread id.
        let thread = m.message_thread_id.unwrap_or(m.message_id);
        remember_topic(pool, chat, thread, Some(&t.name)).await?;
    } else if let (Some(e), Some(thread)) = (&m.forum_topic_edited, m.message_thread_id) {
        remember_topic(pool, chat, thread, e.name.as_deref()).await?;
    } else if let (true, Some(thread)) = (m.is_topic_message, m.message_thread_id) {
        let name = m
            .reply_to_message
            .as_ref()
            .and_then(|r| r.forum_topic_created.as_ref())
            .map(|t| t.name.as_str());
        remember_topic(pool, chat, thread, name).await?;
    }
    Ok(())
}

/// "/start g_<code>" in a group: link it to the code's project.
pub async fn on_code(pool: &PgPool, tg: &Telegram, m: &TgMessage, code: &str) -> Result<()> {
    let chat = m.chat.id;
    let reply = |text: String| async move {
        tg.send(chat, &text, Some(m.message_id), m.message_thread_id)
            .await
            .map(|_| ())
    };
    if !m.chat.is_group() {
        return reply(say(
            tg.lang,
            "Add me to the group from the link in Davra; this code works only in a group.",
            "Meni Davra'dagi havola orqali guruhga qóşing: bu kod faqat guruhda işlaydi.",
        ))
        .await;
    }
    let Some(from) = m.from.as_ref().filter(|f| !f.is_bot) else {
        return Ok(());
    };
    let row: Option<(Uuid, Uuid, Uuid, Uuid)> = sqlx::query_as(
        "select org_id, project_id, member_id, user_id from tg_group_links \
         where code = $1 and used_at is null and expires_at > now()",
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    let Some((org_id, project_id, member_id, user_id)) = row else {
        return reply(say(
            tg.lang,
            "This link has expired or was already used. Press “Link a Telegram group” in Davra again.",
            "Bu havola eskirgan yoki allaqaçon işlatilgan. Davra'da «Telegram guruhni ulaş»ni qayta bosing.",
        ))
        .await;
    };
    let known_tg: Option<i64> = sqlx::query_scalar("select tg_user_id from users where id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    match known_tg {
        Some(t) if t != from.id => {
            return reply(say(
                tg.lang,
                "Only the person who created this link in Davra can use it.",
                "Bu havoladan faqat uni Davra'da yaratgan odam foydalana oladi.",
            ))
            .await;
        }
        Some(_) => {}
        None => {
            if let Err(why) =
                crate::webapp::attach_telegram(pool, user_id, from.id, from.username.as_deref())
                    .await?
            {
                return reply(why).await;
            }
        }
    }
    let other: Option<String> = sqlx::query_scalar(
        "select o.slug || '/' || p.slug from projects p join orgs o on o.id = p.org_id \
         where p.tg_chat_id = $1 and p.id <> $2",
    )
    .bind(chat)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    if let Some(other) = other {
        return reply(say(
            tg.lang,
            &format!("This group is already linked to {other}. Unlink it there first."),
            &format!("Bu guruh allaqaçon {other} loyihasiga ulangan. Avval uni óşa yerda uzing."),
        ))
        .await;
    }
    let used: Option<String> =
        sqlx::query_scalar("update tg_group_links set used_at = now() where code = $1 and used_at is null returning code")
            .bind(code)
            .fetch_optional(pool)
            .await?;
    if used.is_none() {
        return Ok(());
    }
    let (old, org_slug, slug, name): (Option<i64>, String, String, Option<String>) = sqlx::query_as(
        "select p.tg_chat_id, o.slug, p.slug, p.name from projects p join orgs o on o.id = p.org_id where p.id = $1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "update projects set tg_chat_id = $2, tg_chat_title = $3, tg_forum = $4, tg_thread_id = null, \
         tg_linked_by = $5 where id = $1",
    )
    .bind(project_id)
    .bind(chat)
    .bind(&m.chat.title)
    .bind(m.chat.is_forum)
    .bind(member_id)
    .execute(pool)
    .await?;
    tg.set_thread(chat, None);
    db::audit(
        pool,
        Some(org_id),
        Some(member_id),
        "project.link_telegram",
        Some(&project_id.to_string()),
        json!({ "chat": chat, "via": "web" }),
    )
    .await?;

    let shown = name.unwrap_or_else(|| slug.clone());
    let mut text = say(
        tg.lang,
        &format!(
            "✅ This group is now linked to the Davra project {shown} ({org_slug}/{slug}). \
             Messages from the team and its agents appear here."
        ),
        &format!(
            "✅ Bu guruh endi Davra'dagi {shown} ({org_slug}/{slug}) loyihasiga ulandi. \
             Jamoa va agentlarning xabarlari şu yerda kórinadi."
        ),
    );
    if m.chat.is_forum {
        text.push_str("\n\n");
        text.push_str(&say(
            tg.lang,
            &format!("Choose the topic for Davra on the project page, or write /here@{} in that topic.", tg.username),
            &format!("Davra xabarlari qaysi topicga kelişini loyiha sahifasida tanlang yoki óşa topicda /here@{} yozing.", tg.username),
        ));
    }
    if !tg.reads_all(chat).await.unwrap_or(true) {
        text.push_str("\n\n");
        text.push_str(&say(
            tg.lang,
            "⚠️ Make me an admin of this group. Otherwise I see only commands, not people's messages.",
            "⚠️ Meni şu guruhda admin qiling. Aks holda faqat buyruqlarni kóraman, odamlarning xabarlarini kórmayman.",
        ));
    }
    reply(text).await?;
    if let Some(old) = old.filter(|o| *o != chat) {
        tg.set_thread(old, None);
        let note = say(
            tg.lang,
            &format!(
                "This group is no longer linked to {org_slug}/{slug}: it moved to another group."
            ),
            &format!(
                "Bu guruh endi {org_slug}/{slug} loyihasiga ulanmagan: loyiha boşqa guruhga kóçdi."
            ),
        );
        tg.send(old, &note, None, None).await.ok();
    }
    Ok(())
}

/// "/here" in a linked group: the topic it's written in (or General) becomes
/// the project's topic. The person who linked the group or an owner may.
pub async fn on_here(pool: &PgPool, tg: &Telegram, m: &TgMessage) -> Result<()> {
    let chat = m.chat.id;
    let thread_of_msg = m.message_thread_id.filter(|_| m.is_topic_message);
    let reply = |text: String| async move {
        tg.send(chat, &text, Some(m.message_id), thread_of_msg)
            .await
            .map(|_| ())
    };
    if !m.chat.is_group() {
        return Ok(());
    }
    let project: Option<(Uuid, Uuid, Option<Uuid>)> =
        sqlx::query_as("select id, org_id, tg_linked_by from projects where tg_chat_id = $1")
            .bind(chat)
            .fetch_optional(pool)
            .await?;
    let Some((project_id, org_id, linked_by)) = project else {
        return reply(say(
            tg.lang,
            "This group is not linked to a Davra project.",
            "Bu guruh hech qaysi Davra loyihasiga ulanmagan.",
        ))
        .await;
    };
    let Some(from) = m.from.as_ref() else {
        return Ok(());
    };
    let member: Option<(Uuid, String)> = sqlx::query_as(
        "select m.id, g.role from members m join grants g on g.member_id = m.id and g.project_id = $2 \
         where m.org_id = $1 and m.tg_user_id = $3 and m.kind = 'human'",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(from.id)
    .fetch_optional(pool)
    .await?;
    let allowed = member
        .as_ref()
        .is_some_and(|(id, role)| role == "owner" || Some(*id) == linked_by);
    if !allowed {
        return reply(say(
            tg.lang,
            "Only the person who linked this group, or a project owner, can choose the topic.",
            "Topicni faqat guruhni ulagan odam yoki loyiha owner'i tanlay oladi.",
        ))
        .await;
    }
    if let Some(th) = thread_of_msg {
        let name = m
            .reply_to_message
            .as_ref()
            .and_then(|r| r.forum_topic_created.as_ref())
            .map(|t| t.name.as_str());
        remember_topic(pool, chat, th, name).await?;
    }
    sqlx::query("update projects set tg_thread_id = $2 where id = $1")
        .bind(project_id)
        .bind(thread_of_msg)
        .execute(pool)
        .await?;
    tg.set_thread(chat, thread_of_msg);
    db::audit(
        pool,
        Some(org_id),
        member.map(|(id, _)| id),
        "project.telegram_topic",
        Some(&project_id.to_string()),
        json!({ "chat": chat, "thread": thread_of_msg, "via": "here" }),
    )
    .await?;
    reply(match thread_of_msg {
        Some(_) => say(
            tg.lang,
            "✅ Davra will post in this topic from now on.",
            "✅ Davra endi şu topicga yozadi.",
        ),
        None => say(
            tg.lang,
            "✅ Davra will post to the whole group (General) from now on.",
            "✅ Davra endi butun guruhga (General) yozadi.",
        ),
    })
    .await
}

/// The bot was removed from a group: that group is no longer a project's chat.
pub async fn on_membership(pool: &PgPool, tg: &Telegram, u: &ChatMemberUpdated) -> Result<()> {
    if !matches!(u.new_chat_member.status.as_str(), "left" | "kicked") {
        return Ok(());
    }
    let rows: Vec<(Uuid, Uuid)> =
        sqlx::query_as("select id, org_id from projects where tg_chat_id = $1")
            .bind(u.chat.id)
            .fetch_all(pool)
            .await?;
    for (project, org) in rows {
        clear_link(pool, project).await?;
        db::audit(
            pool,
            Some(org),
            None,
            "project.unlink_telegram",
            Some(&project.to_string()),
            json!({ "chat": u.chat.id, "via": "bot removed" }),
        )
        .await?;
        tracing::info!(
            chat = u.chat.id,
            "bot removed from a linked group; unlinked"
        );
    }
    tg.set_thread(u.chat.id, None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_codes() {
        assert_eq!(group_code("/start g_abc", "davra_ai_bot"), Some("g_abc"));
        assert_eq!(
            group_code("/start@davra_ai_bot g_abc", "davra_ai_bot"),
            Some("g_abc")
        );
        assert_eq!(
            group_code("/link@Davra_AI_Bot g_abc", "davra_ai_bot"),
            Some("g_abc")
        );
        assert_eq!(group_code("/start@other_bot g_abc", "davra_ai_bot"), None);
        assert_eq!(group_code("/start link_abc", "davra_ai_bot"), None);
        assert_eq!(group_code("/start", "davra_ai_bot"), None);
        assert_eq!(group_code("hello g_abc", "davra_ai_bot"), None);
    }
}
