//! Minimal Telegram Bot API client: long polling in, plain-text messages out.
//!
//! Telegram is only the human window into the room. The Bot API cannot read
//! history, so every message is stored the moment it arrives.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use davra_core::{mentions, redact};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

use crate::db::{self, NewMessage};

const OFFSET_KEY: &str = "telegram.offset";
const MAX_TG_CHARS: usize = 4096;

pub struct Telegram {
    http: reqwest::Client,
    base: String,
    pub bot_id: i64,
    pub username: String,
    /// Language of card labels (`DAVRA_LANG`).
    pub lang: crate::tgcard::Lang,
}

#[derive(Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<TgMessage>,
    pub edited_message: Option<TgMessage>,
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CallbackQuery {
    pub id: String,
    pub from: TgUser,
    pub message: Option<TgMessage>,
    pub data: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TgMessage {
    pub message_id: i64,
    pub message_thread_id: Option<i64>,
    pub from: Option<TgUser>,
    pub chat: TgChat,
    pub date: i64,
    pub text: Option<String>,
    pub caption: Option<String>,
    pub reply_to_message: Option<Box<TgMessage>>,
    #[serde(default)]
    pub is_topic_message: bool,
    pub forum_topic_created: Option<ForumTopic>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TgUser {
    pub id: i64,
    #[serde(default)]
    pub is_bot: bool,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TgChat {
    pub id: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ForumTopic {
    pub name: String,
}

#[derive(Deserialize)]
struct SentMessage {
    message_id: i64,
}

#[derive(Deserialize)]
struct BotUser {
    id: i64,
    username: Option<String>,
}

/// A text message from a group, flattened for storage.
#[derive(Debug, PartialEq)]
pub struct Incoming {
    pub chat_id: i64,
    pub tg_message_id: i64,
    pub thread_id: Option<i64>,
    pub from_id: Option<i64>,
    pub from_is_bot: bool,
    pub from_name: String,
    pub from_username: Option<String>,
    pub text: String,
    pub reply_to_tg: Option<i64>,
    pub topic: Option<String>,
    pub date: DateTime<Utc>,
    pub edited: bool,
}

/// Pure mapping from a Telegram update to what we store. Non-text messages
/// (stickers, joins, ...) are skipped.
pub fn parse_update(u: &Update) -> Option<Incoming> {
    let (m, edited) = match (&u.message, &u.edited_message) {
        (Some(m), _) => (m, false),
        (None, Some(m)) => (m, true),
        _ => return None,
    };
    let text = m.text.clone().or_else(|| m.caption.clone())?;
    if text.trim().is_empty() {
        return None;
    }
    // In forum groups every topic message "replies" to the topic's creation
    // message; that is a topic, not a real reply.
    let (reply_to_tg, topic) = match &m.reply_to_message {
        Some(r) if m.is_topic_message && Some(r.message_id) == m.message_thread_id => {
            (None, r.forum_topic_created.as_ref().map(|t| t.name.clone()))
        }
        Some(r) => (Some(r.message_id), None),
        None => (None, None),
    };
    let from_name = m
        .from
        .as_ref()
        .map(|f| match (&f.username, &f.last_name) {
            (Some(u), _) => u.clone(),
            (None, Some(l)) => format!("{} {}", f.first_name, l),
            (None, None) => f.first_name.clone(),
        })
        .unwrap_or_else(|| "unknown".into());
    Some(Incoming {
        chat_id: m.chat.id,
        tg_message_id: m.message_id,
        thread_id: m.message_thread_id,
        from_id: m.from.as_ref().map(|f| f.id),
        from_is_bot: m.from.as_ref().is_some_and(|f| f.is_bot),
        from_name,
        from_username: m.from.as_ref().and_then(|f| f.username.clone()),
        text,
        reply_to_tg,
        topic,
        date: DateTime::from_timestamp(m.date, 0).unwrap_or_else(Utc::now),
        edited,
    })
}

/// A human member as Telegram knows them.
pub struct TgPerson {
    pub user_id: i64,
    pub username: Option<String>,
    pub name: String,
}

/// Members of the org mentioned in `text` that have a Telegram identity.
pub async fn mentioned_people(
    pool: &PgPool,
    org_id: uuid::Uuid,
    text: &str,
) -> Result<std::collections::HashMap<String, TgPerson>> {
    let handles = mentions::parse(text);
    if handles.is_empty() {
        return Ok(Default::default());
    }
    let rows: Vec<(String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "select handle, tg_user_id, tg_username, display_name from members \
         where org_id = $1 and kind = 'human' and handle = any($2) and tg_user_id is not null",
    )
    .bind(org_id)
    .bind(&handles)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(handle, user_id, username, display)| {
            let name = display.unwrap_or_else(|| handle.clone());
            (
                handle,
                TgPerson {
                    user_id,
                    username,
                    name,
                },
            )
        })
        .collect())
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Rewrites `@handle`s for Telegram. People with a username are mentioned by
/// it (Telegram links and notifies). People without one get their name as a
/// `text_mention` entity, which notifies them just the same. Offsets are in
/// UTF-16 code units, as the Bot API requires.
pub fn render_mentions(
    text: &str,
    people: &std::collections::HashMap<String, TgPerson>,
) -> (String, Vec<serde_json::Value>) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut entities = Vec::new();
    let mut units = 0usize;
    let mut i = 0;
    let push = |out: &mut String, units: &mut usize, s: &str| {
        out.push_str(s);
        *units += utf16_len(s);
    };
    while i < chars.len() {
        let c = chars[i];
        let boundary = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c == '@' && boundary {
            let mut j = i + 1;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || matches!(chars[j], '_' | '-' | '.'))
            {
                j += 1;
            }
            // A trailing dot ends the sentence, not the handle.
            let mut end = j;
            while end > i + 1 && chars[end - 1] == '.' {
                end -= 1;
            }
            let raw: String = chars[i + 1..end].iter().collect();
            if let Some(person) = people.get(&raw.to_ascii_lowercase()) {
                match &person.username {
                    Some(u) => push(&mut out, &mut units, &format!("@{u}")),
                    None => {
                        entities.push(json!({
                            "type": "text_mention",
                            "offset": units,
                            "length": utf16_len(&person.name),
                            "user": { "id": person.user_id, "is_bot": false, "first_name": person.name },
                        }));
                        push(&mut out, &mut units, &person.name);
                    }
                }
                i = end;
                continue;
            }
        }
        let mut buf = [0u8; 4];
        push(&mut out, &mut units, c.encode_utf8(&mut buf));
        i += 1;
    }
    (out, entities)
}

/// Drops entities that a truncation cut off.
fn entities_within(entities: &[serde_json::Value], text: &str) -> Vec<serde_json::Value> {
    let total = utf16_len(text) as u64;
    entities
        .iter()
        .filter(|e| {
            let end = e["offset"]
                .as_u64()
                .unwrap_or(u64::MAX)
                .saturating_add(e["length"].as_u64().unwrap_or(0));
            end <= total
        })
        .cloned()
        .collect()
}

fn is_command(text: &str, cmd: &str) -> bool {
    text.split_whitespace()
        .next()
        .is_some_and(|w| w == cmd || w.starts_with(&format!("{cmd}@")))
}

impl Telegram {
    pub async fn connect(token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(70))
            .build()?;
        let mut t = Telegram {
            http,
            base: format!("https://api.telegram.org/bot{token}"),
            bot_id: 0,
            username: String::new(),
            lang: crate::tgcard::Lang::from_env(),
        };
        let me: BotUser = t
            .call("getMe", json!({}))
            .await
            .context("telegram getMe failed")?;
        t.bot_id = me.id;
        t.username = me.username.unwrap_or_default();
        Ok(t)
    }

    async fn call<T: DeserializeOwned>(&self, method: &str, body: serde_json::Value) -> Result<T> {
        // `without_url` keeps the bot token out of error messages and logs.
        let resp = self
            .http
            .post(format!("{}/{method}", self.base))
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("telegram {method}: {}", e.without_url()))?;
        let parsed: TgResponse<T> = resp
            .json()
            .await
            .map_err(|e| anyhow!("telegram {method}: {}", e.without_url()))?;
        match (parsed.ok, parsed.result) {
            (true, Some(r)) => Ok(r),
            _ => Err(anyhow!(
                "telegram {method}: {}",
                parsed.description.unwrap_or_else(|| "unknown error".into())
            )),
        }
    }

    /// Sends plain text (no parse mode, so nothing in it is interpreted as
    /// markup). Returns the Telegram message id.
    pub async fn send(
        &self,
        chat_id: i64,
        text: &str,
        reply_to: Option<i64>,
        thread: Option<i64>,
    ) -> Result<i64> {
        self.send_rich(chat_id, text, &[], reply_to, thread).await
    }

    /// Like `send`, with message entities (used for mentions of people who
    /// have no Telegram username).
    pub async fn send_rich(
        &self,
        chat_id: i64,
        text: &str,
        entities: &[serde_json::Value],
        reply_to: Option<i64>,
        thread: Option<i64>,
    ) -> Result<i64> {
        let text = davra_core::format::truncate_chars(text, MAX_TG_CHARS);
        let mut body = json!({ "chat_id": chat_id, "text": text, "link_preview_options": { "is_disabled": true } });
        let fits = entities_within(entities, &text);
        if !fits.is_empty() {
            body["entities"] = json!(fits);
        }
        if let Some(r) = reply_to {
            body["reply_parameters"] =
                json!({ "message_id": r, "allow_sending_without_reply": true });
        }
        if let Some(t) = thread {
            body["message_thread_id"] = json!(t);
        }
        let sent: SentMessage = self.call("sendMessage", body).await?;
        Ok(sent.message_id)
    }

    /// Sends a message with one inline button per row.
    pub async fn send_with_buttons(
        &self,
        chat_id: i64,
        text: &str,
        entities: &[serde_json::Value],
        buttons: &[(String, String)],
    ) -> Result<i64> {
        let text = davra_core::format::truncate_chars(text, MAX_TG_CHARS);
        let mut body = json!({ "chat_id": chat_id, "text": text, "link_preview_options": { "is_disabled": true } });
        let fits = entities_within(entities, &text);
        if !fits.is_empty() {
            body["entities"] = json!(fits);
        }
        if !buttons.is_empty() {
            let rows: Vec<serde_json::Value> = buttons
                .iter()
                .map(|(label, data)| json!([{ "text": label, "callback_data": data }]))
                .collect();
            body["reply_markup"] = json!({ "inline_keyboard": rows });
        }
        let sent: SentMessage = self.call("sendMessage", body).await?;
        Ok(sent.message_id)
    }

    pub async fn answer_callback(&self, id: &str, text: &str) -> Result<()> {
        let _: bool = self
            .call(
                "answerCallbackQuery",
                json!({ "callback_query_id": id, "text": text }),
            )
            .await?;
        Ok(())
    }

    pub async fn clear_buttons(&self, chat_id: i64, message_id: i64) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                "editMessageReplyMarkup",
                json!({ "chat_id": chat_id, "message_id": message_id, "reply_markup": { "inline_keyboard": [] } }),
            )
            .await?;
        Ok(())
    }

    /// Sends a card: a rich message when `rich`, else (or if Telegram refuses
    /// the rich one) the card's plain text. Returns the Telegram message id.
    pub async fn send_card(
        &self,
        chat_id: i64,
        card: &crate::tgcard::Card,
        rich: bool,
        reply_to: Option<i64>,
        thread: Option<i64>,
        keyboard: Option<serde_json::Value>,
    ) -> Result<i64> {
        let mut common = json!({ "chat_id": chat_id });
        if card.silent {
            common["disable_notification"] = json!(true);
        }
        if let Some(r) = reply_to {
            common["reply_parameters"] =
                json!({ "message_id": r, "allow_sending_without_reply": true });
        }
        if let Some(t) = thread {
            common["message_thread_id"] = json!(t);
        }
        if let Some(k) = &keyboard {
            common["reply_markup"] = k.clone();
        }
        if rich {
            let mut body = common.clone();
            body["rich_message"] = json!({ "html": card.html, "skip_entity_detection": true });
            match self.call::<SentMessage>("sendRichMessage", body).await {
                Ok(sent) => return Ok(sent.message_id),
                Err(e) if card.plain.is_empty() => return Err(e),
                Err(e) => tracing::warn!(error = %e, "rich message refused; sending plain text"),
            }
        }
        let text = davra_core::format::truncate_chars(&card.plain, MAX_TG_CHARS);
        let mut body = common;
        body["text"] = json!(text);
        body["link_preview_options"] = json!({ "is_disabled": true });
        let fits = entities_within(&card.entities, &text);
        if !fits.is_empty() {
            body["entities"] = json!(fits);
        }
        let sent: SentMessage = self.call("sendMessage", body).await?;
        Ok(sent.message_id)
    }

    /// Replaces a rich message's content (the digest grows this way).
    pub async fn edit_card(
        &self,
        chat_id: i64,
        message_id: i64,
        card: &crate::tgcard::Card,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                "editMessageText",
                json!({
                    "chat_id": chat_id,
                    "message_id": message_id,
                    "rich_message": { "html": card.html, "skip_entity_detection": true },
                }),
            )
            .await?;
        Ok(())
    }

    pub async fn run_poller(self: Arc<Self>, pool: PgPool) {
        let mut offset: i64 = db::kv_get(&pool, OFFSET_KEY)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        tracing::info!(bot = %self.username, "telegram long polling started");
        loop {
            let updates: Result<Vec<Update>> = self
                .call(
                    "getUpdates",
                    json!({ "offset": offset, "timeout": 50, "allowed_updates": ["message", "edited_message", "callback_query"] }),
                )
                .await;
            match updates {
                Ok(updates) if updates.is_empty() => {}
                Ok(updates) => {
                    for u in &updates {
                        offset = offset.max(u.update_id + 1);
                        if let Err(e) = self.handle(&pool, u).await {
                            tracing::warn!(error = %e, update = u.update_id, "telegram update failed");
                        }
                    }
                    if let Err(e) = db::kv_set(&pool, OFFSET_KEY, &offset.to_string()).await {
                        tracing::warn!(error = %e, "could not persist telegram offset");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "telegram getUpdates failed, retrying in 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn handle(&self, pool: &PgPool, u: &Update) -> Result<()> {
        if let Some(cb) = &u.callback_query {
            return crate::questions::on_button(pool, self, cb).await;
        }
        let Some(inc) = parse_update(u) else {
            return Ok(());
        };
        if inc.from_id == Some(self.bot_id) {
            return Ok(());
        }
        // "/start link_<code>" from the web app's "Connect Telegram" button.
        if !inc.edited {
            if let Some(code) = inc.text.trim().strip_prefix("/start link_") {
                let reply = match inc.from_id {
                    Some(uid) if !inc.from_is_bot => {
                        crate::webapp::complete_link(
                            pool,
                            code.trim(),
                            uid,
                            inc.from_username.as_deref(),
                        )
                        .await?
                    }
                    _ => "Open this link from your own Telegram account.".to_string(),
                };
                self.send(inc.chat_id, &reply, None, None).await?;
                return Ok(());
            }
        }
        let project = db::project_by_tg_chat(pool, inc.chat_id).await?;

        // `/id` helps admins link chats and users without guessing numbers.
        if !inc.edited && is_command(&inc.text, "/id") {
            let linked = project
                .as_ref()
                .map(|p| p.slug.as_str())
                .unwrap_or("not linked");
            let reply = format!(
                "chat_id: {}\nyour user id: {}\nproject: {}",
                inc.chat_id,
                inc.from_id
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "-".into()),
                linked
            );
            self.send(inc.chat_id, &reply, Some(inc.tg_message_id), inc.thread_id)
                .await?;
            return Ok(());
        }

        let Some(project) = project else {
            return Ok(());
        };
        let (body, _) = redact::redact(&inc.text);
        let mut addressed = mentions::parse(&body);

        if inc.edited {
            return db::update_tg_message(pool, project.id, inc.tg_message_id, &body, &addressed)
                .await;
        }

        let author_id = match inc.from_id {
            Some(id) => db::member_by_tg_user(pool, project.org_id, id).await?,
            None => None,
        };
        // Remember the current Telegram username (people change them).
        if let (Some(aid), Some(u)) = (author_id, inc.from_username.as_deref()) {
            sqlx::query("update members set tg_username = $2 where id = $1 and tg_username is distinct from $2")
                .bind(aid)
                .bind(u.to_ascii_lowercase())
                .execute(pool)
                .await?;
        }
        let reply = match inc.reply_to_tg {
            Some(r) => db::reply_target_by_tg(pool, project.id, r).await?,
            None => None,
        };
        // Replying to an agent's relayed message addresses that agent.
        if let Some(h) = reply.as_ref().and_then(|r| r.author_handle.clone()) {
            if !addressed.contains(&h) {
                addressed.push(h);
            }
        }
        db::insert_message(
            pool,
            &NewMessage {
                org_id: project.org_id,
                project_id: project.id,
                author_id,
                author_name: Some(inc.from_name.clone()),
                kind: "chat",
                topic: inc.topic.clone(),
                body,
                reply_to: reply.map(|r| r.id),
                mentions: addressed,
                source: "telegram",
                tg_message_id: Some(inc.tg_message_id),
                tg_thread_id: inc.thread_id,
                created_at: Some(inc.date),
            },
        )
        .await?;
        if author_id.is_none() && !inc.from_is_bot {
            tracing::info!(project = %project.slug, from = %inc.from_name, "stored message from an unlinked Telegram user");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(v: serde_json::Value) -> Update {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn parses_plain_group_message() {
        let u = update(json!({
            "update_id": 10,
            "message": {
                "message_id": 55, "date": 1_762_950_000,
                "chat": { "id": -1001234, "type": "supergroup", "title": "mosla · AI team" },
                "from": { "id": 42, "is_bot": false, "first_name": "G'ulom", "username": "gulom" },
                "text": "@gulom-ai bugun qilib ber"
            }
        }));
        let inc = parse_update(&u).unwrap();
        assert_eq!(inc.chat_id, -1001234);
        assert_eq!(inc.from_id, Some(42));
        assert_eq!(inc.from_name, "gulom");
        assert_eq!(inc.reply_to_tg, None);
        assert!(!inc.edited);
    }

    #[test]
    fn topic_root_is_not_a_reply() {
        let u = update(json!({
            "update_id": 11,
            "message": {
                "message_id": 60, "message_thread_id": 7, "is_topic_message": true, "date": 1_762_950_000,
                "chat": { "id": -1001234 },
                "from": { "id": 42, "first_name": "G'ulom" },
                "text": "salom",
                "reply_to_message": {
                    "message_id": 7, "date": 1_762_900_000, "chat": { "id": -1001234 },
                    "forum_topic_created": { "name": "Handoffs" }
                }
            }
        }));
        let inc = parse_update(&u).unwrap();
        assert_eq!(inc.reply_to_tg, None);
        assert_eq!(inc.topic.as_deref(), Some("Handoffs"));
        assert_eq!(inc.thread_id, Some(7));
    }

    #[test]
    fn real_reply_inside_topic() {
        let u = update(json!({
            "update_id": 12,
            "message": {
                "message_id": 61, "message_thread_id": 7, "is_topic_message": true, "date": 1_762_950_000,
                "chat": { "id": -1001234 },
                "from": { "id": 42, "first_name": "G'ulom" },
                "text": "zo'r",
                "reply_to_message": { "message_id": 58, "date": 1_762_940_000, "chat": { "id": -1001234 } }
            }
        }));
        assert_eq!(parse_update(&u).unwrap().reply_to_tg, Some(58));
    }

    #[test]
    fn skips_non_text_and_uses_captions() {
        let sticker = update(json!({
            "update_id": 13,
            "message": { "message_id": 62, "date": 1, "chat": { "id": 1 }, "from": { "id": 1, "first_name": "a" } }
        }));
        assert!(parse_update(&sticker).is_none());
        let photo = update(json!({
            "update_id": 14,
            "message": { "message_id": 63, "date": 1, "chat": { "id": 1 }, "caption": "skrinshot" }
        }));
        assert_eq!(parse_update(&photo).unwrap().text, "skrinshot");
    }

    #[test]
    fn mentions_for_telegram() {
        let mut people = std::collections::HashMap::new();
        people.insert(
            "ali_9737".to_string(),
            TgPerson {
                user_id: 542,
                username: Some("ali_9737".into()),
                name: "LionAli".into(),
            },
        );
        people.insert(
            "gulom".to_string(),
            TgPerson {
                user_id: 42,
                username: None,
                name: "G'ulom".into(),
            },
        );
        let (text, ents) =
            render_mentions("🤖 bot\n@gulom va @Ali_9737, ko'ring. @unknown", &people);
        assert_eq!(text, "🤖 bot\nG'ulom va @ali_9737, ko'ring. @unknown");
        assert_eq!(ents.len(), 1);
        // "🤖" is 2 UTF-16 units, so G'ulom starts at 2 + " bot\n".len() = 7.
        assert_eq!(ents[0]["offset"], 7);
        assert_eq!(ents[0]["length"], 6);
        assert_eq!(ents[0]["user"]["id"], 42);
        let (text, _) = render_mentions("rahmat @gulom.", &people);
        assert_eq!(text, "rahmat G'ulom.");
        let (text, ents) = render_mentions("mail a@gulom.uz", &people);
        assert_eq!(text, "mail a@gulom.uz");
        assert!(ents.is_empty());
    }

    #[test]
    fn truncated_entities_are_dropped() {
        let e = vec![
            json!({ "offset": 0, "length": 3 }),
            json!({ "offset": 10, "length": 5 }),
        ];
        assert_eq!(entities_within(&e, "abcdefghij").len(), 1);
    }

    #[test]
    fn commands() {
        assert!(is_command("/id", "/id"));
        assert!(is_command("/id@davra_bot", "/id"));
        assert!(!is_command("/identity", "/id"));
        assert!(!is_command("hello /id", "/id"));
    }
}
