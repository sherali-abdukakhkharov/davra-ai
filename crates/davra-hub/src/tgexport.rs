//! Import of a Telegram Desktop chat export (`result.json`).
//!
//! The Bot API cannot read history, so this is how a room gets the messages
//! written before the bot joined. Telegram message ids are stable per chat,
//! so importing twice, or importing messages the bot already saw, is safe.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use davra_core::{mentions, redact};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::db::{self, ChatProject, NewMessage};

#[derive(Deserialize)]
pub struct Export {
    pub name: Option<String>,
    #[serde(default)]
    pub messages: Vec<ExportMessage>,
}

#[derive(Deserialize)]
pub struct ExportMessage {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub date: Option<String>,
    pub date_unixtime: Option<String>,
    pub from: Option<String>,
    pub from_id: Option<String>,
    #[serde(default)]
    pub text: Value,
    pub reply_to_message_id: Option<i64>,
}

/// Export text is either a string or an array of strings and entity objects.
pub fn flatten_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p {
                Value::String(s) => s.clone(),
                Value::Object(o) => o
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                _ => String::new(),
            })
            .collect(),
        _ => String::new(),
    }
}

/// `"user123456"` -> `123456`. Channels and bots-as-chats are skipped.
pub fn user_id(from_id: &str) -> Option<i64> {
    from_id.strip_prefix("user")?.parse().ok()
}

pub fn message_time(m: &ExportMessage) -> Option<DateTime<Utc>> {
    if let Some(ts) = m
        .date_unixtime
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
    {
        return DateTime::from_timestamp(ts, 0);
    }
    // Older exports only have local time without an offset; treat it as UTC.
    let d = m.date.as_deref()?;
    NaiveDateTime::parse_from_str(d, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|n| n.and_utc())
}

pub struct ImportStats {
    pub seen: usize,
    pub inserted: usize,
    pub skipped: usize,
}

pub async fn import(
    pool: &PgPool,
    project: &ChatProject,
    path: &std::path::Path,
) -> Result<ImportStats> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut export: Export =
        serde_json::from_str(&raw).context("not a Telegram Desktop JSON export")?;
    export.messages.sort_by_key(|m| m.id);
    tracing::info!(chat = ?export.name, messages = export.messages.len(), "importing telegram export");

    let mut stats = ImportStats {
        seen: 0,
        inserted: 0,
        skipped: 0,
    };
    for m in &export.messages {
        stats.seen += 1;
        let text = flatten_text(&m.text);
        if m.kind != "message" || text.trim().is_empty() {
            stats.skipped += 1;
            continue;
        }
        let author_id = match m.from_id.as_deref().and_then(user_id) {
            Some(uid) => db::member_by_tg_user(pool, project.org_id, uid).await?,
            None => None,
        };
        let reply = match m.reply_to_message_id {
            Some(r) => db::reply_target_by_tg(pool, project.id, r).await?,
            None => None,
        };
        let (body, _) = redact::redact(&text);
        let mut addressed = mentions::parse(&body);
        if let Some(h) = reply.as_ref().and_then(|r| r.author_handle.clone()) {
            if !addressed.contains(&h) {
                addressed.push(h);
            }
        }
        let inserted = db::insert_message(
            pool,
            &NewMessage {
                org_id: project.org_id,
                project_id: project.id,
                author_id,
                author_name: m.from.clone(),
                kind: "chat",
                topic: None,
                body,
                reply_to: reply.map(|r| r.id),
                mentions: addressed,
                source: "import",
                tg_message_id: Some(m.id),
                tg_thread_id: None,
                created_at: message_time(m),
            },
        )
        .await?;
        if inserted.is_some() {
            stats.inserted += 1;
        } else {
            stats.skipped += 1;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flattens_entities() {
        let v = json!(["salom ", { "type": "mention", "text": "@backend" }, ", ko'rdingmi?"]);
        assert_eq!(flatten_text(&v), "salom @backend, ko'rdingmi?");
        assert_eq!(flatten_text(&json!("oddiy")), "oddiy");
        assert_eq!(flatten_text(&Value::Null), "");
    }

    #[test]
    fn parses_export() {
        let e: Export = serde_json::from_value(json!({
            "name": "atlas · AI team", "type": "private_supergroup", "id": 1234,
            "messages": [
                { "id": 1, "type": "service", "date": "2026-11-01T09:00:00", "actor": "Backend Dev", "text": "" },
                { "id": 2, "type": "message", "date": "2026-11-01T09:01:00", "date_unixtime": "1761987660",
                  "from": "Backend Dev", "from_id": "user42", "text": "hi" },
                { "id": 3, "type": "message", "date": "2026-11-01T09:02:00", "from": "Lead", "from_id": "user7",
                  "reply_to_message_id": 2, "text": ["ok ", { "type": "bold", "text": "zo'r" }] }
            ]
        }))
        .unwrap();
        assert_eq!(e.messages.len(), 3);
        assert_eq!(user_id(e.messages[1].from_id.as_deref().unwrap()), Some(42));
        assert_eq!(
            message_time(&e.messages[1]).unwrap().timestamp(),
            1_761_987_660
        );
        assert_eq!(
            message_time(&e.messages[2]).unwrap().to_rfc3339(),
            "2026-11-01T09:02:00+00:00"
        );
        assert_eq!(user_id("channel99"), None);
    }
}
