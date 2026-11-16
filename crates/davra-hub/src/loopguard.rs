//! When the loop guard stops agents, the project's Telegram chat gets one
//! prompt per streak with two buttons. "Continue" posts a message from the
//! person who pressed it, addressed to the waiting agents: a human message
//! ends the streak, so the agents may post again. "Stop" changes nothing in
//! the room, so the agents keep waiting for a person to write.

use anyhow::Result;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::LOOP_GUARD;
use crate::db::{self, NewMessage, Project};
use crate::telegram::{CallbackQuery, Telegram};
use crate::AppState;

#[derive(Debug, PartialEq, Eq)]
pub enum Choice {
    Continue,
    Stop,
}

pub fn callback_data(hold: i64, choice: Choice) -> String {
    match choice {
        Choice::Continue => format!("lg:{hold}:go"),
        Choice::Stop => format!("lg:{hold}:stop"),
    }
}

pub fn parse_callback(data: &str) -> Option<(i64, Choice)> {
    let (id, choice) = data.strip_prefix("lg:")?.split_once(':')?;
    let choice = match choice {
        "go" => Choice::Continue,
        "stop" => Choice::Stop,
        _ => return None,
    };
    Some((id.parse().ok()?, choice))
}

/// Records that `agent` was stopped in a streak ending at message `last`, and
/// sends the prompt the first time that streak stops anyone.
pub async fn hold(state: &AppState, p: &Project, agent: &str, last: i64) -> Result<()> {
    let id: Option<i64> = sqlx::query_scalar(
        "insert into loop_holds (org_id, project_id, last_message_id, agents) values ($1, $2, $3, array[$4]) \
         on conflict (project_id, last_message_id) do nothing returning id",
    )
    .bind(p.org_id)
    .bind(p.id)
    .bind(last)
    .bind(agent)
    .fetch_optional(&state.pool)
    .await?;
    let Some(id) = id else {
        // Already prompted for this streak; just remember who else is waiting.
        sqlx::query(
            "update loop_holds set agents = array_append(agents, $3) \
             where project_id = $1 and last_message_id = $2 and not ($3 = any(agents))",
        )
        .bind(p.id)
        .bind(last)
        .bind(agent)
        .execute(&state.pool)
        .await?;
        return Ok(());
    };
    let (Some(tg), Some(chat)) = (&state.telegram, p.tg_chat_id) else {
        return Ok(());
    };
    let card = crate::tgcard::loop_hold(tg.lang, LOOP_GUARD, &[agent.to_string()]);
    let (go, stop) = crate::tgcard::loop_buttons(tg.lang);
    let keyboard = json!({ "inline_keyboard": [[
        { "text": go, "callback_data": callback_data(id, Choice::Continue) },
        { "text": stop, "callback_data": callback_data(id, Choice::Stop) },
    ]] });
    let rich = crate::questions::tg_rich(&state.pool, p.id).await?;
    let tg_id = tg
        .send_card(chat, &card, rich, None, None, Some(keyboard))
        .await?;
    sqlx::query("update loop_holds set tg_message_id = $2 where id = $1")
        .bind(id)
        .bind(tg_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// A press on one of the prompt's buttons. Only a person in the project who
/// may post (not a viewer) decides, and only once.
pub async fn on_button(pool: &PgPool, tg: &Telegram, cb: &CallbackQuery) -> Result<()> {
    let Some((id, choice)) = cb.data.as_deref().and_then(parse_callback) else {
        return tg.answer_callback(&cb.id, "").await;
    };
    let hold: Option<(Uuid, Uuid, Vec<String>)> =
        sqlx::query_as("select org_id, project_id, agents from loop_holds where id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let Some((org_id, project_id, agents)) = hold else {
        return tg
            .answer_callback(&cb.id, "This prompt no longer exists")
            .await;
    };
    let person: Option<(Uuid, String)> = sqlx::query_as(
        "select m.id, m.handle from members m join grants g on g.member_id = m.id and g.project_id = $2 \
         where m.org_id = $1 and m.tg_user_id = $3 and m.kind = 'human' and g.role <> 'viewer'",
    )
    .bind(org_id)
    .bind(project_id)
    .bind(cb.from.id)
    .fetch_optional(pool)
    .await?;
    let Some((member_id, who)) = person else {
        let only = match tg.lang {
            crate::tgcard::Lang::En => "Only people in this project can decide",
            crate::tgcard::Lang::Uz => "Faqat şu loyihadagi odamlar hal qila oladi",
        };
        return tg.answer_callback(&cb.id, only).await;
    };
    let decision = match choice {
        Choice::Continue => "continue",
        Choice::Stop => "stop",
    };
    let decided: Option<i64> = sqlx::query_scalar(
        "update loop_holds set decision = $2, decided_by = $3, decided_at = now() \
         where id = $1 and decision is null returning id",
    )
    .bind(id)
    .bind(decision)
    .bind(member_id)
    .fetch_optional(pool)
    .await?;
    if decided.is_none() {
        if let Some(m) = &cb.message {
            tg.clear_buttons(m.chat.id, m.message_id).await.ok();
        }
        let already = match tg.lang {
            crate::tgcard::Lang::En => "Already decided",
            crate::tgcard::Lang::Uz => "Allaqaçon hal qilingan",
        };
        return tg.answer_callback(&cb.id, already).await;
    }

    let go = choice == Choice::Continue;
    // "Continue" is a real message from the person, so it ends the streak and
    // reaches the agents (as [owner] for the ones this person owns).
    let posted = if go {
        let mentions: String = agents.iter().map(|a| format!("@{a} ")).collect();
        let id = db::insert_message(
            pool,
            &NewMessage {
                org_id,
                project_id,
                author_id: Some(member_id),
                author_name: None,
                kind: "chat",
                topic: None,
                body: format!(
                    "{mentions}Continue: the loop guard stopped you, and @{who} lets you go on."
                ),
                reply_to: None,
                mentions: agents.clone(),
                source: "telegram",
                tg_message_id: None,
                tg_thread_id: None,
                created_at: None,
            },
        )
        .await?;
        id
    } else {
        None
    };

    let answer = match (tg.lang, go) {
        (crate::tgcard::Lang::En, true) => "Agents may go on",
        (crate::tgcard::Lang::En, false) => "Agents stay stopped",
        (crate::tgcard::Lang::Uz, true) => "Agentlar davom etadi",
        (crate::tgcard::Lang::Uz, false) => "Agentlar tóxtab turadi",
    };
    tg.answer_callback(&cb.id, answer).await?;
    if let Some(m) = &cb.message {
        tg.clear_buttons(m.chat.id, m.message_id).await.ok();
        let people = crate::telegram::mentioned_people(pool, org_id, &format!("@{who}")).await?;
        let card = crate::tgcard::loop_decision(tg.lang, go, &who, &people);
        let rich = crate::questions::tg_rich(pool, project_id).await?;
        if let Ok(tg_id) = tg
            .send_card(
                m.chat.id,
                &card,
                rich,
                Some(m.message_id),
                m.message_thread_id,
                None,
            )
            .await
        {
            if let Some(mid) = posted {
                db::set_tg_ids(pool, mid, tg_id, m.message_thread_id).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_roundtrip() {
        assert_eq!(
            parse_callback(&callback_data(7, Choice::Continue)),
            Some((7, Choice::Continue))
        );
        assert_eq!(
            parse_callback(&callback_data(7, Choice::Stop)),
            Some((7, Choice::Stop))
        );
        assert_eq!(parse_callback("lg:7:maybe"), None);
        assert_eq!(parse_callback("q:7:0"), None);
        assert!(callback_data(i64::MAX, Choice::Stop).len() <= 64);
    }

    #[test]
    fn prompt_names_the_waiting_agents() {
        let c = crate::tgcard::loop_hold(crate::tgcard::Lang::Uz, 4, &["frontend-ai".into()]);
        assert!(c.plain.contains("Oxirgi 4 ta xabar"));
        assert!(c.plain.contains("frontend-ai"));
        assert!(c.html.contains("<code>frontend-ai</code>"));
    }
}
